pub mod ai;
pub mod asr;
pub mod audio;
pub mod chat;
pub mod commands;
pub mod costs;
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

/// Log lines go to stderr and to `lezioni.log` in the app log folder (the only place a user
/// who opened the app from the Finder can find them). Started again when over 5 MB.
fn init_log(dir: Option<std::path::PathBuf>) {
    struct Tee(Option<std::fs::File>);
    impl std::io::Write for Tee {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            let _ = std::io::stderr().write_all(b);
            if let Some(f) = &mut self.0 {
                let _ = f.write_all(b);
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            if let Some(f) = &mut self.0 {
                f.flush()?;
            }
            Ok(())
        }
    }
    let file = dir.and_then(|d| {
        std::fs::create_dir_all(&d).ok()?;
        let p = d.join(commands::LOG_FILE);
        let big = std::fs::metadata(&p).is_ok_and(|m| m.len() > 5_000_000);
        std::fs::OpenOptions::new().create(true).append(!big).write(true).truncate(big).open(p).ok()
    });
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,whisper_rs=warn")).target(env_logger::Target::Pipe(Box::new(Tee(file)))).try_init();
    log::info!("Lezioni {} avviata", env!("CARGO_PKG_VERSION"));
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            init_log(app.path().app_log_dir().ok());
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let settings = settings::Settings::load(&settings_path);
            if let Some(d) = &settings.library_dir {
                allow_assets(app.handle(), d);
            }
            app.manage(AppState { settings: Mutex::new(settings), settings_path, write: Default::default(), jobs: Default::default(), opened: Mutex::new(packages_in_args()), whisper: std::sync::Arc::new(asr::Loader::new(app.path().app_data_dir()?.join("modelli"))) });
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
            log_reveal,
            openrouter_models,
            whisper_download,
            whisper_status,
            whisper_cancel,
            whisper_delete,
            ai_settings_get,
            ai_settings_set,
            ai_check,
            lesson_process,
            job_cancel,
            lesson_outputs,
            extract_save,
            summary_save,
            study_save,
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
            course_costs,
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
