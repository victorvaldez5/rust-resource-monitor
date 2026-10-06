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

### Process tree

The "Process tree" tab shows every process under the one that started it, with its
PID, user, what it belongs to, CPU and memory. Click a process to fold or unfold its
children. Hovering one shows the chain of processes that started it, its systemd
unit, how long it has been running and its command line.

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

`--dump-tree` prints the process tree the same way.

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
