//! `--check-explain`: a printout for reviewing the texts in `explain.rs`.
//!
//! For every service entry it shows our text next to what this machine says about
//! the same program (man page, systemd description, package summary), then lists the
//! running kernel threads and system processes that only get a general description.

use std::collections::BTreeMap;
use std::process::Command;

use crate::explain::{self, Table};
use crate::snapshot::ProcInfo;

/// Runs a program and returns its trimmed output, or None if it fails or says nothing.
fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !text.is_empty()).then_some(text)
}

/// The first line of `text`, since the tools above can be chatty.
fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

fn or_none(found: Option<String>) -> String {
    found.map_or_else(|| "none found".to_string(), |s| first_line(&s).to_string())
}

fn review_service(name: &str, x: &explain::Explanation) {
    println!("== {name}  [{}]", x.risk.label());
    println!("  ours:        {}", x.what);
    println!("  if killed:   {}", x.if_killed);
    println!("  man page:    {}", or_none(run("whatis", &[name])));
    let unit = format!("{name}.service");
    let described = run(
        "systemctl",
        &["show", "-p", "Description", "--value", &unit],
    )
    // systemctl prints the unit's own name for units that have no file.
    .filter(|d| *d != unit);
    println!("  systemd:     {}", or_none(described));
    let package = run("sh", &["-c", &format!("command -v {name}")])
        .or_else(|| {
            run(
                "sh",
                &[
                    "-c",
                    &format!("ls /usr/sbin/{name} /usr/bin/{name} /usr/libexec/{name} 2>/dev/null"),
                ],
            )
        })
        .and_then(|path| {
            run(
                "rpm",
                &["-qf", "--qf", "%{NAME}: %{SUMMARY}", first_line(&path)],
            )
        });
    println!("  package:     {}", or_none(package));
    println!();
}

/// What the machine is running that only gets a general description.
fn uncovered(procs: &[ProcInfo]) -> BTreeMap<(String, String), usize> {
    let mut found = BTreeMap::new();
    for p in procs {
        let generic = explain::explain(&p.name, &p.group).is_some_and(|x| !x.curated);
        if generic {
            let key = (p.group.clone(), explain::group_key(&p.name).to_string());
            *found.entry(key).or_insert(0) += 1;
        }
    }
    found
}

pub fn print(procs: &[ProcInfo]) {
    let (mut kernel, mut services) = (0, 0);
    for (table, key, x) in explain::entries() {
        match table {
            Table::Service => {
                services += 1;
                review_service(key, x);
            }
            Table::KernelThread | Table::KernelPrefix => kernel += 1,
        }
    }
    println!(
        "{services} service entries reviewed above. {kernel} kernel entries have no local \
         source: check them against https://docs.kernel.org and the kernel source.\n"
    );

    println!("== Running now, with only a general description");
    let found = uncovered(procs);
    if found.is_empty() {
        println!("  nothing: every kernel thread and system process here has an entry");
    }
    for ((group, name), count) in found {
        println!("  {group:<8} {name} ({count})");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(name: &str, group: &str) -> ProcInfo {
        ProcInfo {
            name: name.to_string(),
            group: group.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn counts_unlisted_processes_by_base_name() {
        let procs = [
            proc("mystery/1", "Kernel"),
            proc("mystery/2", "Kernel"),
            proc("kworker/0:1", "Kernel"),
            proc("firefox", "Firefox"),
            proc("some-new-daemon", "System"),
        ];
        let found = uncovered(&procs);
        assert_eq!(found.len(), 2);
        assert_eq!(found[&("Kernel".to_string(), "mystery".to_string())], 2);
        assert_eq!(
            found[&("System".to_string(), "some-new-daemon".to_string())],
            1
        );
    }
}
