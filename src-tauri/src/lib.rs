pub mod ai;
pub mod audio;
pub mod chat;
pub mod commands;
pub mod error;
pub mod jobs;
pub mod library;
pub mod pipeline;
pub mod search;
pub mod secrets;
pub mod settings;
pub mod share;

use std::sync::Mutex;

use tauri::Manager;

use commands::*;

fn is_package(p: &std::path::Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("lezione"))
}

/// Windows and Linux pass the double-clicked file as an argument.
fn packages_in_args() -> Vec<std::path::PathBuf> {
    std::env::args_os().skip(1).map(std::path::PathBuf::from).filter(|p| is_package(p) && p.is_file()).collect()
}

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let settings = settings::Settings::load(&settings_path);
            if let Some(d) = &settings.library_dir {
                allow_assets(app.handle(), d);
            }
            app.manage(AppState { settings: Mutex::new(settings), settings_path, write: Default::default(), jobs: Default::default(), opened: Mutex::new(packages_in_args()) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            settings_get,
            settings_set_library,
            courses_list,
            course_create,
            course_update,
            course_delete,
            lessons_list,
            lesson_get,
            lesson_create,
            lesson_update,
            lesson_delete,
            sources_add_files,
            source_add_bytes,
            source_add_text,
            source_remove,
            source_path,
            source_open,
            reveal,
            ai_settings_get,
            ai_settings_set,
            ai_check,
            lesson_process,
            job_cancel,
            lesson_outputs,
            search_archive,
            export_package,
            package_inspect,
            package_import,
            export_markdown,
            export_anki,
            export_write_text,
            export_print,
            opened_packages,
            chat_load,
            chat_clear,
            chat_ask,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            // macOS: double click on a .lezione while the app is running (or to start it).
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = &event {
                use tauri::Emitter;
                let paths: Vec<std::path::PathBuf> = urls.iter().filter_map(|u| u.to_file_path().ok()).filter(|p| is_package(p)).collect();
                if !paths.is_empty() {
                    app.state::<AppState>().opened.lock().unwrap().extend(paths);
                    let _ = app.emit("packages-opened", ());
                }
            }
            let _ = (app, event);
        });
}
