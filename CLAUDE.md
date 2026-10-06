# rust-resource-monitor

Linux hardware monitor in Rust (edition 2024) with an egui window. See `README.md`
for what it shows and its requirements. Linux only: it reads `/proc`, `/sys`, NVML
and `ss`.

## Commands

- `cargo check` / `cargo build`: fast compile check. The GUI can't be driven from here.
- `cargo test`: unit tests for the pure logic (process log, history sorting and grouping).
- `cargo clippy --all-targets`: keep it warning-free.
- `cargo fmt`: default rustfmt settings. A hook runs it after edits.
- `cargo run --release -- --dump`: print one full `Snapshot` as text. Use this to
  check collectors without opening a window.
- `cargo run --release -- --dump-tree`: print the process tree as text.

Do not run `cargo run --release` without `--dump`: it opens a window and blocks.

## Layout

- `src/main.rs`: entry point and the `--dump` modes.
- `src/sampler.rs`: background thread, one `Collector::sample()` per second into shared `State`.
- `src/snapshot.rs`: the data types shared by collectors and UI (`Snapshot`, `History`, `ProcInfo`, `ProcRecord`).
- `src/collect/`: one module per data source. `mod.rs` builds the `Snapshot`.
  - `procs.rs` builds the per-pid `ProcInfo` list from sysinfo plus per-pid GPU and network maps.
  - `proc_log.rs` remembers every process seen, including exited ones.
  - `owner.rs` maps a pid to the app or system part it belongs to (systemd unit and cgroup).
- `src/ui/`: egui code. The app copies the state out of the mutex each frame and never holds the lock while drawing.
  - `mod.rs` has the tabs and hardware panels, `history.rs` the process history table, `tree.rs` the process tree.

## Conventions and gotchas

- Rates are deltas between samples, so the first sample after startup is a baseline,
  not a measurement. `--dump` takes warm-up samples for that reason.
- sysinfo reports per-process CPU as percent of one core. Divide by core count to get
  percent of all machine (`procs::list` does this).
- sysinfo process start times drift by about a second between samples. Never use
  `(pid, start_time)` as an exact key; `ProcLog` allows a tolerance.
- Per-process disk and socket data is only visible for your own processes without root.
  NVML (GPU per-process) is NVIDIA only; AMD GPUs have no per-process data.
- Missing hardware must degrade quietly: collectors return empty or `None`, never panic.
- Keep the UI thread cheap. Anything slow (such as `ss`) belongs in the sampler, and
  `Net` already throttles it.
- Put new pure logic in a function with a unit test; the GUI itself is not tested.
- Update `README.md` when user-visible behaviour changes.
