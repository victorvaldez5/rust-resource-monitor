//! Asks the system's package manager (rpm, dpkg or pacman) who owns a program
//! file and whether the file still matches the package.
//!
//! Every query runs a command, so this belongs on a worker thread. The parsing
//! lives in plain functions that take the command's output, so it can be tested
//! without any of the package managers installed.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::security::Owner;

/// A package manager query that takes longer than this is abandoned.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a package's verification result is trusted before checking again.
const VERIFY_TTL: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Rpm,
    Dpkg,
    Pacman,
}

/// The package manager this system uses, if it is one of the three we know.
pub fn detect() -> Option<Backend> {
    if Path::new("/var/lib/pacman").is_dir() && have("pacman") {
        Some(Backend::Pacman)
    } else if Path::new("/var/lib/dpkg/status").is_file() && have("dpkg") {
        Some(Backend::Dpkg)
    } else if have("rpm") {
        Some(Backend::Rpm)
    } else {
        None
    }
}

fn have(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|d| d.join(program).is_file()))
}

/// What is known about one installed package.
struct PackageInfo {
    /// Files that differ from the package, with how they differ.
    modified: HashMap<String, String>,
    distro: bool,
    checked: Instant,
}

/// Remembers per-package answers, because one package owns many program files.
#[derive(Default)]
pub struct Cache {
    packages: HashMap<String, PackageInfo>,
    /// Pacman only: installed packages that aren't in any sync repository.
    foreign: Option<HashSet<String>>,
    os: Option<OsInfo>,
}

impl Backend {
    /// The package that owns `path` and how the file compares to it, or None
    /// when no package owns it.
    pub fn owner(self, path: &str, cache: &mut Cache) -> Option<Owner> {
        let package = self.package_of(path)?;
        let stale = cache
            .packages
            .get(&package)
            .is_none_or(|p| p.checked.elapsed() > VERIFY_TTL);
        if stale {
            let info = self.inspect(&package, cache);
            cache.packages.insert(package.clone(), info);
        }
        let info = &cache.packages[&package];
        Some(Owner {
            distro: info.distro,
            modified: info.modified.get(path).cloned(),
            package,
        })
    }

    fn package_of(self, path: &str) -> Option<String> {
        match self {
            Backend::Rpm => {
                let out = run("rpm", &["-qf", "--qf", "%{NAME}\n", path])?;
                parse_rpm_owner(&out)
            }
            Backend::Pacman => {
                let out = run("pacman", &["-Qoq", path])?;
                parse_pacman_owner(&out)
            }
            Backend::Dpkg => dpkg_candidates(path).iter().find_map(|p| {
                let out = run("dpkg", &["-S", p])?;
                parse_dpkg_owner(&out)
            }),
        }
    }

    fn inspect(self, package: &str, cache: &mut Cache) -> PackageInfo {
        let (modified, distro) = match self {
            Backend::Rpm => {
                let modified = run("rpm", &["-V", package])
                    .map(|o| parse_verify(&o))
                    .unwrap_or_default();
                let os = cache.os.get_or_insert_with(OsInfo::read);
                let distro = run(
                    "rpm",
                    &[
                        "-q",
                        "--qf",
                        "%{VENDOR}\t%{RSAHEADER:pgpsig}\t%{DSAHEADER:pgpsig}\t%{SIGPGP:pgpsig}\t%{SIGGPG:pgpsig}\n",
                        package,
                    ],
                )
                .is_some_and(|o| rpm_is_distro(&o, os));
                (modified, distro)
            }
            Backend::Dpkg => {
                let modified = run("dpkg", &["--verify", package])
                    .map(|o| parse_verify(&o))
                    .unwrap_or_default();
                // Without apt there is nothing to tell a repository from a local .deb.
                let distro =
                    run("apt-cache", &["policy", package]).is_none_or(|o| apt_is_distro(&o));
                (modified, distro)
            }
            Backend::Pacman => {
                let modified = run("pacman", &["-Qkk", package])
                    .map(|o| parse_pacman_check(&o))
                    .unwrap_or_default();
                let foreign = cache.foreign.get_or_insert_with(|| {
                    run("pacman", &["-Qmq"])
                        .map(|o| o.lines().map(|l| l.trim().to_string()).collect())
                        .unwrap_or_default()
                });
                (modified, !foreign.contains(package))
            }
        };
        PackageInfo {
            modified,
            distro,
            checked: Instant::now(),
        }
    }
}

/// Runs a command with the C locale and returns what it printed to stdout and
/// stderr, or None if it couldn't start, ran too long, or printed nothing.
/// The exit status is ignored: the verify commands exit non-zero exactly when
/// they have something to report.
fn run(program: &str, args: &[&str]) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        readers.push(drain(out));
    }
    if let Some(err) = child.stderr.take() {
        readers.push(drain(err));
    }
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let text: String = readers
        .into_iter()
        .filter_map(|r| r.join().ok())
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .collect();
    (!text.trim().is_empty()).then_some(text)
}

/// Reads a pipe on its own thread so a chatty command can't fill it and stall.
fn drain(mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf);
        buf
    })
}

// ---- Parsing -------------------------------------------------------------

fn parse_rpm_owner(out: &str) -> Option<String> {
    let line = out.lines().next()?.trim();
    // "file /x is not owned by any package" when it isn't.
    (!line.is_empty() && !line.contains(' ')).then(|| line.to_string())
}

fn parse_pacman_owner(out: &str) -> Option<String> {
    let line = out.lines().next()?.trim();
    // Errors start with "error:" and contain spaces; a package name has neither.
    (!line.is_empty() && !line.contains(' ') && !line.starts_with("error"))
        .then(|| line.to_string())
}

/// `dpkg -S` prints "pkg[:arch]: /path", with several packages comma separated.
/// A package is returned with its architecture if it has one, as `dpkg --verify` wants.
fn parse_dpkg_owner(out: &str) -> Option<String> {
    out.lines()
        .filter(|l| !l.starts_with("diversion by"))
        .find_map(|line| {
            let (pkgs, _path) = line.split_once(": /")?;
            let first = pkgs.split(", ").next()?.trim();
            (!first.is_empty() && !first.contains(' ')).then(|| first.to_string())
        })
}

/// On merged-/usr systems dpkg may list a file under its old /bin or /lib path.
fn dpkg_candidates(path: &str) -> Vec<String> {
    let mut out = vec![path.to_string()];
    if let Some(rest) = path.strip_prefix("/usr/")
        && ["bin/", "sbin/", "lib/", "lib32/", "lib64/"]
            .iter()
            .any(|d| rest.starts_with(d))
    {
        out.push(format!("/{rest}"));
    }
    out
}

/// Reads `rpm -V` or `dpkg --verify` output: a line of nine flag characters, an
/// optional one-letter file type, then the path. Returns the files whose size or
/// contents differ, with a description. A flag of `?` means "not checked".
fn parse_verify(out: &str) -> HashMap<String, String> {
    let mut modified = HashMap::new();
    for line in out.lines() {
        let Some((flags, rest)) = line.trim_start().split_once(char::is_whitespace) else {
            continue;
        };
        if flags.len() != 9 {
            continue; // "missing" and similar
        }
        let rest = rest.trim_start();
        let path = match rest.split_once(' ') {
            Some((kind, path)) if kind.len() == 1 && "cdglr".contains(kind) => path.trim_start(),
            _ => rest,
        };
        if !path.starts_with('/') {
            continue;
        }
        let mut how = Vec::new();
        if flags.contains('S') {
            how.push("size differs");
        }
        if flags.contains('5') {
            how.push("contents differ");
        }
        if !how.is_empty() {
            modified.insert(path.to_string(), how.join(" and "));
        }
    }
    modified
}

/// Reads `pacman -Qkk` warnings such as
/// `warning: bash: /usr/bin/bash (SHA256 checksum mismatch)`.
fn parse_pacman_check(out: &str) -> HashMap<String, String> {
    let mut modified = HashMap::new();
    for line in out.lines() {
        let Some((before, why)) = line
            .trim_end()
            .strip_suffix(')')
            .and_then(|l| l.rsplit_once(" ("))
        else {
            continue;
        };
        let how = if why.contains("checksum mismatch") {
            "contents differ"
        } else if why == "Size mismatch" {
            "size differs"
        } else {
            continue;
        };
        if let Some((_, path)) = before.split_once(": /") {
            modified.insert(format!("/{path}"), how.to_string());
        }
    }
    modified
}

/// What `/etc/os-release` says about the distribution, lowercased.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct OsInfo {
    ids: Vec<String>,
    name: String,
}

impl OsInfo {
    fn read() -> Self {
        std::fs::read_to_string("/etc/os-release")
            .map(|s| Self::parse(&s))
            .unwrap_or_default()
    }

    fn parse(text: &str) -> Self {
        let mut os = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"').to_lowercase();
            match key {
                "ID" | "ID_LIKE" => os.ids.extend(value.split_whitespace().map(String::from)),
                "NAME" => os.name = value,
                _ => {}
            }
        }
        os
    }

    /// Whether a package vendor string is this distribution (or one it derives from).
    fn matches_vendor(&self, vendor: &str) -> bool {
        /// Distribution ids whose packages' vendor is spelled differently.
        const ALIASES: [(&str, &str); 4] = [
            ("rhel", "red hat"),
            ("sles", "suse"),
            ("opensuse-leap", "suse"),
            ("ol", "oracle"),
        ];
        let vendor = vendor.to_lowercase();
        if vendor.is_empty() || vendor == "(none)" {
            return false;
        }
        let words = self
            .name
            .split_whitespace()
            .filter(|w| w.len() >= 4 && *w != "linux" && *w != "gnu/linux");
        self.ids
            .iter()
            .filter(|id| id.len() >= 3)
            .map(String::as_str)
            .chain(words)
            .any(|t| vendor.contains(t))
            || ALIASES
                .iter()
                .any(|(id, alias)| self.ids.iter().any(|i| i == id) && vendor.contains(alias))
    }
}

/// Reads the tab separated `rpm -q` line: vendor, then the signature fields.
fn rpm_is_distro(out: &str, os: &OsInfo) -> bool {
    let Some(line) = out.lines().next() else {
        return false;
    };
    let mut fields = line.split('\t');
    let vendor = fields.next().unwrap_or("");
    let signed = fields.any(|f| !f.is_empty() && f != "(none)");
    signed && os.matches_vendor(vendor)
}

/// Reads `apt-cache policy <pkg>`. The installed version's block lists where it
/// can be downloaded from; a version that only comes from `/var/lib/dpkg/status`
/// was installed from a local file, and one from a PPA is outside the distribution.
fn apt_is_distro(out: &str) -> bool {
    let mut in_installed = false;
    for line in out.lines() {
        if line.trim_start().starts_with("***") {
            in_installed = true;
            continue;
        }
        if !in_installed {
            continue;
        }
        if !line.contains('/') {
            break; // the next version's heading
        }
        let source = line.trim();
        if source.contains("/var/lib/dpkg/status") {
            continue;
        }
        if !source.contains("ppa.") && !source.contains("launchpad") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpm_owner_rejects_the_not_owned_message() {
        assert_eq!(parse_rpm_owner("bash\n"), Some("bash".into()));
        assert_eq!(
            parse_rpm_owner("file /tmp/x is not owned by any package\n"),
            None
        );
    }

    #[test]
    fn pacman_owner_rejects_errors() {
        assert_eq!(parse_pacman_owner("bash\n"), Some("bash".into()));
        assert_eq!(parse_pacman_owner("error: No package owns /tmp/x\n"), None);
    }

    #[test]
    fn dpkg_owner_takes_the_first_package_and_skips_diversions() {
        assert_eq!(
            parse_dpkg_owner("libc6:amd64: /usr/lib/x86_64-linux-gnu/libc.so.6\n"),
            Some("libc6:amd64".into())
        );
        assert_eq!(
            parse_dpkg_owner("diversion by foo from: /usr/bin/x\nbar, baz: /usr/bin/x\n"),
            Some("bar".into())
        );
        assert_eq!(
            parse_dpkg_owner("dpkg-query: no path found matching pattern /x\n"),
            None
        );
    }

    #[test]
    fn dpkg_tries_the_pre_merge_path_too() {
        assert_eq!(dpkg_candidates("/usr/bin/ls"), ["/usr/bin/ls", "/bin/ls"]);
        assert_eq!(dpkg_candidates("/opt/x"), ["/opt/x"]);
    }

    #[test]
    fn verify_reports_only_size_and_content_changes() {
        let out = "\
S.5....T.  c /etc/bash.conf
.M.......    /usr/bin/mode-only
..5......    /usr/bin/bash
missing   d /usr/share/doc/bash/README
.......T.    /usr/bin/touched
??5??????  c /etc/dpkg-style
";
        let m = parse_verify(out);
        assert_eq!(m["/etc/bash.conf"], "size differs and contents differ");
        assert_eq!(m["/usr/bin/bash"], "contents differ");
        assert_eq!(m["/etc/dpkg-style"], "contents differ");
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn pacman_check_reads_checksum_and_size_warnings() {
        let out = "\
warning: bash: /usr/bin/bash (SHA256 checksum mismatch)
warning: bash: /usr/bin/sh (Size mismatch)
warning: bash: /etc/skel/.bashrc (Modification time mismatch)
bash: 1200 total files, 2 altered files
";
        let m = parse_pacman_check(out);
        assert_eq!(m["/usr/bin/bash"], "contents differ");
        assert_eq!(m["/usr/bin/sh"], "size differs");
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn fedora_vendor_matches_only_when_signed() {
        let os = OsInfo::parse("NAME=\"Fedora Linux\"\nID=fedora\n");
        let signed = "Fedora Project\tRSA/SHA256, Tue 20 Jan 2026, Key ID dbfcf71c6d9f90a6\t(none)\t(none)\t(none)\n";
        let unsigned = "Fedora Project\t(none)\t(none)\t(none)\t(none)\n";
        let other = "Some Vendor\tRSA/SHA256, Key ID 1234\t(none)\t(none)\t(none)\n";
        assert!(rpm_is_distro(signed, &os));
        assert!(!rpm_is_distro(unsigned, &os));
        assert!(!rpm_is_distro(other, &os));
        assert!(!rpm_is_distro(
            "(none)\tRSA/SHA256, Key ID 1\t(none)\t(none)\t(none)\n",
            &os
        ));
    }

    #[test]
    fn derived_distros_match_their_own_and_their_parents_vendor() {
        let rocky =
            OsInfo::parse("NAME=\"Rocky Linux\"\nID=\"rocky\"\nID_LIKE=\"rhel centos fedora\"\n");
        assert!(rocky.matches_vendor("Rocky Enterprise Software Foundation"));
        assert!(rocky.matches_vendor("Red Hat, Inc."));
        assert!(!rocky.matches_vendor("Microsoft Corporation"));
    }

    #[test]
    fn apt_policy_tells_repositories_from_local_and_ppa_installs() {
        let repo = "\
bash:
  Installed: 5.2-1
  Candidate: 5.2-1
  Version table:
 *** 5.2-1 500
        500 http://deb.debian.org/debian bookworm/main amd64 Packages
        100 /var/lib/dpkg/status
     5.1-1 500
        500 http://deb.debian.org/debian bullseye/main amd64 Packages
";
        let local = "\
foo:
  Installed: 1.0
  Version table:
 *** 1.0 100
        100 /var/lib/dpkg/status
";
        let ppa = "\
foo:
  Installed: 1.0
  Version table:
 *** 1.0 500
        500 https://ppa.launchpadcontent.net/x/y/ubuntu jammy/main amd64 Packages
        100 /var/lib/dpkg/status
";
        assert!(apt_is_distro(repo));
        assert!(!apt_is_distro(local));
        assert!(!apt_is_distro(ppa));
    }
}
