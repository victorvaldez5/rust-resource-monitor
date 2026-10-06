use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::collect::Collector;
use crate::snapshot::State;

pub const INTERVAL: Duration = Duration::from_secs(1);

/// Samples the hardware once per `INTERVAL` on a background thread and asks the
/// UI to repaint after each sample, so the window only redraws when data changes.
pub fn spawn(shared: Arc<Mutex<State>>, ctx: eframe::egui::Context) {
    thread::Builder::new()
        .name("sampler".into())
        .spawn(move || {
            let mut collector = Collector::new();
            // CPU usage needs a short gap after the baseline to be meaningful.
            thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
            loop {
                let started = Instant::now();
                let snapshot = collector.sample();
                {
                    let mut state = shared.lock().unwrap();
                    state.history.push(&snapshot);
                    state.snapshot = snapshot;
                }
                ctx.request_repaint();
                thread::sleep(INTERVAL.saturating_sub(started.elapsed()));
            }
        })
        .expect("failed to start sampler thread");
}
