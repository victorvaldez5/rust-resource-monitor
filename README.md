# rust-resource-monitor

A simple hardware monitor for Linux, written in Rust with [egui](https://github.com/emilk/egui).
It shows how busy and how hot your CPU, RAM, GPU, storage and network are, and which
processes are responsible.

## What it shows

| Panel | Usage | Temperature | Per process |
|---|---|---|---|
| CPU | Total and per-core load | Package sensor | CPU % |
| RAM | Used / total, swap, memory pressure | DDR5 DIMM sensors | Resident memory |
| GPU | Load, VRAM, power draw, fan speed | GPU sensor | VRAM and GPU % (NVIDIA only) |
| Storage | Read / write rate per drive, space per filesystem | Drive sensors | Disk read / write |
| Network | Download / upload rate, ping to gateway and internet | Adapter sensor | TCP download / upload |

Every panel has a graph of the last 60 seconds, sampled once per second. The "Process history" tab lists every process seen since the monitor started,
with a column for the user, what it belongs to, uptime, when it was last active, CPU, RAM, GPU, VRAM,
disk read and write, and network down and up. Processes with the same name, such as a browser's helpers, are folded into
with their usage added up; click it to see each process id. Click a column header to sort by it; click
again to reverse. Exited processes are dimmed and show only their uptime and last activity.

### System tray

The app puts an icon in the system tray with live ring gauges: CPU on the outside, then
GPU, then RAM, each filling clockwise from the top and turning yellow above 60% and red
above 85%. The dot in the middle is the memory pressure: green, yellow or red like the
RAM graph. Hover for the numbers and temperatures. Click the icon to minimize or restore
the window, or use its menu. While the icon is up, closing the window minimizes it
instead of quitting; use "Quit" in the tray menu to exit. Without a GPU reading the icon
has two rings. If the desktop has no tray, the window behaves as usual.

The tray uses the StatusNotifierItem protocol, which KDE Plasma supports. GNOME needs
the AppIndicator extension.

### What a process belongs to

The "Belongs to" label, shown in the process tree and in tooltips, says which application or part of
the system a process is part of:

| Label | Meaning |
|---|---|
| Kernel | Kernel threads |
| System | System services, such as NetworkManager |
| KDE | The Plasma desktop and KDE applications |
| User session | Background helpers of your login session, such as PipeWire |
| An application name | Everything that application started, for example "Zen" |

Applications are recognised by the systemd unit the desktop starts them in. A program
run from a terminal is named after the command that was typed, so everything Claude
Code starts is "Claude" rather than the terminal it runs in.

### Explanations for system and kernel processes

Hovering a kernel thread or a system service, in either the process tree or the
process history, adds two short plain-language answers to the tooltip: what the
process is for, and what happens if it is killed, with a risk label (low risk,
some risk, dangerous). Numbered kernel threads such as `kworker/0:1H` and
`ksoftirqd/3` are matched by their base name. Kernel threads and services that
aren't in the built-in list (`src/explain.rs`) get a general description instead.
For a service that isn't in the list, the tooltip also quotes the one-line description
from its systemd unit file. Ordinary applications get no explanation. The descriptions are advice only;
nothing in the monitor kills processes.

`cargo run --release -- --check-explain` prints every service entry next to the man
page, systemd description and package summary this machine has for it, and lists the
running processes that only get a general description. Use it to review the texts.

### Process tree

The "Process tree" tab shows every process under the one that started it, with its
PID, user, what it belongs to, CPU and memory. Click the arrow next to a process to
fold or unfold its children. Click its name to open a details pane on the right with
the chain of processes that started it, its systemd unit, how long it has been
running, its command line and, for system and kernel processes, the plain-language
explanation. The selected row is outlined while its pane is open. Close the pane with its Close
button, with Esc, or by clicking the selected name again. Hovering a name shows the same details. When the window
or the pane leaves too little room, the list scrolls sideways (drag the bar at the
bottom, or hold Shift and use the mouse wheel) instead of cutting off columns.

Per-core copies of a process, such as `ksoftirqd/0`, `ksoftirqd/1` and so on, are
folded into one line like `ksoftirqd (24)` with their CPU and memory added up. Click it
to list them. The process history folds them the same way.

### Where a program came from

Each process's program file is checked against the system's package manager (`rpm`,
`dpkg` or `pacman`, whichever the system has) and given a verdict. Linux has no
universal code signing, so this answers "where did this come from, and has it been
changed?", not "is it safe?".

| Verdict | Meaning |
|---|---|
| Verified | A package from your distribution owns the file and it matches the package. |
| Updated | The package was updated after the process started; restart it. |
| Third-party | An unsigned package or one from outside the distribution (RPM Fusion, a vendor repo, a PPA, the AUR), or a Flatpak or Snap. |
| Unpackaged | No package owns the file (something in your home folder, `/tmp`, a build). |
| Deleted | The file was deleted while it runs and no package owns it. |
| Modified | A package owns the file but its size or contents differ from the package. |
| Unknown | Not checked yet, or the file can't be read. |

The history table has a sortable "Trust" column (worst first). In the process tree a
coloured dot marks anything other than Verified, and the details pane gives the reason
and, for files that aren't verified, the SHA-256 of the program file. Results are
worked out in the background, so a new process shows Unknown for a moment.

Limits: a script run by an interpreter shows the interpreter's verdict, not the
script's; the dpkg and pacman checks are covered by unit tests but have not been
tried on those systems; protected programs (setuid or with capabilities) and other
users' processes need root to be read. `--dump-trust` prints every process that isn't
Verified, with its reason and hash.

### Memory pressure

The RAM graph shows memory pressure rather than plain usage: its height is the share
of memory that is no longer available, and its colour is the pressure level.

| Level | When |
|---|---|
| Green | Plenty of room |
| Yellow | Less than 15% of RAM available, or tasks stalled on memory more than 1% of the time |
| Red | Less than 5% of RAM available, or tasks stalled on memory more than 10% of the time |

"Available" is `MemAvailable` from `/proc/meminfo`, and the stall figure is `some avg10`
from the kernel's pressure stall information (`/proc/pressure/memory`). On kernels
without it, the level uses available memory alone.

## Building and running

You need a recent Rust toolchain (the crate uses the 2024 edition).

```sh
cargo run --release
```

To print one sample as text instead of opening the window, which is handy for
checking what sensors were found:

```sh
cargo run --release -- --dump
```

`--dump-tree` prints the process tree the same way, and `--dump-trust` prints where
each program came from.

## Requirements

Developed and tested on Fedora 44 (KDE Plasma, Wayland) with an AMD Ryzen CPU, an
NVIDIA GPU and an AMD integrated GPU. Other hardware may work but has not been tried.

- **CPU temperature:** the `k10temp` (AMD) or `coretemp` (Intel) kernel driver.
- **NVIDIA GPUs:** the proprietary driver, which provides the NVML library.
- **AMD GPUs:** the `amdgpu` kernel driver. GPU names come from `/usr/share/hwdata/pci.ids`.
- **Per-process network usage:** the `ss` command from `iproute`.
- **Ping:** unprivileged ICMP sockets, which Fedora allows by default
  (`net.ipv4.ping_group_range`).

Anything that is missing is left out of the window; the rest still works.

## Limitations

- Without root, Linux only exposes disk I/O and socket ownership for your own
  processes, so other users' and system processes are missing from the storage and
  network lists.
- Per-process network usage is measured from TCP connections. UDP and QUIC (HTTP/3)
  traffic is not counted per process.
- AMD GPUs have no per-process list.
- Intel GPUs are not supported.
