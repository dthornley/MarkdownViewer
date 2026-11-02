// Markdown Viewer desktop app: a thin Tauri shell around index.html.
//
// The frontend is the same single page the `mdview` CLI serves. Each document gets
// its own window (macOS can show those as tabs); the page reads its document through
// the `mdv://` protocol below (so relative images and links resolve exactly like they
// do over HTTP), gets change notifications from a notify watcher, and receives files
// opened from Finder, the command line, the Open Recent menu or the last session.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{
    http::{Request, Response},
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window,
};

const MAX_RECENT: usize = 10;

#[derive(Default)]
struct AppState {
    /// Window label -> document shown in it. Windows without a document are not listed.
    docs: Mutex<HashMap<String, String>>,
    /// Window label -> file handed to it before its page was ready to receive events.
    pending: Mutex<HashMap<String, String>>,
    /// Window label -> watcher for its document; replaced whenever the document changes.
    watchers: Mutex<HashMap<String, RecommendedWatcher>>,
    /// Most recently opened first.
    recent: Mutex<Vec<String>>,
    /// View › Open Documents in Tabs.
    tabs: AtomicBool,
    /// Set once the app is quitting, so closing windows stop rewriting the session.
    quitting: AtomicBool,
    counter: AtomicUsize,
}

// ---------- small JSON files in the app data directory ----------

fn data_file(app: &AppHandle, name: &str) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join(name))
}

fn read_json<T: serde::de::DeserializeOwned + Default>(app: &AppHandle, name: &str) -> T {
    data_file(app, name)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn write_json<T: serde::Serialize>(app: &AppHandle, name: &str, value: &T) {
    if let Some(p) = data_file(app, name) {
        if let Err(e) = std::fs::write(&p, serde_json::to_vec_pretty(value).unwrap_or_default()) {
            log::warn!("could not write {}: {e}", p.display());
        }
    }
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Settings {
    tabs: bool,
}

fn save_session(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.quitting.load(Ordering::Relaxed) {
        return;
    }
    let mut paths: Vec<String> = state.docs.lock().unwrap().values().cloned().collect();
    paths.sort();
    write_json(app, "session.json", &paths);
}

fn add_recent(app: &AppHandle, path: &str) {
    let state = app.state::<AppState>();
    {
        let mut recent = state.recent.lock().unwrap();
        recent.retain(|p| p != path);
        recent.insert(0, path.to_string());
        recent.truncate(MAX_RECENT);
        write_json(app, "recent.json", &*recent);
    }
    if let Err(e) = build_menu(app) {
        log::warn!("menu rebuild failed: {e}");
    }
}

// ---------- windows ----------

#[cfg(target_os = "macos")]
fn apply_tabbing(window: &WebviewWindow, tabs: bool) {
    use objc2_app_kit::{NSWindow, NSWindowTabbingMode};
    if let Ok(ptr) = window.ns_window() {
        let ns: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
        ns.setTabbingMode(if tabs { NSWindowTabbingMode::Preferred } else { NSWindowTabbingMode::Automatic });
    }
}

/// Create a document window. With `path`, the page picks the file up as soon as it is ready.
fn new_window(app: &AppHandle, path: Option<String>) {
    let state = app.state::<AppState>();
    let n = state.counter.fetch_add(1, Ordering::Relaxed);
    let label = format!("doc-{n}");
    if let Some(p) = &path {
        state.pending.lock().unwrap().insert(label.clone(), p.clone());
        state.docs.lock().unwrap().insert(label.clone(), p.clone());
    }
    let offset = 40.0 + 28.0 * ((n % 8) as f64);
    let builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title("Markdown Viewer")
        .inner_size(1100.0, 800.0)
        .min_inner_size(480.0, 320.0)
        .position(offset, offset)
        .visible(false);
    #[cfg(target_os = "macos")]
    let builder = builder.tabbing_identifier("com.thornley.markdownviewer.document");
    match builder.build() {
        Ok(window) => {
            let tabs = state.tabs.load(Ordering::Relaxed);
            let w = window.clone();
            // Tabbing mode must be set before the window is first shown for AppKit to group it.
            let _ = window.run_on_main_thread(move || {
                #[cfg(target_os = "macos")]
                apply_tabbing(&w, tabs);
                #[cfg(not(target_os = "macos"))]
                let _ = tabs;
                let _ = w.show();
                let _ = w.set_focus();
            });
            log::info!("window {label}: {}", path.as_deref().unwrap_or("(empty)"));
            if path.is_some() {
                save_session(app);
            }
        }
        Err(e) => log::error!("could not create window: {e}"),
    }
}

fn set_tabs(app: &AppHandle, tabs: bool) {
    let state = app.state::<AppState>();
    state.tabs.store(tabs, Ordering::Relaxed);
    write_json(app, "settings.json", &Settings { tabs });
    #[cfg(target_os = "macos")]
    for window in app.webview_windows().values() {
        apply_tabbing(window, tabs);
    }
}

/// Show `path`: focus the window already showing it, reuse an empty window, or open a new one.
fn open_document(app: &AppHandle, path: PathBuf) {
    let path = path.canonicalize().unwrap_or(path);
    if !path.is_file() {
        log::warn!("not a file: {}", path.display());
        return;
    }
    let path = path.to_string_lossy().into_owned();
    let state = app.state::<AppState>();
    let windows = app.webview_windows();
    let (existing, empty) = {
        let docs = state.docs.lock().unwrap();
        let existing = docs.iter().find(|(_, p)| **p == path).map(|(l, _)| l.clone());
        let mut empties: Vec<&String> = windows.keys().filter(|l| !docs.contains_key(*l)).collect();
        empties.sort_by_key(|l| !windows[*l].is_focused().unwrap_or(false)); // focused first
        (existing, empties.first().map(|l| (*l).clone()))
    };
    if let Some(label) = existing {
        if let Some(w) = windows.get(&label) {
            let _ = w.set_focus();
            return;
        }
    }
    match empty {
        Some(label) => {
            log::info!("open {path} in empty window {label}");
            state.pending.lock().unwrap().insert(label.clone(), path.clone());
            let _ = windows[&label].set_focus();
            let _ = app.emit_to(&label, "open-file", &path);
        }
        None => new_window(app, Some(path)),
    }
}

/// The window whose menu shortcut or menu click this is.
fn focused_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.webview_windows().into_values().find(|w| w.is_focused().unwrap_or(false))
}

// ---------- commands ----------

#[tauri::command]
fn take_pending_file(window: Window, state: State<'_, AppState>) -> Option<String> {
    state.pending.lock().unwrap().remove(window.label())
}

/// Watch `path` (via its parent directory, so editors that save by rename are seen too)
/// and emit `file-changed` to this window on every event touching it. The page debounces.
/// Called whenever a window starts showing a document, so this also records it.
#[tauri::command]
fn watch_file(app: AppHandle, window: Window, state: State<'_, AppState>, path: String) -> Result<(), String> {
    let target = PathBuf::from(&path);
    let dir = target.parent().ok_or("file has no parent directory")?.to_path_buf();
    let name = target.file_name().ok_or("path has no file name")?.to_os_string();
    let label = window.label().to_string();
    let (emit_app, emit_label, emit_path) = (app.clone(), label.clone(), path.clone());
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            if ev.paths.iter().any(|p| p.file_name() == Some(name.as_os_str())) {
                log::debug!("changed: {emit_path} ({:?})", ev.kind);
                let _ = emit_app.emit_to(&emit_label, "file-changed", &emit_path);
            }
        }
    })
    .map_err(|e| e.to_string())?;
    watcher.watch(&dir, RecursiveMode::NonRecursive).map_err(|e| e.to_string())?;
    log::info!("{label} watching {}", target.display());
    state.watchers.lock().unwrap().insert(label.clone(), watcher); // drops the previous watcher
    state.docs.lock().unwrap().insert(label, path.clone());
    save_session(&app);
    add_recent(&app, &path);
    Ok(())
}

/// Native open dialog; the chosen file goes to this window if it is empty, else to a new one.
#[tauri::command]
async fn open_dialog(app: AppHandle) {
    use tauri_plugin_dialog::DialogExt;
    let picked = app
        .dialog()
        .file()
        .add_filter("Markdown", &["md", "markdown", "mdown", "mkd", "txt"])
        .blocking_pick_files();
    for file in picked.unwrap_or_default() {
        if let Ok(p) = file.into_path() {
            open_document(&app, p);
        }
    }
}

// ---------- mdv:// protocol ----------

fn mime_for(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("md") | Some("markdown") | Some("mdown") | Some("mkd") => "text/markdown; charset=utf-8",
        Some("txt") => "text/plain; charset=utf-8",
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("pdf") => "application/pdf",
        Some("css") => "text/css",
        Some("js") | Some("mjs") => "text/javascript",
        Some("json") => "application/json",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mp3") => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

/// `mdv://localhost/<absolute path>` serves any local file, so the page can fetch the
/// document and resolve relative images and links against it with ordinary URL logic.
fn serve_local_file(request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let path = percent_encoding::percent_decode_str(request.uri().path())
        .decode_utf8_lossy()
        .into_owned();
    #[cfg(windows)]
    let path = path.trim_start_matches('/').to_string();
    // Only the viewer's own webview origin may read these responses.
    #[cfg(not(windows))]
    let origin = "tauri://localhost";
    #[cfg(windows)]
    let origin = "http://tauri.localhost";
    let builder = Response::builder().header("Access-Control-Allow-Origin", origin);
    let result = std::fs::read(&path);
    log::debug!("mdv:// {} -> {}", path, if result.is_ok() { "200" } else { "404" });
    match result {
        Ok(bytes) => builder
            .status(200)
            .header("Content-Type", mime_for(&path))
            .header("Cache-Control", "no-store")
            .body(bytes)
            .unwrap(),
        Err(e) => builder
            .status(404)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(format!("{path}: {e}").into_bytes())
            .unwrap(),
    }
}

// ---------- default app for Markdown (macOS) ----------

/// Ask Launch Services to make this app the default for Markdown files: the same as
/// Finder's Get Info › Open with › Change All. macOS shows its own confirmation prompt per
/// type, so types are set one after another; `done` gets the outcome once all are through.
#[cfg(target_os = "macos")]
fn make_default_app(app: &AppHandle, done: impl Fn(Result<String, String>) + Send + Sync + 'static) {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSString, NSURL};
    use objc2_uniform_type_identifiers::UTType;
    use std::sync::{Arc, Mutex};

    const EXTENSIONS: [&str; 4] = ["md", "markdown", "mdown", "mkd"];
    // Markdown Viewer.app/Contents/MacOS/markdown-viewer -> Markdown Viewer.app
    let Some(bundle) = std::env::current_exe().ok().and_then(|p| p.ancestors().nth(3).map(Path::to_path_buf)) else {
        return done(Err("could not locate the app bundle".into()));
    };
    // The public Markdown type, plus the type each extension resolves to on this Mac, but only
    // when that type really is a Markdown type: an installed editor may map .mdown to its own
    // type that also claims .txt and .text, and we must not take those over.
    let mut types: Vec<String> = vec!["net.daringfireball.markdown".into()];
    for ext in EXTENSIONS {
        if let Some(t) = UTType::typeWithFilenameExtension(&NSString::from_str(ext)) {
            let id = t.identifier().to_string();
            let preferred = t.preferredFilenameExtension().map(|e| e.to_string().to_lowercase()).unwrap_or_default();
            let is_markdown = id.to_lowercase().contains("markdown") || EXTENSIONS.contains(&preferred.as_str());
            if id.starts_with("dyn.") || !is_markdown || types.contains(&id) {
                log::info!("skipping type {id} for .{ext} (preferred extension .{preferred})");
                continue;
            }
            types.push(id);
        }
    }
    log::info!("setting default app for {types:?} to {}", bundle.display());

    struct Job { bundle: PathBuf, types: Vec<String>, failures: Mutex<Vec<String>>, done: Box<dyn Fn(Result<String, String>) + Send + Sync> }
    fn step(job: Arc<Job>, index: usize) {
        if index >= job.types.len() {
            let failures = job.failures.lock().unwrap();
            let hint = if job.bundle.starts_with("/Applications") { "" } else { "\n\nMove Markdown Viewer to the Applications folder and try again." };
            return (job.done)(if failures.is_empty() {
                Ok(format!("Markdown Viewer is now the default app for Markdown files ({} type{}). Double-clicking a .md file in Finder opens it here.", job.types.len(), if job.types.len() == 1 { "" } else { "s" }))
            } else {
                Err(format!("Not changed for:\n{}{hint}", failures.join("\n")))
            });
        }
        let Some(uti) = UTType::typeWithIdentifier(&NSString::from_str(&job.types[index])) else {
            return step(job, index + 1);
        };
        let url = NSURL::fileURLWithPath(&NSString::from_str(&job.bundle.to_string_lossy()));
        let next = job.clone();
        let handler = block2::RcBlock::new(move |err: *mut objc2_foundation::NSError| {
            if !err.is_null() {
                let msg = unsafe { (*err).localizedDescription() }.to_string();
                log::warn!("default app for {}: {msg}", next.types[index]);
                next.failures.lock().unwrap().push(format!("• {} — {msg}", next.types[index]));
            }
            step(next.clone(), index + 1);
        });
        NSWorkspace::sharedWorkspace().setDefaultApplicationAtURL_toOpenContentType_completionHandler(&url, &uti, Some(&handler));
    }
    let _ = app;
    step(Arc::new(Job { bundle, types, failures: Mutex::new(Vec::new()), done: Box::new(done) }), 0);
}

// ---------- menu ----------

fn build_menu(app: &AppHandle) -> tauri::Result<()> {
    let state = app.state::<AppState>();
    let new_win = MenuItemBuilder::with_id("new", "New Window").accelerator("CmdOrCtrl+N").build(app)?;
    let open = MenuItemBuilder::with_id("open", "Open…").accelerator("CmdOrCtrl+O").build(app)?;
    let reload = MenuItemBuilder::with_id("reload", "Reload").accelerator("CmdOrCtrl+R").build(app)?;
    let print = MenuItemBuilder::with_id("print", "Print…").accelerator("CmdOrCtrl+P").build(app)?;
    let toc = MenuItemBuilder::with_id("toc", "Toggle Table of Contents").accelerator("CmdOrCtrl+Shift+T").build(app)?;
    let theme = MenuItemBuilder::with_id("theme", "Toggle Dark Mode").accelerator("CmdOrCtrl+Shift+D").build(app)?;
    let update = MenuItemBuilder::with_id("update", "Check for Updates…").build(app)?;
    #[cfg(target_os = "macos")]
    let default_app = MenuItemBuilder::with_id("default-app", "Make Default Markdown Viewer").build(app)?;
    let back = MenuItemBuilder::with_id("back", "Back").accelerator("CmdOrCtrl+[").build(app)?;
    let forward = MenuItemBuilder::with_id("forward", "Forward").accelerator("CmdOrCtrl+]").build(app)?;
    let zoom_in = MenuItemBuilder::with_id("zoom-in", "Zoom In").accelerator("CmdOrCtrl+=").build(app)?;
    let zoom_out = MenuItemBuilder::with_id("zoom-out", "Zoom Out").accelerator("CmdOrCtrl+-").build(app)?;
    let zoom_reset = MenuItemBuilder::with_id("zoom-reset", "Actual Size").accelerator("CmdOrCtrl+0").build(app)?;
    let tabs = CheckMenuItemBuilder::with_id("tabs", "Open Documents in Tabs")
        .checked(state.tabs.load(Ordering::Relaxed))
        .build(app)?;

    let mut recent_menu = SubmenuBuilder::new(app, "Open Recent");
    let recent = state.recent.lock().unwrap().clone();
    for path in &recent {
        let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.clone());
        recent_menu = recent_menu.item(&MenuItemBuilder::with_id(format!("recent:{path}"), name).build(app)?);
    }
    if !recent.is_empty() {
        recent_menu = recent_menu.separator();
    }
    let clear = MenuItemBuilder::with_id("recent-clear", "Clear Menu").enabled(!recent.is_empty()).build(app)?;
    let recent_menu = recent_menu.item(&clear).build()?;

    let file = SubmenuBuilder::new(app, "File")
        .item(&new_win).item(&open).item(&recent_menu).item(&reload).separator().item(&print).separator().close_window()
        .build()?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .undo().redo().separator().cut().copy().paste().select_all()
        .build()?;
    let view = SubmenuBuilder::new(app, "View")
        .item(&back).item(&forward).separator().item(&toc).item(&theme).separator().item(&zoom_in).item(&zoom_out).item(&zoom_reset).separator().item(&tabs).separator().fullscreen();
    #[cfg(not(target_os = "macos"))]
    let view = view.separator().item(&update);
    let view = view.build()?;
    let window = SubmenuBuilder::new(app, "Window").minimize().maximize().separator().build()?;
    #[cfg(target_os = "macos")]
    window.set_as_windows_menu_for_nsapp()?; // AppKit adds the window list and tab commands here

    let mut menu = MenuBuilder::new(app);
    #[cfg(target_os = "macos")]
    {
        let app_menu = SubmenuBuilder::new(app, "Markdown Viewer")
            .about(None).item(&update).item(&default_app).separator().services().separator().hide().hide_others().show_all().separator().quit()
            .build()?;
        menu = menu.item(&app_menu);
    }
    let menu = menu.item(&file).item(&edit).item(&view).item(&window).build()?;
    app.set_menu(menu)?;
    Ok(())
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "new" => new_window(app, None),
        "open" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { open_dialog(app).await });
        }
        #[cfg(target_os = "macos")]
        "default-app" => {
            use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
            let app = app.clone();
            make_default_app(&app.clone(), move |result| {
                let (text, kind) = match result {
                    Ok(msg) => (msg, MessageDialogKind::Info),
                    Err(e) => (format!("The default app was not changed.\n\n{e}"), MessageDialogKind::Error),
                };
                log::info!("default app result: {text}");
                app.dialog().message(text).title("Default Markdown Viewer").kind(kind).show(|_| {});
            });
        }
        "recent-clear" => {
            app.state::<AppState>().recent.lock().unwrap().clear();
            write_json(app, "recent.json", &Vec::<String>::new());
            let _ = build_menu(app);
        }
        "tabs" => {
            let tabs = !app.state::<AppState>().tabs.load(Ordering::Relaxed);
            set_tabs(app, tabs);
            let _ = build_menu(app);
        }
        _ if id.starts_with("recent:") => open_document(app, PathBuf::from(&id["recent:".len()..])),
        // Everything else acts on the document in the front window.
        _ => {
            if let Some(w) = focused_window(app) {
                let _ = app.emit_to(w.label(), "menu", id);
            }
        }
    }
}

// ---------- app ----------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState::default())
        .register_uri_scheme_protocol("mdv", |_ctx, request| serve_local_file(request))
        .invoke_handler(tauri::generate_handler![take_pending_file, watch_file, open_dialog])
        .on_window_event(|window, event| {
            let app = window.app_handle();
            let state = app.state::<AppState>();
            match event {
                // The user closed this window: forget its document (the session must not reopen it).
                tauri::WindowEvent::CloseRequested { .. } => {
                    log::info!("{} closed", window.label());
                    state.docs.lock().unwrap().remove(window.label());
                    save_session(app);
                }
                tauri::WindowEvent::Destroyed => {
                    state.docs.lock().unwrap().remove(window.label());
                    state.watchers.lock().unwrap().remove(window.label());
                    state.pending.lock().unwrap().remove(window.label());
                }
                _ => {}
            }
        })
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Debug)
                        .level_for("tao", log::LevelFilter::Warn)
                        .level_for("wry", log::LevelFilter::Warn)
                        .targets([
                            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
                        ])
                        .build(),
                )?;
            }
            let handle = app.handle().clone();
            {
                let state = app.state::<AppState>();
                *state.recent.lock().unwrap() = read_json::<Vec<String>>(&handle, "recent.json");
                state.tabs.store(read_json::<Settings>(&handle, "settings.json").tabs, Ordering::Relaxed);
            }
            build_menu(&handle)?;
            handle.on_menu_event(|app, ev| on_menu(app, &ev.id().0));

            // Files named on the command line; otherwise the documents open when the app last quit.
            let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).filter(|p| p.is_file()).collect();
            let paths = if args.is_empty() {
                read_json::<Vec<String>>(&handle, "session.json").into_iter().map(PathBuf::from).filter(|p| p.is_file()).collect()
            } else {
                args
            };
            if paths.is_empty() {
                new_window(&handle, None);
            } else {
                for p in paths {
                    open_document(&handle, p);
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| match event {
        // Finder double-click, drag onto the Dock icon, `open -a` on macOS.
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        tauri::RunEvent::Opened { urls } => {
            for path in urls.iter().filter_map(|u| u.to_file_path().ok()) {
                open_document(app, path);
            }
        }
        // Dock icon clicked with no windows left.
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { has_visible_windows, .. } => {
            if !has_visible_windows {
                new_window(app, None);
            }
        }
        tauri::RunEvent::ExitRequested { code, api, .. } => {
            if code.is_none() {
                // Last window closed: stay running like other Mac document apps; Dock click reopens.
                #[cfg(target_os = "macos")]
                api.prevent_exit();
                #[cfg(not(target_os = "macos"))]
                let _ = api;
            } else {
                // Quitting: the open documents come back next launch.
                save_session(app);
                app.state::<AppState>().quitting.store(true, Ordering::Relaxed);
            }
        }
        _ => {}
    });
}
