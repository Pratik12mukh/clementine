#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod converter;

use converter::Kind;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

/// Run blocking work off the UI thread.
async fn blocking<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

fn paths_of(v: &[String]) -> Vec<PathBuf> {
    v.iter().map(PathBuf::from).collect()
}

// ------------------------------------------------------------ catalog ---

#[derive(Serialize)]
struct FieldOut {
    key: &'static str,
    label: &'static str,
    placeholder: &'static str,
}

#[derive(Serialize)]
struct ToolOut {
    id: &'static str,
    label: &'static str,
    title: &'static str,
    fields: Vec<FieldOut>,
    multi: bool,
}

#[derive(Serialize)]
struct Describe {
    /// image, video, audio, pdf, ... or "mixed" / "other" when only file actions apply
    kind: String,
    targets: Vec<String>,
    tools: Vec<ToolOut>,
}

fn describe_paths(paths: &[String]) -> Result<Describe, String> {
    if paths.is_empty() {
        return Err("No files".to_string());
    }
    let ps = paths_of(paths);
    let kinds: Vec<Kind> = ps.iter().map(|p| converter::kind_of_path(p)).collect();
    if kinds.iter().any(|k| *k != kinds[0]) {
        return Ok(Describe { kind: "mixed".into(), targets: vec![], tools: vec![] });
    }
    let kind = kinds[0];
    if kind == Kind::Other {
        return Ok(Describe { kind: "other".into(), targets: vec![], tools: vec![] });
    }
    // Targets every selected file can be converted to, in the first file's order.
    let first_ext = converter::ext_of(&ps[0]);
    let targets: Vec<String> = converter::targets_for(kind, &first_ext)
        .into_iter()
        .filter(|t| {
            ps.iter()
                .all(|p| converter::targets_for(kind, &converter::ext_of(p)).contains(t))
        })
        .map(|t| t.to_string())
        .collect();
    let tools = converter::tools_for(kind, &first_ext, ps.len())
        .into_iter()
        .filter(|t| {
            t.multi
                || ps.iter().all(|p| {
                    converter::tools_for(kind, &converter::ext_of(p), 1)
                        .iter()
                        .any(|x| x.id == t.id)
                })
        })
        .map(|t| ToolOut {
            id: t.id,
            label: t.label,
            title: t.title,
            multi: t.multi,
            fields: t
                .fields
                .iter()
                .map(|f| FieldOut { key: f.key, label: f.label, placeholder: f.placeholder })
                .collect(),
        })
        .collect();
    Ok(Describe { kind: kind.name().to_string(), targets, tools })
}

#[tauri::command]
async fn describe(paths: Vec<String>) -> Result<Describe, String> {
    blocking(move || describe_paths(&paths)).await
}

#[derive(Serialize)]
struct Stats {
    conversions: usize,
    tools: usize,
}

#[tauri::command]
fn stats() -> Stats {
    Stats { conversions: converter::conversion_option_count(), tools: converter::tool_count() }
}

// ------------------------------------------------------------ running ---

#[tauri::command]
async fn convert(path: String, target: String) -> Result<String, String> {
    blocking(move || converter::convert(&PathBuf::from(path), &target).map(|p| p.display().to_string())).await
}

#[tauri::command]
async fn run_tool(path: String, name: String, a: Option<String>, b: Option<String>) -> Result<String, String> {
    blocking(move || {
        converter::tool(&PathBuf::from(path), &name, a.as_deref(), b.as_deref()).map(|p| p.display().to_string())
    })
    .await
}

#[tauri::command]
async fn run_multi(paths: Vec<String>, name: String) -> Result<String, String> {
    blocking(move || converter::tool_multi(&paths_of(&paths), &name).map(|p| p.display().to_string())).await
}

#[tauri::command]
async fn file_action(path: String, name: String, dest: Option<String>) -> Result<String, String> {
    blocking(move || converter::file_action(Path::new(&path), &name, dest.as_deref())).await
}

// ------------------------------------------------------ helper tools ---

#[derive(Serialize)]
struct ToolStatus {
    id: &'static str,
    label: &'static str,
    found: bool,
    used_for: &'static str,
}

#[tauri::command]
async fn tool_status() -> Result<Vec<ToolStatus>, String> {
    blocking(|| {
        Ok(converter::tool_status()
            .into_iter()
            .map(|t| ToolStatus { id: t.id, label: t.label, found: t.found, used_for: t.used_for })
            .collect())
    })
    .await
}

#[tauri::command]
async fn install_tool(id: String) -> Result<String, String> {
    blocking(move || converter::install_tool(&id)).await
}

// ------------------------------------------------------------ window ---

#[tauri::command]
fn set_float(window: tauri::WebviewWindow, on: bool) -> Result<(), String> {
    window.set_always_on_top(on).map_err(|e| e.to_string())
}

#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    blocking(move || {
        Ok(app
            .dialog()
            .file()
            .set_title("Choose a folder")
            .blocking_pick_folder()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.display().to_string()))
    })
    .await
}

// -------------------------------------------- live modifier keys ---

/// Windows does not deliver key events to a page while an OS file drag is in
/// progress, so Shift / Ctrl / Alt are read straight from the system and sent
/// to the page as a "modifiers" event whenever they change.
#[cfg(windows)]
mod keys {
    #[link(name = "user32")]
    extern "system" {
        fn GetAsyncKeyState(vk: i32) -> i16;
    }
    fn down(vk: i32) -> bool {
        // High bit set means the key is currently down.
        unsafe { (GetAsyncKeyState(vk) as u16) & 0x8000 != 0 }
    }
    pub fn read() -> (bool, bool, bool) {
        (down(0x10), down(0x11), down(0x12)) // VK_SHIFT, VK_CONTROL, VK_MENU
    }
}

#[cfg(not(windows))]
mod keys {
    pub fn read() -> (bool, bool, bool) {
        (false, false, false)
    }
}

#[derive(Serialize, Clone, PartialEq, Copy)]
struct Mods {
    shift: bool,
    ctrl: bool,
    alt: bool,
}

fn start_modifier_watcher(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut last = Mods { shift: false, ctrl: false, alt: false };
        loop {
            let (shift, ctrl, alt) = keys::read();
            let now = Mods { shift, ctrl, alt };
            if now != last {
                last = now;
                let _ = app.emit("modifiers", now);
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    });
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            start_modifier_watcher(app.handle().clone());
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_focus();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            describe,
            stats,
            convert,
            run_tool,
            run_multi,
            file_action,
            tool_status,
            install_tool,
            set_float,
            pick_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running Clementine");
}
