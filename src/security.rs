//! Where a running program came from, as a verdict the UI can show.
//!
//! Linux has no universal code signing, so this asks the package manager
//! instead: which package owns the program file, does the file still match what
//! the package shipped, and did the package come from the distribution. A
//! verdict says where a program came from, not that it is safe.

/// How far a program's file can be traced back to a trusted source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Trust {
    /// Not looked at yet, or the file can't be read.
    #[default]
    Unknown,
    /// A kernel thread: there is no program file.
    Kernel,
    /// Matches a package from the distribution.
    Verified,
    /// Owned by a package that has been updated since the process started.
    Stale,
    /// Comes from a package outside the distribution, or from Flatpak or Snap.
    ThirdParty,
    /// No package owns the file.
    Unpackaged,
    /// The file was deleted while the process runs and no package owns it.
    Deleted,
    /// Owned by a package, but the file differs from the package's copy.
    Modified,
}

impl Trust {
    pub fn label(self) -> &'static str {
        match self {
            Trust::Unknown => "Unknown",
            Trust::Kernel => "Kernel",
            Trust::Verified => "Verified",
            Trust::Stale => "Updated",
            Trust::ThirdParty => "Third-party",
            Trust::Unpackaged => "Unpackaged",
            Trust::Deleted => "Deleted",
            Trust::Modified => "Modified",
        }
    }

    /// Higher is more worth a look. Used to sort and to pick the worst of a group.
    pub fn severity(self) -> u8 {
        match self {
            Trust::Kernel | Trust::Verified => 0,
            Trust::Unknown | Trust::Stale => 1,
            Trust::ThirdParty => 2,
            Trust::Unpackaged => 3,
            Trust::Deleted => 4,
            Trust::Modified => 5,
        }
    }
}

/// Everything learned about one program file.
#[derive(Clone, Debug, Default)]
pub struct Provenance {
    pub trust: Trust,
    /// One sentence saying why `trust` is what it is.
    pub reason: String,
    /// The owning package, if there is one.
    pub package: Option<String>,
    /// SHA-256 of the file, hex. Only worked out for files that aren't verified.
    pub sha256: Option<String>,
}

/// What the package manager says about a file it owns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Owner {
    pub package: String,
    /// The package is signed and from the distribution's own vendor or repository.
    pub distro: bool,
    /// How the file differs from the package's copy, if it does.
    pub modified: Option<String>,
}

/// Turns the facts about a program file into a verdict and a reason.
///
/// `exe` is the path without any " (deleted)" suffix; `deleted` says the suffix
/// was there. `owner` is None when no package owns the file.
pub fn classify(exe: &str, deleted: bool, owner: Option<&Owner>) -> (Trust, String) {
    if let Some(kind) = sandbox(exe) {
        return (
            Trust::ThirdParty,
            format!("Runs from a {kind}; the system package manager doesn't vouch for it."),
        );
    }
    match (owner, deleted) {
        (Some(o), true) => (
            Trust::Stale,
            format!(
                "Package {} replaced this file after the process started. Restart it to run the new version.",
                o.package
            ),
        ),
        (
            Some(Owner {
                package,
                modified: Some(how),
                ..
            }),
            false,
        ) => (
            Trust::Modified,
            format!("Differs from package {package}: {how}."),
        ),
        (Some(o), false) if !o.distro => (
            Trust::ThirdParty,
            format!(
                "Package {} is unsigned or isn't from the distribution's repositories.",
                o.package
            ),
        ),
        (Some(o), false) => (
            Trust::Verified,
            format!("Matches package {} from the distribution.", o.package),
        ),
        (None, true) => (
            Trust::Deleted,
            format!(
                "The file was deleted while the process runs and no package owns it{}.",
                if exe.starts_with("/memfd:") {
                    " (it runs from memory)"
                } else {
                    ""
                }
            ),
        ),
        (None, false) => (
            Trust::Unpackaged,
            format!("No package owns {exe}{}.", location_note(exe)),
        ),
    }
}

/// "Flatpak app" or "Snap" when the path is inside one.
fn sandbox(exe: &str) -> Option<&'static str> {
    if exe.starts_with("/snap/") {
        Some("Snap")
    } else if exe.starts_with("/app/") || exe.contains("/flatpak/") {
        Some("Flatpak app")
    } else {
        None
    }
}

fn location_note(exe: &str) -> &'static str {
    const TEMP: [&str; 3] = ["/tmp/", "/var/tmp/", "/dev/shm/"];
    if TEMP.iter().any(|d| exe.starts_with(d)) {
        "; it runs from a temporary folder"
    } else if exe.starts_with("/home/") {
        "; it lives in a home directory"
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner(distro: bool, modified: Option<&str>) -> Owner {
        Owner {
            package: "bash".into(),
            distro,
            modified: modified.map(String::from),
        }
    }

    #[test]
    fn unmodified_distro_package_is_verified() {
        let (t, _) = classify("/usr/bin/bash", false, Some(&owner(true, None)));
        assert_eq!(t, Trust::Verified);
    }

    #[test]
    fn changed_file_is_modified_even_from_a_distro_package() {
        let (t, why) = classify(
            "/usr/bin/bash",
            false,
            Some(&owner(true, Some("contents differ"))),
        );
        assert_eq!(t, Trust::Modified);
        assert!(why.contains("bash") && why.contains("contents differ"));
    }

    #[test]
    fn foreign_package_is_third_party() {
        let (t, _) = classify("/opt/x/x", false, Some(&owner(false, None)));
        assert_eq!(t, Trust::ThirdParty);
    }

    #[test]
    fn updated_package_file_is_stale_not_deleted() {
        let (t, _) = classify("/usr/bin/bash", true, Some(&owner(true, None)));
        assert_eq!(t, Trust::Stale);
    }

    #[test]
    fn unowned_file_is_unpackaged_and_unowned_deleted_file_is_deleted() {
        let (t, why) = classify("/tmp/x", false, None);
        assert_eq!(t, Trust::Unpackaged);
        assert!(why.contains("temporary"));
        assert_eq!(classify("/memfd:a", true, None).0, Trust::Deleted);
    }

    #[test]
    fn sandboxed_apps_are_third_party_whatever_the_package_manager_says() {
        assert_eq!(classify("/app/bin/x", false, None).0, Trust::ThirdParty);
        assert_eq!(
            classify("/snap/x/1/bin/x", false, None).0,
            Trust::ThirdParty
        );
    }

    #[test]
    fn severity_puts_modified_above_unpackaged_above_verified() {
        assert!(Trust::Modified.severity() > Trust::Unpackaged.severity());
        assert!(Trust::Unpackaged.severity() > Trust::ThirdParty.severity());
        assert!(Trust::ThirdParty.severity() > Trust::Verified.severity());
    }
}
