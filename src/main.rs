mod collect;
mod sampler;
mod snapshot;
mod ui;

use std::sync::{Arc, Mutex};

use eframe::egui;

use snapshot::State;

fn main() -> eframe::Result {
    // `--dump` prints one sample as text and exits, for checking sensors without the window.
    if std::env::args().any(|a| a == "--dump") {
        // Rates are differences between samples, and the per-app network table
        // only fills in on its second pass, so take a few before printing.
        const WARM_UP_SAMPLES: usize = 2;
        let mut collector = collect::Collector::new();
        for _ in 0..WARM_UP_SAMPLES {
            std::thread::sleep(sampler::INTERVAL);
            collector.sample();
        }
        std::thread::sleep(sampler::INTERVAL);
        println!("{:#?}", collector.sample());
        return Ok(());
    }

    let shared = Arc::new(Mutex::new(State::default()));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Resource Monitor")
            .with_app_id("rust-resource-monitor")
            .with_inner_size([1320.0, 860.0])
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
