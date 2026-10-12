mod check_explain;
mod collect;
mod explain;
mod sampler;
mod security;
mod snapshot;
mod tray;
mod ui;

use std::sync::{Arc, Mutex};

use eframe::egui;

use snapshot::State;

fn main() -> eframe::Result {
    // `--dump` prints one sample as text and exits, for checking sensors without
    // the window; `--dump-tree` prints the process tree the same way.
    // `--check-explain` prints the explanation texts beside the system's own docs.
    let dump_tree = std::env::args().any(|a| a == "--dump-tree");
    let check_explain = std::env::args().any(|a| a == "--check-explain");
    let dump_trust = std::env::args().any(|a| a == "--dump-trust");
    if dump_tree || check_explain || dump_trust || std::env::args().any(|a| a == "--dump") {
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
        if dump_trust {
            // Package checks run on their own thread; wait for them to finish.
            const MAX_WAIT_SAMPLES: usize = 120;
            for _ in 0..MAX_WAIT_SAMPLES {
                if collector.trust_pending() == 0 {
                    break;
                }
                std::thread::sleep(sampler::INTERVAL);
            }
            snapshot = collector.sample();
        }
        let procs = std::mem::take(&mut snapshot.procs);
        if dump_trust {
            print_trust(&procs);
        } else if check_explain {
            check_explain::print(&procs);
        } else if dump_tree {
            for row in ui::tree::rows(&procs, &Default::default()) {
                let p = &procs[row.index];
                let indent = "  ".repeat(row.depth);
                let name = match &row.group {
                    Some(g) => format!("{} ({})", g.name, g.members.len()),
                    None => p.name.clone(),
                };
                println!("{:>7} {:<12} {:<16} {indent}{name}", p.pid, p.user, p.group);
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
            let tray = Arc::new(tray::TrayState::default());
            tray::spawn(shared.clone(), cc.egui_ctx.clone(), tray.clone());
            Ok(Box::new(ui::App::new(shared, tray)))
        }),
    )
}

/// Lists every process that isn't plainly verified, worst first, then a summary.
fn print_trust(procs: &[snapshot::ProcInfo]) {
    let mut counts = std::collections::BTreeMap::new();
    for p in procs {
        *counts.entry(p.trust.label()).or_insert(0usize) += 1;
    }
    let mut shown: Vec<_> = procs
        .iter()
        .filter(|p| !matches!(p.trust, security::Trust::Verified | security::Trust::Kernel))
        .collect();
    shown.sort_by_key(|p| (std::cmp::Reverse(p.trust.severity()), p.pid));
    for p in shown {
        let (package, sha, reason) = match &p.provenance {
            Some(v) => (
                v.package.as_deref().unwrap_or("-"),
                v.sha256.as_deref().unwrap_or("-"),
                v.reason.as_str(),
            ),
            None => ("-", "-", "not checked yet"),
        };
        println!(
            "{:>7} {:<12} {:<11} {:<16} {package}\n          sha256 {sha}\n          {reason}",
            p.pid,
            p.user,
            p.trust.label(),
            p.name
        );
    }
    println!("\n{counts:?}");
}
