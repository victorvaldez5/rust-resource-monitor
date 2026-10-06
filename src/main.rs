mod collect;
mod sampler;
mod snapshot;
mod ui;

use std::sync::{Arc, Mutex};

use eframe::egui;

use snapshot::State;

fn main() -> eframe::Result {
    // `--dump` prints one sample as text and exits, for checking sensors without
    // the window; `--dump-tree` prints the process tree the same way.
    let dump_tree = std::env::args().any(|a| a == "--dump-tree");
    if dump_tree || std::env::args().any(|a| a == "--dump") {
        // Rates are differences between samples, and the per-app network table
        // only fills in on its second pass, so take a few before printing.
        const WARM_UP_SAMPLES: usize = 2;
        let mut collector = collect::Collector::new();
        for _ in 0..WARM_UP_SAMPLES {
            std::thread::sleep(sampler::INTERVAL);
            collector.sample();
        }
        std::thread::sleep(sampler::INTERVAL);
        let mut snapshot = collector.sample();
        let procs = std::mem::take(&mut snapshot.procs);
        if dump_tree {
            for row in ui::tree::rows(&procs, &Default::default()) {
                let p = &procs[row.index];
                let indent = "  ".repeat(row.depth);
                println!(
                    "{:>7} {:<12} {:<16} {indent}{}",
                    p.pid, p.user, p.group, p.name
                );
            }
        } else {
            println!("{snapshot:#?}");
        }
        return Ok(());
    }

    let shared = Arc::new(Mutex::new(State::default()));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Resource Monitor")
            .with_app_id("rust-resource-monitor")
            .with_inner_size([1420.0, 860.0])
            .with_min_inner_size([640.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Resource Monitor",
        options,
        Box::new(move |cc| {
            sampler::spawn(shared.clone(), cc.egui_ctx.clone());
            Ok(Box::new(ui::App::new(shared)))
        }),
    )
}
