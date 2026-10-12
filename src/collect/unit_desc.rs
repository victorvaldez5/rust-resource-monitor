//! One-line descriptions of systemd units, read from their unit files.

use std::collections::HashMap;
use std::path::PathBuf;

use super::temps;

/// Where systemd looks for unit files, system ones first.
const SYSTEM_DIRS: &[&str] = &[
    "/etc/systemd/system",
    "/run/systemd/system",
    "/usr/lib/systemd/system",
    "/lib/systemd/system",
    "/etc/systemd/user",
    "/usr/lib/systemd/user",
];

/// Looks each unit up once; the files don't change while the monitor runs.
#[derive(Default)]
pub struct UnitDescriptions(HashMap<String, String>);

impl UnitDescriptions {
    /// The `Description=` of a unit such as `NetworkManager.service`,
    /// or "" if it has none or its file can't be found.
    pub fn get(&mut self, unit: &str) -> String {
        if !unit.ends_with(".service") {
            return String::new();
        }
        self.0
            .entry(unit.to_string())
            .or_insert_with(|| read(unit))
            .clone()
    }
}

fn read(unit: &str) -> String {
    let user_dir = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/systemd/user"));
    let dirs = user_dir
        .into_iter()
        .chain(SYSTEM_DIRS.iter().map(PathBuf::from));
    for dir in dirs {
        for name in file_names(unit) {
            if let Some(text) = temps::read_string(dir.join(&name)) {
                return parse_description(&text).unwrap_or_default();
            }
        }
    }
    String::new()
}

/// The unit file names to try: `foo@bar.service` is an instance of `foo@.service`.
fn file_names(unit: &str) -> Vec<String> {
    let mut names = vec![unit.to_string()];
    if let Some((template, rest)) = unit.split_once('@')
        && let Some((_, suffix)) = rest.rsplit_once('.')
    {
        names.push(format!("{template}@.{suffix}"));
    }
    names
}

/// The text of the `Description=` line in the `[Unit]` section.
fn parse_description(text: &str) -> Option<String> {
    let mut in_unit = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_unit = line == "[Unit]";
        } else if in_unit && let Some(d) = line.strip_prefix("Description=") {
            let d = d.trim();
            return (!d.is_empty()).then(|| d.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_description_from_the_unit_section_only() {
        let text = "[Install]\nDescription=wrong\n[Unit]\n# note\nDescription= Network Manager \nAfter=x\n";
        assert_eq!(parse_description(text).as_deref(), Some("Network Manager"));
        assert_eq!(parse_description("[Service]\nExecStart=/bin/x"), None);
        assert_eq!(parse_description("[Unit]\nDescription=\n"), None);
    }

    #[test]
    fn instances_fall_back_to_their_template() {
        assert_eq!(
            file_names("sshd@5.service"),
            ["sshd@5.service", "sshd@.service"]
        );
        assert_eq!(file_names("cron.service"), ["cron.service"]);
    }

    #[test]
    fn only_services_are_described() {
        assert_eq!(UnitDescriptions::default().get("app-foo.scope"), "");
    }
}
