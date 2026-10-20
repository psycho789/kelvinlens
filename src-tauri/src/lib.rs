//! Tauri shell: keeps opened images and videos in memory and exposes the core library to the UI.
//! The app has no network access; files are only read or written at paths the user picked.

mod cmd_error;
mod commands;
#[cfg(debug_assertions)]
mod dev;
mod dto;
mod state;
#[cfg(test)]
mod wire_tests;

use std::path::Path;

use kelvinlens_core::ThermalImage;
use kelvinlens_core::video::{self, Video};

#[cfg(test)]
pub(crate) use dto::*;
pub(crate) use state::*;

/// `--self-test <file>` opens a thermogram, prints its size and centre temperature, and exits
/// without creating a window. CI runs it on the packaged binary of every OS.
fn self_test(path: &str) -> i32 {
    let p = Path::new(path);
    let opened = if video::is_video(p) { Video::open(p).and_then(|v| v.frame(0, None)) } else { ThermalImage::open(p) };
    match opened {
        Ok(img) => {
            let centre = img.kelvin_at(img.width() / 2, img.height() / 2).unwrap_or(f32::NAN);
            println!("{} {}x{} centre={:.2}C", img.file_name(), img.width(), img.height(), centre - 273.15);
            0
        }
        Err(e) => {
            eprintln!("self-test failed: {e}");
            1
        }
    }
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--self-test") {
        std::process::exit(self_test(args.get(i + 1).map(String::as_str).unwrap_or("")));
    }

    macro_rules! handlers {
        ($($extra:path),*) => {
            tauri::generate_handler![
                commands::image::open_image,
                commands::image::close_image,
                commands::image::thermal_data,
                commands::image::photo_jpeg,
                commands::image::set_params,
                commands::image::reset_params,
                commands::analysis::measure,
                commands::analysis::outlines,
                commands::analysis::profile,
                commands::analysis::coverage,
                commands::analysis::evaluate_formulas,
                commands::render::render,
                commands::render::fuse,
                commands::analysis::inspect,
                commands::image::triage,
                commands::image::stitch,
                commands::files::export_thermal_tiff,
                commands::files::save_flir_copy,
                commands::video::select_frame,
                commands::video::video_series,
                commands::video::hsm,
                commands::video::export_segment,
                commands::video::avi_add_frame,
                commands::video::avi_reset,
                commands::video::avi_finish,
                commands::image::list_folder,
                commands::image::is_folder,
                commands::image::palettes,
                commands::image::templates,
                commands::files::export_csv,
                commands::files::stage_bytes,
                commands::files::write_staged,
                commands::files::write_text,
                commands::files::read_text,
                commands::files::read_image_data_url,
                commands::files::export_map,
                commands::files::save_sidecar,
                commands::report::make_report,
                commands::image::initial_paths,
                $($extra),*
            ]
        };
    }

    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init()).manage(AppState::default());
    #[cfg(debug_assertions)]
    let builder = builder.invoke_handler(handlers![dev::dev_log]).setup(|app| {
        dev::start(app.handle());
        Ok(())
    });
    #[cfg(not(debug_assertions))]
    let builder = builder.invoke_handler(handlers![]);
    builder.run(tauri::generate_context!()).expect("error while running Kelvinlens");
}
