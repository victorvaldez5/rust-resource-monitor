//! Works out which application or part of the system each process belongs to.

use std::collections::{HashMap, HashSet};

use sysinfo::{Pid, Process, System};

use super::{procs, temps};

const KERNEL: &str = "Kernel";
const SYSTEM: &str = "System";
const SESSION: &str = "User session";
const KDE: &str = "KDE";

/// Every kernel thread is kthreadd or one of its children.
const KTHREADD: u32 = 2;
const SHELLS: &[&str] = &["bash", "zsh", "fish", "sh", "dash", "ksh", "tcsh", "nu", "xonsh"];
/// Programs that only exist to start another one as a different user.
const WRAPPERS: &[&str] = &["sudo", "su", "doas"];
/// Unit and program names that start like this are parts of the KDE desktop.
const KDE_PREFIXES: &[&str] =
    &["akonadi", "baloo", "kcm_", "kded", "ksecret", "ksm", "kunifiedpush", "kwin"];
/// Guards the walk up the parent chain against a malformed process table.
const MAX_DEPTH: usize = 64;

#[derive(Clone, Default)]
pub struct Owner {
    /// Short label such as "Kernel", "KDE" or "Zen".
    pub group: String,
    /// The systemd unit the process runs in, if any.
    pub unit: String,
}

pub struct Owners(HashMap<Pid, Owner>);

impl Owners {
    pub fn new(sys: &System) -> Self {
        // pid -> cgroup path, for everything that isn't a kernel thread.
        let cgroups: HashMap<Pid, String> = sys
            .processes()
            .iter()
            .filter(|(pid, p)| !is_kernel(**pid, p))
            .map(|(pid, _)| (*pid, cgroup(*pid).unwrap_or_default()))
            .collect();
        let shells: HashSet<Pid> = sys
            .processes()
            .iter()
            .filter(|(pid, p)| is_interactive_shell(**pid, p))
            .map(|(pid, _)| *pid)
            .collect();

        let owners = sys.processes().iter().map(|(pid, p)| {
            let Some(cgroup) = cgroups.get(pid) else {
                return (*pid, Owner { group: KERNEL.to_string(), unit: String::new() });
            };
            let unit = unit(cgroup).to_string();
            let group = started_from_shell(sys, *pid, p, &cgroups, &shells)
                .unwrap_or_else(|| group_of_unit(cgroup, &unit, &procs::display_name(p)));
            (*pid, Owner { group, unit })
        });
        Self(owners.collect())
    }

    pub fn get(&self, pid: Pid) -> Owner {
        self.0.get(&pid).cloned().unwrap_or_default()
    }
}

fn is_kernel(pid: Pid, p: &Process) -> bool {
    pid.as_u32() == KTHREADD || p.parent().is_some_and(|parent| parent.as_u32() == KTHREADD)
}

/// The cgroup v2 path from a line like `0::/user.slice/.../app-foo.scope`.
fn cgroup(pid: Pid) -> Option<String> {
    let text = temps::read_string(format!("/proc/{pid}/cgroup"))?;
    text.lines().find_map(|l| l.strip_prefix("0::")).map(str::to_string)
}

/// The innermost systemd unit in a cgroup path, or "" if there is none.
fn unit(cgroup: &str) -> &str {
    cgroup
        .rsplit('/')
        .find(|part| part.ends_with(".service") || part.ends_with(".scope"))
        .unwrap_or("")
}

/// A shell someone is typing into: it leads its session and has a terminal.
/// Shells that programs spawn to run a command have no terminal of their own.
fn is_interactive_shell(pid: Pid, p: &Process) -> bool {
    SHELLS.contains(&procs::display_name(p).as_str())
        && p.session_id() == Some(pid)
        && has_terminal(pid)
}

fn has_terminal(pid: Pid) -> bool {
    // /proc/<pid>/stat is "pid (comm) state ppid pgrp session tty_nr ...", and
    // comm may itself contain spaces and brackets, so count from the last ')'.
    temps::read_string(format!("/proc/{pid}/stat"))
        .and_then(|stat| {
            let tty = stat.rsplit_once(')')?.1.split_whitespace().nth(4)?;
            Some(tty != "0")
        })
        .unwrap_or(false)
}

/// For a program run from a terminal, the name of the command that was typed:
/// the ancestor sitting directly under the interactive shell. Without this,
/// everything started in a terminal would be credited to the terminal.
fn started_from_shell(
    sys: &System,
    pid: Pid,
    p: &Process,
    cgroups: &HashMap<Pid, String>,
    shells: &HashSet<Pid>,
) -> Option<String> {
    let cgroup = cgroups.get(&pid)?;
    let mut root = p;
    let mut current = p;
    for _ in 0..MAX_DEPTH {
        let parent_pid = current.parent()?;
        // A program that moved into its own unit, such as a Flatpak app,
        // is described better by that unit.
        if cgroups.get(&parent_pid) != Some(cgroup) {
            return None;
        }
        if shells.contains(&parent_pid) {
            let name = procs::display_name(root);
            return Some(if is_kde(&name) { KDE.to_string() } else { capitalise(&name) });
        }
        current = sys.process(parent_pid)?;
        if !WRAPPERS.contains(&procs::display_name(current).as_str()) {
            root = current;
        }
    }
    None
}

fn group_of_unit(cgroup: &str, unit: &str, name: &str) -> String {
    if is_kde(unit) {
        return KDE.to_string();
    }
    if !cgroup.starts_with("/user.slice") {
        return SYSTEM.to_string();
    }
    // Desktops start each application in its own unit under app.slice, named
    // "app-..." or, for ones that set themselves up, a scope. Other services
    // there are background helpers of the session.
    let is_app = unit.starts_with("app-") || unit.ends_with(".scope");
    if cgroup.contains("/app.slice/") && is_app {
        return app_name(unit);
    }
    if is_kde(name) { KDE.to_string() } else { SESSION.to_string() }
}

fn is_kde(name: &str) -> bool {
    let name = name.to_lowercase();
    let name = name.strip_prefix("app-").unwrap_or(&name);
    name.contains("kde") || name.contains("plasma") || KDE_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// Turns a unit name into an application name:
/// `app-flatpak-app.zen_browser.zen-4242.scope` -> "Zen", `kitty-9388-0.scope` -> "Kitty".
fn app_name(unit: &str) -> String {
    let name = unit.rsplit_once('.').map_or(unit, |(name, _)| name);
    // The part after '@' only tells instances of the same application apart.
    let name = name.split('@').next().unwrap_or(name);
    // systemd writes a '-' inside a name as "\x2d".
    let name = name.replace("\\x2d", "-");
    let mut name = name.as_str();
    name = name.strip_prefix("app-").unwrap_or(name);
    name = name.strip_prefix("flatpak-").unwrap_or(name);
    // Drop trailing "-<pid>" or "-<random id>" parts.
    while let Some((head, tail)) = name.rsplit_once('-') {
        let is_id = tail.chars().all(|c| c.is_ascii_hexdigit()) && tail.chars().any(|c| c.is_ascii_digit());
        if !is_id || head.is_empty() {
            break;
        }
        name = head;
    }
    // "com.example.App" style ids: the last part is the application.
    capitalise(name.rsplit('.').next().unwrap_or(name))
}

fn capitalise(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
