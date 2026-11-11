//! All the `#[tauri::command]`s. They are thin: the work is in `library.rs`.
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::NaiveDate;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::error::{AppError, CmdResult};
use crate::library::{Course, Lesson, LessonPatch, Library};
use crate::settings::Settings;

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
    /// Held by every command that writes the archive: `lezione.json` is read, changed and
    /// written back, and a folder rename in the middle of a copy would lose files.
    pub write: std::sync::Arc<Mutex<()>>,
    pub jobs: crate::jobs::Jobs,
    /// `.lezione` files the app was opened with (double click), until the UI asks for them.
    pub opened: Mutex<Vec<PathBuf>>,
    pub whisper: Arc<crate::asr::Loader>,
}

impl AppState {
    fn writing(&self) -> std::sync::MutexGuard<'_, ()> {
        self.write.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn library(&self) -> CmdResult<Library> {
        let dir = self.settings.lock().unwrap().library_dir.clone();
        match dir {
            Some(d) => Ok(Library::open(d)?),
            None => Err(AppError::NoLibrary { message: "Scegli la cartella dell'archivio".into() }),
        }
    }
}

/// Lets the webview show files of the archive (image and PDF previews, audio player).
pub fn allow_assets(app: &AppHandle, dir: &std::path::Path) {
    if let Err(e) = app.asset_protocol_scope().allow_directory(dir, true) {
        log::warn!("asset scope: {e}");
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    library_dir: Option<PathBuf>,
    /// Suggested archive folder for the first run: Documenti/Lezioni.
    suggested_dir: Option<PathBuf>,
}

#[tauri::command(async)]
pub fn settings_get(app: AppHandle, state: State<'_, AppState>) -> CmdResult<SettingsView> {
    let s = state.settings.lock().unwrap();
    Ok(SettingsView { library_dir: s.library_dir.clone(), suggested_dir: app.path().document_dir().ok().map(|d| d.join("Lezioni")) })
}

#[tauri::command(async)]
pub fn settings_set_library(app: AppHandle, state: State<'_, AppState>, dir: PathBuf) -> CmdResult<()> {
    let _w = state.writing();
    Library::open(&dir)?;
    let mut s = state.settings.lock().unwrap();
    s.library_dir = Some(dir.clone());
    s.save(&state.settings_path)?;
    allow_assets(&app, &dir);
    Ok(())
}

#[tauri::command(async)]
pub fn courses_list(state: State<'_, AppState>) -> CmdResult<Vec<Course>> {
    Ok(state.library()?.courses()?)
}

#[tauri::command(async)]
pub fn course_create(state: State<'_, AppState>, name: String, teacher: String, terms: String) -> CmdResult<Course> {
    let _w = state.writing();
    Ok(state.library()?.create_course(&name, &teacher, &terms)?)
}

#[tauri::command(async)]
pub fn course_update(state: State<'_, AppState>, id: String, name: String, teacher: String, terms: String) -> CmdResult<Course> {
    let _w = state.writing();
    Ok(state.library()?.update_course(&id, &name, &teacher, &terms)?)
}

#[tauri::command(async)]
pub fn course_delete(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let _w = state.writing();
    Ok(state.library()?.delete_course(&id)?)
}

#[tauri::command(async)]
pub fn lessons_list(state: State<'_, AppState>, course_id: String) -> CmdResult<Vec<Lesson>> {
    Ok(state.library()?.lessons(&course_id)?)
}

#[tauri::command(async)]
pub fn lesson_get(state: State<'_, AppState>, id: String) -> CmdResult<Lesson> {
    Ok(state.library()?.lesson(&id)?)
}

#[tauri::command(async)]
pub fn lesson_create(state: State<'_, AppState>, course_id: String, title: String, date: NaiveDate) -> CmdResult<Lesson> {
    let _w = state.writing();
    Ok(state.library()?.create_lesson(&course_id, &title, date)?)
}

#[tauri::command(async)]
pub fn lesson_update(state: State<'_, AppState>, id: String, patch: LessonPatch) -> CmdResult<Lesson> {
    let _w = state.writing();
    Ok(state.library()?.update_lesson(&id, patch)?)
}

#[tauri::command(async)]
pub fn lesson_delete(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let _w = state.writing();
    Ok(state.library()?.delete_lesson(&id)?)
}

#[derive(Serialize)]
pub struct AddResult {
    lesson: Lesson,
    warnings: Vec<String>,
}

#[tauri::command(async)]
pub fn sources_add_files(state: State<'_, AppState>, lesson_id: String, paths: Vec<PathBuf>) -> CmdResult<AddResult> {
    let lib = state.library()?;
    let _w = state.writing();
    let (lesson, warnings) = lib.add_files(&lesson_id, &paths)?;
    Ok(AddResult { lesson, warnings })
}

/// Raw body (the pasted file); `lesson-id` and `file-name` (ASCII, chosen by the UI) in the headers.
#[tauri::command(async)]
pub fn source_add_bytes(state: State<'_, AppState>, request: tauri::ipc::Request<'_>) -> CmdResult<Lesson> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(AppError::other("contenuto non valido"));
    };
    let header = |k: &str| request.headers().get(k).and_then(|v| v.to_str().ok()).map(str::to_string).ok_or_else(|| AppError::other(format!("manca {k}")));
    let lesson_id = header("lesson-id")?;
    let name = header("file-name")?;
    let _w = state.writing();
    Ok(state.library()?.add_bytes(&lesson_id, &name, bytes)?)
}

#[tauri::command(async)]
pub fn source_add_text(state: State<'_, AppState>, lesson_id: String, title: String, text: String) -> CmdResult<Lesson> {
    let _w = state.writing();
    Ok(state.library()?.add_text(&lesson_id, &title, &text)?)
}

#[tauri::command(async)]
pub fn source_remove(state: State<'_, AppState>, lesson_id: String, source_id: String) -> CmdResult<Lesson> {
    let _w = state.writing();
    Ok(state.library()?.remove_source(&lesson_id, &source_id)?)
}

#[tauri::command(async)]
pub fn source_path(state: State<'_, AppState>, lesson_id: String, source_id: String) -> CmdResult<PathBuf> {
    Ok(state.library()?.source_path(&lesson_id, &source_id)?)
}

/// Opens a source with the system app (Anteprima, Foto, VLC…).
#[tauri::command(async)]
pub fn source_open(app: AppHandle, state: State<'_, AppState>, lesson_id: String, source_id: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let p = state.library()?.source_path(&lesson_id, &source_id)?;
    app.opener().open_path(p.to_string_lossy(), None::<&str>).map_err(|e| AppError::other(e.to_string()))
}

/// Shows a file or folder of the archive in Finder / Esplora file.
#[tauri::command(async)]
pub fn reveal(app: AppHandle, path: PathBuf) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener().reveal_item_in_dir(path).map_err(|e| AppError::other(e.to_string()))
}

pub const LOG_FILE: &str = "lezioni.log";

/// Shows the log file in the Finder / Explorer.
#[tauri::command(async)]
pub fn log_reveal(app: AppHandle) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let p = app.path().app_log_dir().map_err(|e| AppError::other(e.to_string()))?.join(LOG_FILE);
    app.opener().reveal_item_in_dir(p).map_err(|e| AppError::other(e.to_string()))
}

// ---------- AI ----------

use crate::ai::{Gemini, OpenRouter, Provider};
use crate::settings::{ModelSet, ProviderKind};
use crate::jobs::JobEvent;
use crate::pipeline::{self, Archive, Engine, Models, Options, Outputs};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettingsView {
    has_key: bool,
    has_openrouter_key: bool,
    provider: ProviderKind,
    models: Models,
    openrouter: ModelSet,
    whisper_ready: bool,
}

#[tauri::command(async)]
pub fn ai_settings_get(state: State<'_, AppState>) -> CmdResult<AiSettingsView> {
    let s = state.settings.lock().unwrap().clone();
    Ok(AiSettingsView {
        has_key: crate::secrets::gemini_key()?.is_some(),
        has_openrouter_key: crate::secrets::openrouter_key()?.is_some(),
        provider: s.provider,
        models: s.models,
        openrouter: s.openrouter,
        whisper_ready: state.whisper.downloaded(),
    })
}

/// Keys: None = keep, "" = remove, otherwise the new key.
#[tauri::command(async)]
pub fn ai_settings_set(state: State<'_, AppState>, key: Option<String>, openrouter_key: Option<String>, provider: ProviderKind, models: Models, openrouter: ModelSet) -> CmdResult<()> {
    let some = |k: &str| if k.trim().is_empty() { None } else { Some(k.trim().to_string()) };
    if let Some(k) = key {
        crate::secrets::set_gemini_key(some(&k).as_deref())?;
    }
    if let Some(k) = openrouter_key {
        crate::secrets::set_openrouter_key(some(&k).as_deref())?;
    }
    let clean = |m: String, d: String| if m.trim().is_empty() { d } else { m.trim().to_string() };
    let (def, or_def) = (Models::default(), ModelSet::default());
    let mut s = state.settings.lock().unwrap();
    s.provider = provider;
    s.models = Models { engine: models.engine, transcribe: clean(models.transcribe, def.transcribe), generate: clean(models.generate, def.generate), fallback: models.fallback.trim().to_string() };
    s.openrouter = ModelSet { transcribe: clean(openrouter.transcribe, or_def.transcribe), generate: clean(openrouter.generate, or_def.generate), fallback: openrouter.fallback.trim().to_string() };
    s.save(&state.settings_path)?;
    Ok(())
}

fn gemini() -> CmdResult<Gemini> {
    match crate::secrets::gemini_key()? {
        Some(k) => Ok(Gemini::new(k)),
        None => Err(AppError::other("Manca la chiave API di Gemini: aggiungila in Impostazioni")),
    }
}

fn openrouter() -> CmdResult<OpenRouter> {
    match crate::secrets::openrouter_key()? {
        Some(k) => Ok(OpenRouter::new(k)),
        None => Err(AppError::other("Manca la chiave di OpenRouter: aggiungila in Impostazioni")),
    }
}

/// The provider chosen in Settings, with its fallback model.
fn provider(s: &Settings) -> CmdResult<Provider> {
    let p = match s.provider {
        ProviderKind::Gemini => Provider::Gemini(gemini()?),
        ProviderKind::OpenRouter => Provider::OpenRouter(openrouter()?),
    };
    Ok(p.with_fallback(&s.effective_models().fallback))
}

/// Checks a key (the one given, or the saved one); returns a line for Settings.
#[tauri::command]
pub async fn ai_check(provider: ProviderKind, key: Option<String>) -> CmdResult<String> {
    let key = key.filter(|k| !k.trim().is_empty()).map(|k| k.trim().to_string());
    Ok(match provider {
        ProviderKind::Gemini => {
            key.map(Gemini::new).map_or_else(gemini, Ok)?.check().await?;
            "Funziona".into()
        }
        ProviderKind::OpenRouter => key.map(OpenRouter::new).map_or_else(openrouter, Ok)?.check().await?,
    })
}

/// OpenRouter's price list (public, no key needed).
#[tauri::command]
pub async fn openrouter_models() -> CmdResult<Vec<crate::ai::openrouter::ModelInfo>> {
    Ok(OpenRouter::new("").models().await?.as_ref().clone())
}

/// Starts downloading the Whisper model in the background (from Settings).
#[tauri::command(async)]
pub fn whisper_download(state: State<'_, AppState>) {
    state.whisper.start_download();
}

#[tauri::command(async)]
pub fn whisper_status(state: State<'_, AppState>) -> crate::asr::ModelStatus {
    state.whisper.status()
}

#[tauri::command(async)]
pub fn whisper_cancel(state: State<'_, AppState>) {
    state.whisper.cancel_download();
}

#[tauri::command(async)]
pub fn whisper_delete(state: State<'_, AppState>) -> CmdResult<()> {
    Ok(state.whisper.delete()?)
}

/// Starts processing a lesson in the background; events on `channel`.
#[tauri::command]
pub async fn lesson_process(app: AppHandle, state: State<'_, AppState>, lesson_id: String, options: Options, job_id: String, channel: tauri::ipc::Channel<JobEvent>) -> CmdResult<()> {
    let dir = state.settings.lock().unwrap().library_dir.clone().ok_or_else(|| AppError::NoLibrary { message: "Scegli la cartella dell'archivio".into() })?;
    let settings = state.settings.lock().unwrap().clone();
    let models = settings.effective_models();
    let ch = channel.clone();
    let ai = crate::ai::Metered::new(provider(&settings)?.with_notify(Arc::new(move |message| {
        let _ = ch.send(JobEvent::Detail { message });
    })));
    let spent = ai.spent.clone();
    let archive = Archive { dir, lock: state.write.clone() };
    let cancel = state.jobs.start(&job_id, &lesson_id);
    let whisper = state.whisper.clone();
    let ch = channel.clone();
    let lesson_id2 = lesson_id.clone();
    let work = tauri::async_runtime::spawn(async move {
        // Whisper only when a recording is still to be transcribed: download (first time) and load.
        let lesson = archive.lesson(&lesson_id)?;
        let asr: Option<Arc<dyn crate::asr::Asr>> = if models.engine == Engine::Local && pipeline::needs_transcription(&lesson, options.force) {
            let _ = ch.send(JobEvent::Step { done: 0.0, total: 1, message: "Preparazione della trascrizione sul computer…".into() });
            let c = ch.clone();
            let notify = move |message: String| {
                let _ = c.send(JobEvent::Detail { message });
            };
            Some(whisper.get(&notify, &cancel).await?)
        } else {
            None
        };
        let ch2 = ch.clone();
        let progress = move |done: f64, total: u32, message: String| {
            log::info!("{message}");
            let _ = ch.send(JobEvent::Step { done, total, message });
        };
        let detail = move |message: String| {
            log::info!("{message}");
            let _ = ch2.send(JobEvent::Detail { message });
        };
        let ctx = pipeline::Ctx { ai: &ai, archive: &archive, models: &models, cancel: &cancel, progress: &progress, detail: &detail, window: pipeline::WINDOW_SECS, asr, fallbacks: Default::default() };
        ctx.run(&lesson_id, &options).await
    });
    tauri::async_runtime::spawn(async move {
        // The inner task is awaited separately so that a panic still reaches the UI.
        let ev = match work.await {
            Ok(Ok(r)) => JobEvent::Done { usage: r.usage, warnings: r.warnings },
            Ok(Err(e)) => {
                log::warn!("elaborazione: {e:#}");
                JobEvent::Failed { message: format!("{e:#}") }
            }
            Err(e) => {
                log::error!("elaborazione interrotta: {e}");
                JobEvent::Failed { message: format!("errore interno durante l'elaborazione ({e}): manda il file di log (Impostazioni → Log) a chi sviluppa l'app") }
            }
        };
        // What was spent is recorded also when the job failed or was cancelled.
        let state = app.state::<AppState>();
        let spent = std::mem::take(&mut *spent.lock().unwrap());
        if let Ok(lib) = state.library() {
            let _w = state.writing();
            if let Err(e) = crate::costs::record_lesson(&lib, &lesson_id2, "elaborazione", spent) {
                log::warn!("costi: {e:#}");
            }
        }
        let _ = channel.send(ev);
        state.jobs.finish(&job_id);
    });
    Ok(())
}

#[tauri::command(async)]
pub fn job_cancel(state: State<'_, AppState>, job_id: String) -> bool {
    state.jobs.cancel(&job_id)
}

// ---------- corrections by hand ----------

fn not_busy(state: &AppState, lesson_id: &str) -> CmdResult<()> {
    if state.jobs.busy(lesson_id) {
        return Err(anyhow::anyhow!("la lezione è in elaborazione: aspetta che finisca per correggere i testi").into());
    }
    Ok(())
}

#[tauri::command(async)]
pub fn extract_save(state: State<'_, AppState>, lesson_id: String, source_id: String, text: String) -> CmdResult<Lesson> {
    not_busy(&state, &lesson_id)?;
    let _w = state.writing();
    Ok(pipeline::save_extract(&state.library()?, &lesson_id, &source_id, &text)?)
}

#[tauri::command(async)]
pub fn summary_save(state: State<'_, AppState>, lesson_id: String, text: String) -> CmdResult<()> {
    not_busy(&state, &lesson_id)?;
    let _w = state.writing();
    Ok(pipeline::save_summary(&state.library()?, &lesson_id, &text)?)
}

#[tauri::command(async)]
pub fn study_save(state: State<'_, AppState>, lesson_id: String, study: pipeline::Study) -> CmdResult<()> {
    not_busy(&state, &lesson_id)?;
    let _w = state.writing();
    Ok(pipeline::save_study(&state.library()?, &lesson_id, &study)?)
}

#[tauri::command(async)]
pub fn lesson_outputs(state: State<'_, AppState>, id: String) -> CmdResult<Outputs> {
    Ok(pipeline::outputs(&state.library()?.lesson(&id)?))
}

// ---------- search and sharing ----------

use crate::search::{self, Hit};
use crate::share::{self, ImportReport, Include, OnExisting, PackageInfo};

#[tauri::command(async)]
pub fn search_archive(state: State<'_, AppState>, query: String) -> CmdResult<Vec<Hit>> {
    Ok(search::search(&state.library()?, &query)?)
}

#[tauri::command(async)]
pub fn export_package(state: State<'_, AppState>, lesson_ids: Vec<String>, include: Include, dest: PathBuf) -> CmdResult<()> {
    let _w = state.writing();
    Ok(share::export_package(&state.library()?, &lesson_ids, include, &dest)?)
}

#[tauri::command(async)]
pub fn package_inspect(state: State<'_, AppState>, path: PathBuf) -> CmdResult<PackageInfo> {
    Ok(share::inspect(&state.library()?, &path)?)
}

#[tauri::command(async)]
pub fn package_import(state: State<'_, AppState>, path: PathBuf, course_id: Option<String>, on_existing: OnExisting) -> CmdResult<ImportReport> {
    let _w = state.writing();
    Ok(share::import(&state.library()?, &path, course_id.as_deref(), on_existing)?)
}

#[tauri::command(async)]
pub fn export_markdown(state: State<'_, AppState>, lesson_ids: Vec<String>, dest: PathBuf) -> CmdResult<usize> {
    Ok(share::export_markdown(&state.library()?, &lesson_ids, &dest)?.len())
}

#[tauri::command(async)]
pub fn export_anki(state: State<'_, AppState>, lesson_ids: Vec<String>, dest: PathBuf) -> CmdResult<usize> {
    Ok(share::export_anki(&state.library()?, &lesson_ids, &dest)?)
}

/// A page built by the UI (single-file HTML export).
#[tauri::command(async)]
pub fn export_write_text(dest: PathBuf, text: String) -> CmdResult<()> {
    std::fs::write(&dest, text).map_err(|e| AppError::other(format!("scrittura di {}: {e}", dest.display())))
}

/// PDF: the page opens in the system browser, which shows its print dialog; there the user
/// chooses «Salva come PDF» (same result on Mac, Windows and Linux, formulas included).
#[tauri::command(async)]
pub fn export_print(app: AppHandle, name: String, html: String) -> CmdResult<PathBuf> {
    use tauri_plugin_opener::OpenerExt;
    let dir = std::env::temp_dir().join("lezioni-stampa");
    std::fs::create_dir_all(&dir).map_err(|e| AppError::other(e.to_string()))?;
    let p = dir.join(format!("{}.html", crate::library::safe_name(&name, 100)));
    let html = html.replacen("</body>", "<script>window.addEventListener('load',()=>setTimeout(()=>window.print(),400))</script></body>", 1);
    std::fs::write(&p, html).map_err(|e| AppError::other(e.to_string()))?;
    app.opener().open_path(p.to_string_lossy(), None::<&str>).map_err(|e| AppError::other(e.to_string()))?;
    Ok(p)
}

/// Packages opened with the app before the UI was ready (then emptied).
#[tauri::command(async)]
pub fn opened_packages(state: State<'_, AppState>) -> Vec<PathBuf> {
    std::mem::take(&mut *state.opened.lock().unwrap())
}

// ---------- chat ----------

use crate::chat::{self, Message, Scope};

#[tauri::command(async)]
pub fn chat_load(state: State<'_, AppState>, lesson_id: String) -> CmdResult<Vec<Message>> {
    Ok(chat::load(&state.library()?, &lesson_id)?)
}

#[tauri::command(async)]
pub fn chat_clear(state: State<'_, AppState>, lesson_id: String, scope: Scope) -> CmdResult<()> {
    let _w = state.writing();
    let lib = state.library()?;
    let keep: Vec<Message> = chat::load(&lib, &lesson_id)?.into_iter().filter(|m| m.scope != scope).collect();
    Ok(chat::save(&lib, &lesson_id, &keep)?)
}

#[derive(Serialize)]
pub struct ChatAnswer {
    messages: Vec<Message>,
    usage: crate::ai::Usage,
}

#[tauri::command]
pub async fn chat_ask(state: State<'_, AppState>, lesson_id: String, scope: Scope, question: String) -> CmdResult<ChatAnswer> {
    let settings = state.settings.lock().unwrap().clone();
    let model = settings.effective_models().generate;
    let ai = provider(&settings)?;
    let lib = state.library()?;
    let (messages, usage) = chat::ask(&ai, &model, &lib, &lesson_id, scope, &question, &tokio_util::sync::CancellationToken::new()).await?;
    {
        let _w = state.writing();
        if let Err(e) = crate::costs::record_lesson(&lib, &lesson_id, "chat", [(model, usage.clone())].into()) {
            log::warn!("costi: {e:#}");
        }
    }
    Ok(ChatAnswer { messages, usage })
}

#[tauri::command(async)]
pub fn course_costs(state: State<'_, AppState>, course_id: String) -> CmdResult<crate::costs::CourseCosts> {
    Ok(crate::costs::course(&state.library()?, &course_id)?)
}

// ---------- bookmarks ----------

#[tauri::command(async)]
pub fn bookmarks_list(state: State<'_, AppState>, lesson_id: String) -> CmdResult<Vec<crate::bookmarks::Bookmark>> {
    Ok(crate::bookmarks::list(&state.library()?, &lesson_id)?)
}

#[tauri::command(async)]
pub fn bookmark_add(state: State<'_, AppState>, lesson_id: String, source_id: String, at: f64, kind: crate::bookmarks::Kind, note: String) -> CmdResult<Vec<crate::bookmarks::Bookmark>> {
    let _w = state.writing();
    Ok(crate::bookmarks::add(&state.library()?, &lesson_id, &source_id, at, kind, &note)?)
}

#[tauri::command(async)]
pub fn bookmark_update(state: State<'_, AppState>, lesson_id: String, id: String, kind: crate::bookmarks::Kind, note: String) -> CmdResult<Vec<crate::bookmarks::Bookmark>> {
    let _w = state.writing();
    Ok(crate::bookmarks::update(&state.library()?, &lesson_id, &id, kind, &note)?)
}

#[tauri::command(async)]
pub fn bookmark_remove(state: State<'_, AppState>, lesson_id: String, id: String) -> CmdResult<Vec<crate::bookmarks::Bookmark>> {
    let _w = state.writing();
    Ok(crate::bookmarks::remove(&state.library()?, &lesson_id, &id)?)
}

// ---------- cleanup of a piece ----------

#[derive(Serialize)]
pub struct Recleaned {
    text: String,
    usage: crate::ai::Usage,
    model: String,
}

/// Cleans again a piece of a transcript from Whisper's text; the UI shows it before replacing.
#[tauri::command]
pub async fn extract_reclean(state: State<'_, AppState>, lesson_id: String, source_id: String, from: u64, to: Option<u64>, model: Option<String>) -> CmdResult<Recleaned> {
    not_busy(&state, &lesson_id)?;
    let settings = state.settings.lock().unwrap().clone();
    let model = model.filter(|m| !m.trim().is_empty()).unwrap_or_else(|| settings.effective_models().transcribe);
    let ai = provider(&settings)?;
    let lib = state.library()?;
    let (text, usage, model) = pipeline::reclean(&ai, &model, &lib, &lesson_id, &source_id, from, to, &tokio_util::sync::CancellationToken::new()).await?;
    {
        let _w = state.writing();
        if let Err(e) = crate::costs::record_lesson(&lib, &lesson_id, "ripulitura", [(model.clone(), usage.clone())].into()) {
            log::warn!("costi: {e:#}");
        }
    }
    Ok(Recleaned { text, usage, model })
}

// ---------- earlier versions ----------

#[tauri::command(async)]
pub fn version_read(state: State<'_, AppState>, lesson_id: String, area: crate::versions::Area, stamp: String, file: String) -> CmdResult<String> {
    Ok(crate::versions::read(&state.library()?, &lesson_id, area, &stamp, &file)?)
}

#[tauri::command(async)]
pub fn version_restore(state: State<'_, AppState>, lesson_id: String, area: crate::versions::Area, stamp: String, file: Option<String>) -> CmdResult<Lesson> {
    not_busy(&state, &lesson_id)?;
    let _w = state.writing();
    Ok(crate::versions::restore(&state.library()?, &lesson_id, area, &stamp, file.as_deref())?)
}
