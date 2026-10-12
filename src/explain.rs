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
    /// False for the general fallbacks, which say nothing about this process in particular.
    pub curated: bool,
}

const fn e(what: &'static str, if_killed: &'static str, risk: Risk) -> Explanation {
    Explanation {
        what,
        if_killed,
        risk,
        curated: true,
    }
}

const fn generic(what: &'static str, if_killed: &'static str, risk: Risk) -> Explanation {
    Explanation {
        curated: false,
        ..e(what, if_killed, risk)
    }
}

const KERNEL_GENERIC: Explanation = generic(
    "A thread inside the Linux kernel, the core of the operating system. The kernel \
     uses these to do its own housekeeping in the background, such as talking to \
     hardware or tidying memory. They have no program file and show no window.",
    "Normally you can't. Kernel threads ignore kill signals, so nothing happens (or \
     you are told you lack permission). They are not apps and are not worth worrying about.",
    Risk::Low,
);

const SYSTEM_GENERIC: Explanation = generic(
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
            "Carries out the moves of programs between CPU cores that the scheduler decides on. One per core.",
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
        "watchdogd",
        e(
            "Feeds the hardware watchdog timer so the computer is not reset by mistake.",
            "Can't be killed. If it stopped, the machine could reboot itself.",
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
            "Watches a pressure alert that a program set up (for example systemd-oomd) and wakes it when the system is short of CPU, memory or disk.",
            "Can't be killed. Nothing happens, and the pressure numbers are unaffected.",
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
        "UVM",
        e(
            "Runs work for the NVIDIA graphics driver.",
            "Can't be killed. The graphics driver could stop working.",
            Risk::Medium,
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
        "kcompactd",
        e(
            "Defragments RAM: it moves memory around so large free blocks are available.",
            "Can't be killed. Without it, big memory requests could get slower.",
            Risk::Low,
        ),
    ),
    (
        "kswapd",
        e(
            "Frees up RAM when it runs low, by pushing rarely used data out to swap or dropping cached files. It is busy when memory is nearly full.",
            "Can't be killed. If it is using a lot of CPU, you are short on RAM; close some applications.",
            Risk::Low,
        ),
    ),
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
            "Background upkeep for a Btrfs drive: saving pending changes to disk and deleting removed snapshots.",
            "Can't be killed. The drive could not be written to safely.",
            Risk::High,
        ),
    ),
    (
        "nvme",
        e(
            "Background helper for an NVMe drive's controller, such as resets and housekeeping.",
            "Can't be killed. The drive could stop responding.",
            Risk::Medium,
        ),
    ),
    (
        "scsi_",
        e(
            "Recovers a SATA, USB or SCSI drive after it reports an error. It is idle unless a drive misbehaves.",
            "Can't be killed. Drive error recovery would not run.",
            Risk::Low,
        ),
    ),
    (
        "ata_",
        e(
            "Background helper for a SATA drive's controller, such as resets and housekeeping.",
            "Can't be killed. The drive could stop responding.",
            Risk::Medium,
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
            "Tells the desktop whether the computer has two graphics cards, so it can offer to run an app on the stronger one.",
            "The 'run with dedicated graphics' option goes away. Apps use the default card.",
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
            "The system copy (pid 1) is protected: it can't be killed, and a polite stop request just makes it reload itself. Stopping your own copy ends your session and closes your apps.",
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
            "Watches memory pressure and, when memory is nearly full, ends a whole group of programs early so the desktop stays usable.",
            "Nothing visible. Without it, a memory shortage may freeze the computer for longer. systemd restarts it if it crashes.",
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
            "Bluetooth devices disconnect. It may start again when an app asks for Bluetooth, or you can restart it.",
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
            "Handles remote logins over the network (SSH). There is usually one copy per connection.",
            "Ends that one remote login. If this is the main listener, new logins stop until it is started again.",
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
            "Rules already loaded stay active, but firewall changes and automatic rules for new networks stop working until it is back.",
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
            "The panel and wallpaper vanish. Your windows stay open. It may restart by itself; if not, run 'plasmashell --replace'.",
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
            "A short-lived command that lists network connections. This monitor runs it to match connections to programs.",
            "Nothing. It exits by itself.",
            Risk::Low,
        ),
    ),
];

/// Which table an entry comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Table {
    KernelThread,
    KernelPrefix,
    Service,
}

/// Every hand-written entry, for the `--check-explain` review.
pub fn entries() -> impl Iterator<Item = (Table, &'static str, &'static Explanation)> {
    let tagged = |table, list: &'static [(&'static str, Explanation)]| {
        list.iter().map(move |(k, x)| (table, *k, x))
    };
    tagged(Table::KernelThread, KERNEL_THREADS)
        .chain(tagged(Table::KernelPrefix, KERNEL_PREFIXES))
        .chain(tagged(Table::Service, SERVICES))
}

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

/// "md" is too short to match on its own: RAID threads are "md0_raid1" and the like.
fn prefix_matches(key: &str, base: &str) -> bool {
    match base.strip_prefix(key) {
        Some(rest) if key == "md" => rest.starts_with(|c: char| c.is_ascii_digit()),
        Some(_) => true,
        None => false,
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
        let found = KERNEL_THREADS.iter().find(|(k, _)| *k == base).or_else(|| {
            KERNEL_PREFIXES
                .iter()
                .find(|(k, _)| prefix_matches(k, base))
        });
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
    fn md_prefix_only_matches_raid_threads() {
        assert!(
            explain("md127_resync", "Kernel")
                .unwrap()
                .what
                .contains("RAID")
        );
        assert!(
            explain("mdio_thread", "Kernel")
                .unwrap()
                .what
                .contains("Linux kernel")
        );
        assert!(explain("kswapd1", "Kernel").unwrap().what.contains("RAM"));
    }

    #[test]
    fn only_listed_entries_count_as_curated() {
        assert!(explain("ksoftirqd/2", "Kernel").unwrap().curated);
        assert!(!explain("mystery_thread", "Kernel").unwrap().curated);
        assert!(!explain("some-new-daemon", "System").unwrap().curated);
    }

    #[test]
    fn entries_have_no_duplicate_keys_within_a_table() {
        let mut seen = std::collections::HashSet::new();
        for (table, key, _) in entries() {
            assert!(seen.insert((table, key)), "duplicate entry {key}");
        }
    }

    #[test]
    fn ordinary_applications_have_no_explanation() {
        assert!(explain("firefox", "Firefox").is_none());
        assert!(explain("zen", "Zen").is_none());
    }
}
