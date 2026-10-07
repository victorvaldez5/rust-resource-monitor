//! Plain-language descriptions of system and kernel processes.

/// How bad it is to stop a process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Risk {
    /// Nothing noticeable happens, or it restarts itself.
    Low,
    /// Some feature stops working until it is restarted.
    Medium,
    /// The session, or the whole machine, can go down.
    High,
}

impl Risk {
    pub fn label(self) -> &'static str {
        match self {
            Risk::Low => "Low risk",
            Risk::Medium => "Some risk",
            Risk::High => "Dangerous",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Explanation {
    /// What the process is for.
    pub what: &'static str,
    /// What happens if it is stopped.
    pub if_killed: &'static str,
    pub risk: Risk,
}

const fn e(what: &'static str, if_killed: &'static str, risk: Risk) -> Explanation {
    Explanation {
        what,
        if_killed,
        risk,
    }
}

const KERNEL_GENERIC: Explanation = e(
    "A thread inside the Linux kernel, the core of the operating system. The kernel \
     uses these to do its own housekeeping in the background, such as talking to \
     hardware or tidying memory. They have no program file and show no window.",
    "You can't. The kernel ignores kill signals sent to its own threads, so the \
     attempt is refused or does nothing. They are not apps and are not worth worrying about.",
    Risk::Low,
);

const SYSTEM_GENERIC: Explanation = e(
    "A background service started by the system rather than by you, usually when \
     the computer booted.",
    "Unknown. Look up its name before stopping it. Most services are restarted by \
     systemd automatically, but some features may stop until then.",
    Risk::Medium,
);

/// Kernel threads: key is the name with any "/3" or ":1" suffix removed.
const KERNEL_THREADS: &[(&str, Explanation)] = &[
    (
        "kthreadd",
        e(
            "The parent of every other kernel thread. It creates them on request.",
            "Can't be killed. If it could be, the kernel could no longer start new background work and the system would soon lock up.",
            Risk::High,
        ),
    ),
    (
        "kworker",
        e(
            "A general-purpose worker. The kernel hands it small jobs to run in the background, such as finishing disk writes, handling a network packet or updating the screen. There are many, one set per CPU core.",
            "Can't be killed. If one is using a lot of CPU, a driver or piece of hardware is probably busy or misbehaving. Check which device is active.",
            Risk::Low,
        ),
    ),
    (
        "ksoftirqd",
        e(
            "Handles deferred work from hardware interrupts, such as network traffic, when it is too much to deal with right away. One per CPU core.",
            "Can't be killed. High CPU here usually means heavy network or disk traffic.",
            Risk::Low,
        ),
    ),
    (
        "migration",
        e(
            "Moves running programs from one CPU core to another so the load stays balanced. One per core.",
            "Can't be killed. Without it, work could pile up on a single core.",
            Risk::Low,
        ),
    ),
    (
        "rcu_preempt",
        e(
            "Part of RCU, the way the kernel safely frees data that many threads may still be reading.",
            "Can't be killed. Memory would stop being reclaimed correctly.",
            Risk::Low,
        ),
    ),
    (
        "rcu_sched",
        e(
            "Part of RCU, the way the kernel safely frees data that many threads may still be reading.",
            "Can't be killed. Memory would stop being reclaimed correctly.",
            Risk::Low,
        ),
    ),
    (
        "rcu_gp",
        e(
            "Part of RCU, the way the kernel safely frees data that many threads may still be reading.",
            "Can't be killed. Memory would stop being reclaimed correctly.",
            Risk::Low,
        ),
    ),
    (
        "rcu_tasks_kthread",
        e(
            "Part of RCU, the way the kernel safely frees data that many threads may still be reading.",
            "Can't be killed.",
            Risk::Low,
        ),
    ),
    (
        "kcompactd0",
        e(
            "Defragments RAM: it moves memory around so large free blocks are available.",
            "Can't be killed. Without it, big memory requests could get slower.",
            Risk::Low,
        ),
    ),
    (
        "kswapd0",
        e(
            "Frees up RAM when it runs low, by pushing rarely used data out to swap or dropping cached files. It is busy when memory is nearly full.",
            "Can't be killed. If it is using a lot of CPU, you are short on RAM; close some applications.",
            Risk::Low,
        ),
    ),
    (
        "khugepaged",
        e(
            "Merges small memory pages into huge ones, which makes some programs faster.",
            "Can't be killed. Only a small speed gain would be lost.",
            Risk::Low,
        ),
    ),
    (
        "ksmd",
        e(
            "Kernel Samepage Merging: finds identical memory in different programs, such as virtual machines, and stores it once.",
            "Can't be killed. Memory use of virtual machines could go up.",
            Risk::Low,
        ),
    ),
    (
        "kdevtmpfs",
        e(
            "Creates the device files in /dev when hardware appears.",
            "Can't be killed. New devices would not show up.",
            Risk::Medium,
        ),
    ),
    (
        "kauditd",
        e(
            "Passes security audit messages from the kernel to the logs.",
            "Can't be killed. Audit logging would stall.",
            Risk::Low,
        ),
    ),
    (
        "oom_reaper",
        e(
            "Cleans up after the kernel's out-of-memory killer ends a program to rescue the system.",
            "Can't be killed. Memory taken by programs killed when RAM runs out would be freed more slowly.",
            Risk::Low,
        ),
    ),
    (
        "writeback",
        e(
            "Writes changed data from memory to disk in the background.",
            "Can't be killed. Without it, saved files could be lost on a crash.",
            Risk::Medium,
        ),
    ),
    (
        "jbd2",
        e(
            "The journal of an ext4 drive: it records changes first so the filesystem survives a power cut.",
            "Can't be killed. Without it the drive could be corrupted after a crash.",
            Risk::High,
        ),
    ),
    (
        "irq",
        e(
            "Handles the signals one piece of hardware sends to get the CPU's attention.",
            "Can't be killed. That device would stop responding.",
            Risk::Medium,
        ),
    ),
    (
        "cpuhp",
        e(
            "Turns CPU cores on and off, for example when the computer sleeps.",
            "Can't be killed. Sleeping and waking could fail.",
            Risk::Low,
        ),
    ),
    (
        "idle_inject",
        e(
            "Forces a CPU core to rest briefly so it stays cool.",
            "Can't be killed. The CPU could run hotter.",
            Risk::Low,
        ),
    ),
    (
        "watchdog",
        e(
            "Checks that each CPU core is still responding, and reports if one is stuck.",
            "Can't be killed. A frozen core would go unreported.",
            Risk::Low,
        ),
    ),
    (
        "kblockd",
        e(
            "Handles requests to read and write storage drives.",
            "Can't be killed. Disk access would stall.",
            Risk::High,
        ),
    ),
    (
        "md",
        e(
            "Manages software RAID, which combines several drives into one.",
            "Can't be killed. The array could stop working.",
            Risk::High,
        ),
    ),
    (
        "psimon",
        e(
            "Measures how much programs are waiting on CPU, memory or disk. The desktop uses it to spot trouble.",
            "Can't be killed. Memory pressure figures would stop updating.",
            Risk::Low,
        ),
    ),
    (
        "napi",
        e(
            "Handles incoming network packets for one network adapter.",
            "Can't be killed. That adapter would stop receiving.",
            Risk::Medium,
        ),
    ),
];

/// Kernel threads named after the part of the kernel they belong to.
const KERNEL_PREFIXES: &[(&str, Explanation)] = &[
    (
        "rcu_",
        e(
            "Part of RCU, the way the kernel safely frees data that many threads may still be reading.",
            "Can't be killed. Memory would stop being reclaimed correctly.",
            Risk::Low,
        ),
    ),
    (
        "dmcrypt",
        e(
            "Encrypts and decrypts data going to and from an encrypted drive.",
            "Can't be killed. The encrypted drive would stop working.",
            Risk::High,
        ),
    ),
    (
        "nvidia",
        e(
            "Runs work for the NVIDIA graphics driver.",
            "Can't be killed. The graphics driver could stop working.",
            Risk::Medium,
        ),
    ),
    (
        "nv_",
        e(
            "Runs work for the NVIDIA graphics driver.",
            "Can't be killed. The graphics driver could stop working.",
            Risk::Medium,
        ),
    ),
    (
        "xfs",
        e(
            "Background work for an XFS drive: writing changes, clearing the journal and freeing space.",
            "Can't be killed. The drive could not be written to safely.",
            Risk::High,
        ),
    ),
    (
        "ext4",
        e(
            "Background work for an ext4 drive.",
            "Can't be killed. The drive could not be written to safely.",
            Risk::High,
        ),
    ),
    (
        "btrfs",
        e(
            "Background work for a Btrfs drive: checking, balancing and cleaning.",
            "Can't be killed. The drive could not be written to safely.",
            Risk::High,
        ),
    ),
    (
        "nvme",
        e(
            "Talks to an NVMe solid-state drive.",
            "Can't be killed. The drive would stop responding.",
            Risk::High,
        ),
    ),
    (
        "scsi_",
        e(
            "Talks to a storage drive (SATA, USB or SCSI).",
            "Can't be killed. The drive would stop responding.",
            Risk::High,
        ),
    ),
    (
        "ata_",
        e(
            "Talks to a SATA drive.",
            "Can't be killed. The drive would stop responding.",
            Risk::High,
        ),
    ),
    (
        "usb",
        e(
            "Handles USB devices.",
            "Can't be killed. USB devices could stop responding.",
            Risk::Medium,
        ),
    ),
    (
        "card",
        e(
            "Updates the display of one graphics output.",
            "Can't be killed. The screen could freeze.",
            Risk::Medium,
        ),
    ),
    (
        "amdgpu",
        e(
            "Runs work for the AMD graphics driver.",
            "Can't be killed. The graphics driver could stop working.",
            Risk::Medium,
        ),
    ),
    (
        "i915",
        e(
            "Runs work for the Intel graphics driver.",
            "Can't be killed. The graphics driver could stop working.",
            Risk::Medium,
        ),
    ),
];

/// Services, by program name.
const SERVICES: &[(&str, Explanation)] = &[
    (
        "systemd-userdbd",
        e(
            "Looks up user accounts and groups for programs that ask.",
            "Restarted on demand. Lookups fail briefly.",
            Risk::Low,
        ),
    ),
    (
        "systemd-userwork",
        e(
            "A helper that answers one user-account lookup for systemd-userdbd. They come and go.",
            "Nothing. It is replaced as needed.",
            Risk::Low,
        ),
    ),
    (
        "tuned",
        e(
            "Adjusts system settings for power saving or performance.",
            "The system keeps its current settings. Profile switching stops.",
            Risk::Low,
        ),
    ),
    (
        "tuned-ppd",
        e(
            "Connects the desktop's power mode selector to tuned.",
            "The power mode selector stops working.",
            Risk::Low,
        ),
    ),
    (
        "tailscaled",
        e(
            "The Tailscale VPN service, which links your devices over a private network.",
            "Your Tailscale connection drops until it restarts.",
            Risk::Low,
        ),
    ),
    (
        "smartd",
        e(
            "Watches drive health and warns of failing drives.",
            "You get no early warning of a failing drive.",
            Risk::Low,
        ),
    ),
    (
        "abrt-dump-journal-core",
        e(
            "Notices program crashes in the logs so they can be reported.",
            "Crash reports stop being collected.",
            Risk::Low,
        ),
    ),
    (
        "abrtd",
        e(
            "Collects crash reports.",
            "Crash reports stop being collected.",
            Risk::Low,
        ),
    ),
    (
        "uresourced",
        e(
            "Reserves CPU and memory for the app you are using so it stays responsive.",
            "The desktop may be a little less smooth under heavy load.",
            Risk::Low,
        ),
    ),
    (
        "switcheroo-control",
        e(
            "Chooses which graphics card runs which app on computers with two.",
            "Apps use the default graphics card.",
            Risk::Low,
        ),
    ),
    (
        "sedispatch",
        e(
            "Forwards SELinux security alerts to the troubleshooter.",
            "SELinux alerts stop being shown.",
            Risk::Low,
        ),
    ),
    (
        "watchdogd",
        e(
            "Restarts the computer if it freezes completely.",
            "A total freeze would not be recovered automatically.",
            Risk::Low,
        ),
    ),
    (
        "sddm-helper",
        e(
            "Starts your desktop session after you log in.",
            "Your session may end and return you to the login screen.",
            Risk::High,
        ),
    ),
    (
        "systemd",
        e(
            "The first program the system starts and the parent of nearly everything. It starts and watches every other service. A second copy runs for your login (as your user) and does the same for your session.",
            "The system copy (pid 1) can't be stopped; trying shuts nothing down but can destabilise things. Stopping your own copy ends your session and closes your apps.",
            Risk::High,
        ),
    ),
    (
        "systemd-journald",
        e(
            "Collects log messages from the system and programs and stores them.",
            "Restarted automatically within seconds. Logging has a brief gap.",
            Risk::Low,
        ),
    ),
    (
        "systemd-udevd",
        e(
            "Detects hardware you plug in and sets it up, such as a USB drive or keyboard.",
            "New devices would not be set up until it is back. Usually restarted automatically.",
            Risk::Medium,
        ),
    ),
    (
        "systemd-logind",
        e(
            "Manages logins, seats and power buttons, and decides what happens when you close the lid.",
            "Can be restarted by systemd, but your session may lose its power and screen-lock controls. Log in again if the desktop misbehaves.",
            Risk::Medium,
        ),
    ),
    (
        "systemd-oomd",
        e(
            "Ends a runaway program early when memory is nearly full, so the desktop stays usable.",
            "Nothing visible. The computer may freeze longer if you run out of memory.",
            Risk::Low,
        ),
    ),
    (
        "systemd-resolved",
        e(
            "Looks up website names (DNS) and turns them into addresses.",
            "Websites will not load by name until it restarts, which is normally automatic.",
            Risk::Medium,
        ),
    ),
    (
        "systemd-timesyncd",
        e(
            "Keeps the computer's clock correct using internet time servers.",
            "The clock slowly drifts. Nothing else breaks.",
            Risk::Low,
        ),
    ),
    (
        "systemd-homed",
        e(
            "Manages home folders that are portable or encrypted.",
            "Those users could not log in until it is back.",
            Risk::Medium,
        ),
    ),
    (
        "dbus-broker",
        e(
            "The message bus. Programs use it to talk to each other, for example your desktop asking the system to connect to Wi-Fi.",
            "Many programs and parts of the desktop stop working and you may be logged out. Restart the computer if so.",
            Risk::High,
        ),
    ),
    (
        "dbus-daemon",
        e(
            "The message bus. Programs use it to talk to each other.",
            "Many programs and parts of the desktop stop working and you may be logged out.",
            Risk::High,
        ),
    ),
    (
        "NetworkManager",
        e(
            "Manages your network connections, both Wi-Fi and wired.",
            "You lose your internet connection until it restarts or you restart it.",
            Risk::Medium,
        ),
    ),
    (
        "wpa_supplicant",
        e(
            "Handles the secure part of joining a Wi-Fi network.",
            "Wi-Fi disconnects. NetworkManager usually starts it again.",
            Risk::Medium,
        ),
    ),
    (
        "avahi-daemon",
        e(
            "Finds printers, speakers and other devices on your home network.",
            "Those devices stop appearing. Nothing else is affected.",
            Risk::Low,
        ),
    ),
    (
        "bluetoothd",
        e(
            "Handles Bluetooth: pairing and connecting headphones, mice and so on.",
            "Bluetooth devices disconnect until it restarts.",
            Risk::Low,
        ),
    ),
    (
        "cupsd",
        e(
            "The print server.",
            "Printing stops working until it restarts.",
            Risk::Low,
        ),
    ),
    (
        "sshd",
        e(
            "Lets other computers log in to this one over the network.",
            "Nobody can connect remotely. Connections already open usually stay up.",
            Risk::Low,
        ),
    ),
    (
        "chronyd",
        e(
            "Keeps the computer's clock correct using internet time servers.",
            "The clock slowly drifts. Nothing else breaks.",
            Risk::Low,
        ),
    ),
    (
        "crond",
        e(
            "Runs scheduled jobs, such as nightly cleanups.",
            "Scheduled jobs do not run until it is started again.",
            Risk::Low,
        ),
    ),
    (
        "polkitd",
        e(
            "Decides whether a program is allowed to do something that needs administrator rights, and asks you for a password.",
            "Password prompts and settings changes may fail until it is back.",
            Risk::Medium,
        ),
    ),
    (
        "udisksd",
        e(
            "Mounts and unmounts drives, such as USB sticks.",
            "You could not open or safely remove drives until it is back.",
            Risk::Low,
        ),
    ),
    (
        "upowerd",
        e(
            "Reports battery and power status.",
            "The battery indicator disappears until it restarts.",
            Risk::Low,
        ),
    ),
    (
        "power-profiles-daemon",
        e(
            "Switches between power saver, balanced and performance modes.",
            "The power profile selector stops working.",
            Risk::Low,
        ),
    ),
    (
        "thermald",
        e(
            "Keeps the CPU from overheating by slowing it down when needed.",
            "The CPU may run hotter. The hardware still protects itself.",
            Risk::Medium,
        ),
    ),
    (
        "irqbalance",
        e(
            "Spreads hardware interrupts across CPU cores.",
            "One core might get overloaded. Otherwise no change.",
            Risk::Low,
        ),
    ),
    (
        "auditd",
        e(
            "Records security events to the audit log.",
            "Security events stop being recorded.",
            Risk::Low,
        ),
    ),
    (
        "firewalld",
        e(
            "The firewall, controlling which connections may reach your computer.",
            "Rules may stop being applied, leaving the computer more exposed.",
            Risk::Medium,
        ),
    ),
    (
        "gdm",
        e(
            "The graphical login screen.",
            "Your desktop session ends and you are returned to login.",
            Risk::High,
        ),
    ),
    (
        "sddm",
        e(
            "The graphical login screen.",
            "Your desktop session ends and you are returned to login.",
            Risk::High,
        ),
    ),
    (
        "Xorg",
        e(
            "The graphics server that draws windows on the screen.",
            "Your desktop and everything open in it closes, and unsaved work is lost.",
            Risk::High,
        ),
    ),
    (
        "kwin_wayland",
        e(
            "KDE's window manager: it draws and arranges every window on screen.",
            "Your desktop session ends and open programs close. Unsaved work is lost.",
            Risk::High,
        ),
    ),
    (
        "pipewire",
        e(
            "Handles sound and screen sharing.",
            "Sound cuts out until it restarts, normally in a moment.",
            Risk::Low,
        ),
    ),
    (
        "wireplumber",
        e(
            "Decides which audio device PipeWire uses, such as speakers or headphones.",
            "Sound cuts out until it restarts, normally in a moment.",
            Risk::Low,
        ),
    ),
    (
        "pipewire-pulse",
        e(
            "Lets older audio programs work with PipeWire.",
            "Sound in some apps cuts out until it restarts.",
            Risk::Low,
        ),
    ),
    (
        "plasmashell",
        e(
            "The KDE desktop itself: the panel, menu, wallpaper and widgets.",
            "The panel and wallpaper vanish. Your windows stay open. Restart it with 'plasmashell --replace'.",
            Risk::Medium,
        ),
    ),
    (
        "baloo_file",
        e(
            "KDE's file indexer. It scans your files so search is fast and can use a lot of disk the first time.",
            "File search stops finding new files. It is safe to stop.",
            Risk::Low,
        ),
    ),
    (
        "ssh-agent",
        e(
            "Remembers your unlocked SSH keys so you don't have to retype the passphrase.",
            "You'll be asked for passphrases again.",
            Risk::Low,
        ),
    ),
    (
        "ss",
        e(
            "A short-lived tool this monitor runs to see which programs are using the network.",
            "Nothing. It exits by itself.",
            Risk::Low,
        ),
    ),
];

/// Removes the numbering kernel threads carry, so "ksoftirqd/3" and
/// "kworker/0:1H-events" become "ksoftirqd" and "kworker".
fn base_name(name: &str) -> &str {
    name.split(['/', ':']).next().unwrap_or(name).trim()
}

/// The name shared by per-core copies of a kernel thread: "ksoftirqd/3" and
/// "ksoftirqd/4" both give "ksoftirqd". Other names are returned unchanged.
pub fn group_key(name: &str) -> &str {
    match name.split_once('/') {
        Some((base, _)) if !base.is_empty() => base,
        _ => name,
    }
}

/// Without permission to read the program file, the kernel's name for it is cut
/// to 15 characters, so "systemd-journald" arrives as "systemd-journal".
fn matches_service(key: &str, name: &str) -> bool {
    const COMM_MAX: usize = 15;
    key == name || (name.len() == COMM_MAX && key.starts_with(name))
}

/// Explains a process by its name and the "Belongs to" label.
/// Returns a generic description for kernel threads and services that aren't listed.
pub fn explain(name: &str, group: &str) -> Option<Explanation> {
    let is_kernel = group == "Kernel";
    let base = base_name(name);
    if is_kernel {
        let found = KERNEL_THREADS
            .iter()
            .find(|(k, _)| *k == base)
            .or_else(|| KERNEL_PREFIXES.iter().find(|(k, _)| base.starts_with(k)));
        return Some(found.map_or(KERNEL_GENERIC, |(_, x)| *x));
    }
    let found = SERVICES.iter().find(|(k, _)| matches_service(k, name));
    match (found, group) {
        (Some((_, x)), _) => Some(*x),
        (None, "System") => Some(SYSTEM_GENERIC),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbered_kernel_threads_match_their_base_name() {
        let kworker = explain("kworker/0:1H-events", "Kernel").unwrap();
        assert!(kworker.what.contains("worker"));
        assert_eq!(explain("ksoftirqd/12", "Kernel").unwrap().risk, Risk::Low);
        assert_eq!(explain("jbd2/sda1-8", "Kernel").unwrap().risk, Risk::High);
    }

    #[test]
    fn kernel_threads_fall_back_to_prefix_then_generic() {
        let nvme = explain("nvme-wq", "Kernel").unwrap();
        assert!(nvme.what.contains("NVMe"));
        let unknown = explain("mystery_thread", "Kernel").unwrap();
        assert!(unknown.what.contains("Linux kernel"));
    }

    #[test]
    fn services_match_by_name_and_unknown_system_ones_get_generic_text() {
        assert_eq!(explain("dbus-broker", "System").unwrap().risk, Risk::High);
        assert!(
            explain("some-new-daemon", "System")
                .unwrap()
                .what
                .contains("background service")
        );
    }

    #[test]
    fn names_cut_to_fifteen_characters_still_match() {
        let x = explain("systemd-journal", "System").unwrap();
        assert!(x.what.contains("log"));
        // A short name that happens to start a key is not a match.
        assert!(
            explain("systemd-j", "System")
                .unwrap()
                .what
                .contains("background service")
        );
    }

    #[test]
    fn per_core_copies_share_a_group_key() {
        assert_eq!(group_key("kworker/1:0H-kblockd"), "kworker");
        assert_eq!(group_key("idle_inject/12"), "idle_inject");
        assert_eq!(group_key("kswapd0"), "kswapd0");
        assert_eq!(group_key("/odd"), "/odd");
    }

    #[test]
    fn ordinary_applications_have_no_explanation() {
        assert!(explain("firefox", "Firefox").is_none());
        assert!(explain("zen", "Zen").is_none());
    }
}
