// Markdown Viewer desktop app: a thin Tauri shell around index.html.
//
// The frontend is the same single page the `mdview` CLI serves. In the app it
// reads documents through the `mdv://` protocol below (so relative images and
// links resolve exactly like they do over HTTP), gets change notifications from
// a notify watcher, and receives files opened from Finder / the command line.

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{
    http::{Request, Response},
    menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    AppHandle, Emitter, Manager, State,
};

#[derive(Default)]
struct AppState {
    /// File the app was asked to open before the page was ready to receive events.
    pending: Mutex<Option<String>>,
    /// Watcher for the document currently shown; replaced whenever the document changes.
    watcher: Mutex<Option<RecommendedWatcher>>,
}

/// Remember `path` for the page to pick up, and tell it right away if it is already listening.
fn open_in_frontend(app: &AppHandle, path: String) {
    log::info!("open requested: {path}");
    *app.state::<AppState>().pending.lock().unwrap() = Some(path.clone());
    let _ = app.emit("open-file", path);
}

#[tauri::command]
fn take_pending_file(state: State<'_, AppState>) -> Option<String> {
    state.pending.lock().unwrap().take()
}

/// Watch `path` (via its parent directory, so editors that save by rename are seen too)
/// and emit `file-changed` on every event touching it. The page debounces.
#[tauri::command]
fn watch_file(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<(), String> {
    let target = PathBuf::from(&path);
    let dir = target.parent().ok_or("file has no parent directory")?.to_path_buf();
    let name = target.file_name().ok_or("path has no file name")?.to_os_string();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            if ev.paths.iter().any(|p| p.file_name() == Some(name.as_os_str())) {
                log::debug!("changed: {path} ({:?})", ev.kind);
                let _ = app.emit("file-changed", &path);
            }
        }
    })
    .map_err(|e| e.to_string())?;
    watcher.watch(&dir, RecursiveMode::NonRecursive).map_err(|e| e.to_string())?;
    log::info!("watching {}", target.display());
    *state.watcher.lock().unwrap() = Some(watcher); // drops the previous watcher
    Ok(())
}

#[tauri::command]
async fn pick_file(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog()
        .file()
        .add_filter("Markdown", &["md", "markdown", "mdown", "mkd", "txt"])
        .blocking_pick_file()
        .and_then(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
}

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

fn build_menu(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id("open", "Open…").accelerator("CmdOrCtrl+O").build(app)?;
    let reload = MenuItemBuilder::with_id("reload", "Reload").accelerator("CmdOrCtrl+R").build(app)?;
    let print = MenuItemBuilder::with_id("print", "Print…").accelerator("CmdOrCtrl+P").build(app)?;
    let toc = MenuItemBuilder::with_id("toc", "Toggle Table of Contents").accelerator("CmdOrCtrl+Shift+T").build(app)?;
    let theme = MenuItemBuilder::with_id("theme", "Toggle Dark Mode").accelerator("CmdOrCtrl+Shift+D").build(app)?;
    let update = MenuItemBuilder::with_id("update", "Check for Updates…").build(app)?;

    let file = SubmenuBuilder::new(app, "File")
        .item(&open).item(&reload).separator().item(&print).separator().close_window()
        .build()?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .undo().redo().separator().cut().copy().paste().select_all()
        .build()?;
    #[cfg(target_os = "macos")]
    let view = SubmenuBuilder::new(app, "View").item(&toc).item(&theme).separator().fullscreen().build()?;
    #[cfg(not(target_os = "macos"))]
    let view = SubmenuBuilder::new(app, "View").item(&toc).item(&theme).separator().fullscreen().separator().item(&update).build()?;
    let window = SubmenuBuilder::new(app, "Window").minimize().maximize().build()?;

    let mut menu = MenuBuilder::new(app);
    #[cfg(target_os = "macos")]
    {
        let app_menu = SubmenuBuilder::new(app, "Markdown Viewer")
            .about(None).item(&update).separator().services().separator().hide().hide_others().show_all().separator().quit()
            .build()?;
        menu = menu.item(&app_menu);
    }
    let menu = menu.item(&file).item(&edit).item(&view).item(&window).build()?;
    app.set_menu(menu)?;
    app.on_menu_event(|app, ev| {
        let _ = app.emit("menu", ev.id().0.clone());
    });
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState::default())
        .register_uri_scheme_protocol("mdv", |_ctx, request| serve_local_file(request))
        .invoke_handler(tauri::generate_handler![take_pending_file, watch_file, pick_file])
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
            build_menu(app.handle())?;
            // `Markdown Viewer.app/Contents/MacOS/markdown-viewer file.md`, or a file argument on Windows/Linux.
            if let Some(arg) = std::env::args().nth(1) {
                let p = PathBuf::from(&arg);
                if p.is_file() {
                    let abs = p.canonicalize().unwrap_or(p);
                    *app.state::<AppState>().pending.lock().unwrap() = Some(abs.to_string_lossy().into_owned());
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| {
        // Finder double-click, drag onto the Dock icon, `open -a` on macOS.
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        if let tauri::RunEvent::Opened { urls } = event {
            if let Some(path) = urls.iter().filter_map(|u| u.to_file_path().ok()).next() {
                open_in_frontend(app, path.to_string_lossy().into_owned());
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        let _ = (app, event);
    });
}
