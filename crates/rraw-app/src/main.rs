#![forbid(unsafe_code)]

//! Native macOS Rraw application entry point wiring all 9 workspace library crates.

use std::path::Path;

use rraw_app::{initialize_shell_summary, RrawApp};

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        let info = rraw_core::build_info::BuildInfo::current();
        println!(
            "rraw-app v{} ({} {})",
            info.version, info.commit_hash, info.build_date
        );
        return std::process::ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--summary" || a == "--headless") {
        return match initialize_shell_summary() {
            Ok(summary) => {
                println!("{summary}");
                std::process::ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("rraw-app initialization failed: {err}");
                std::process::ExitCode::FAILURE
            }
        };
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("Rraw — 120 Hz EDR Darkroom"),
        ..Default::default()
    };

    let initial_path = args.get(1).and_then(|arg| {
        if arg.starts_with('-') {
            None
        } else {
            let p = Path::new(arg);
            let abs_path = if p.is_absolute() {
                p.to_path_buf()
            } else {
                std::env::current_dir()
                    .map(|cwd| cwd.join(p))
                    .unwrap_or_else(|_| p.to_path_buf())
            };
            Some(abs_path)
        }
    });

    match eframe::run_native(
        "Rraw — 120 Hz EDR Darkroom",
        native_options,
        Box::new(move |cc| {
            let mut app = RrawApp::new(cc);
            if let Some(path) = initial_path {
                app.open_raw_file(&path);
            }
            Ok(Box::new(app))
        }),
    ) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("rraw-app GUI launch failed: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}
