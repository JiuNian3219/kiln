#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
mod agent;
mod agent_protocol;
mod clipboard;
mod credential;
mod deepseek;
mod session;
mod settings;
mod workspace;

use credential::WindowsCredentialStore;
use session::{ClarificationPayload, SessionInput};
use settings::{
    CatalogEntry, CatalogRepository, Settings, SettingsInput, SettingsPayload, SettingsRepository,
    discover_catalog,
};

struct AppState {
    pending: Mutex<Option<PendingSession>>,
    reference_text: Mutex<Option<String>>,
    reference_shortcut: Mutex<String>,
    settings: Mutex<Settings>,
}

#[derive(Clone)]
struct PendingSession {
    target: isize,
    original: String,
    reference_text: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewPayload {
    original: String,
    replacement: String,
    agents: Vec<CatalogEntry>,
    knowledge_bases: Vec<CatalogEntry>,
    selected_agent_id: String,
    selected_knowledge_base_ids: Vec<String>,
    use_agent: bool,
    use_knowledge_base: bool,
    use_network: bool,
    network_available: bool,
    reference_text: Option<String>,
    reference_active: bool,
}

impl Default for AppState {
    fn default() -> Self {
        let settings = SettingsRepository::load().unwrap_or_default();
        Self {
            pending: Mutex::new(None),
            reference_text: Mutex::new(None),
            reference_shortcut: Mutex::new(String::new()),
            settings: Mutex::new(settings),
        }
    }
}

fn preview_payload(
    original: String,
    replacement: String,
    settings: &Settings,
    reference_text: Option<String>,
) -> PreviewPayload {
    let agents = discover_catalog(&settings.agents_root, "AGENT.md");
    let knowledge_bases = discover_catalog(&settings.knowledge_bases_root, "INDEX.md");
    let selected_agent_id = if agents
        .iter()
        .any(|entry| entry.id == settings.default_agent)
    {
        settings.default_agent.clone()
    } else {
        String::new()
    };
    let selected_knowledge_base_ids = if knowledge_bases
        .iter()
        .any(|entry| entry.id == settings.default_knowledge_base)
    {
        vec![settings.default_knowledge_base.clone()]
    } else {
        Vec::new()
    };
    PreviewPayload {
        original,
        replacement,
        use_agent: !selected_agent_id.is_empty(),
        use_knowledge_base: !selected_knowledge_base_ids.is_empty(),
        use_network: false,
        network_available: settings.allow_network,
        agents,
        knowledge_bases,
        selected_agent_id,
        selected_knowledge_base_ids,
        reference_active: reference_text.is_some(),
        reference_text,
    }
}

fn show_preview(
    window: &WebviewWindow,
    payload: PreviewPayload,
) -> std::result::Result<(), String> {
    window
        .emit("selection-captured", payload)
        .map_err(|error| error.to_string())?;
    window.center().map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

async fn capture_reference_text(mode: &str) -> std::result::Result<(Option<String>, bool), String> {
    if mode == "clipboard" {
        let text = tauri::async_runtime::spawn_blocking(clipboard::read_clipboard_text_on_worker)
            .await
            .map_err(|error| format!("Reference worker failed: {error}"))?
            .map_err(|error| error.message().to_string())?;
        return Ok((text, false));
    }
    let captured = tauri::async_runtime::spawn_blocking(clipboard::capture_selection_on_worker)
        .await
        .map_err(|error| format!("Reference worker failed: {error}"))?
        .map_err(|error| error.message().to_string())?;
    if let Some(captured) = captured {
        return Ok((Some(captured.text), true));
    }
    let text = tauri::async_runtime::spawn_blocking(clipboard::read_clipboard_text_on_worker)
        .await
        .map_err(|error| format!("Reference worker failed: {error}"))?
        .map_err(|error| error.message().to_string())?;
    Ok((text, false))
}

fn show_reference_toast(app: &AppHandle) -> std::result::Result<(), String> {
    if let Some(existing) = app.get_webview_window("reference-toast") {
        let _ = existing.close();
    }
    let toast =
        WebviewWindowBuilder::new(app, "reference-toast", WebviewUrl::App("toast.html".into()))
            .title("Reference saved")
            .inner_size(300.0, 50.0)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .focusable(false)
            .build()
            .map_err(|error| error.to_string())?;
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let position = monitor.position();
        let size = monitor.size();
        let _ = toast.set_position(PhysicalPosition::new(
            position.x + size.width as i32 - 316,
            position.y + size.height as i32 - 86,
        ));
    }
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(2550));
        let _ = toast.close();
    });
    Ok(())
}

async fn save_reference_inner(
    app: &AppHandle,
    state: &AppState,
) -> std::result::Result<(), String> {
    let mode = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?
        .reference_capture_mode
        .clone();
    let (text, show_toast) = capture_reference_text(&mode).await?;
    let Some(text) = text else {
        return Ok(());
    };
    if text.len() > 100_000 {
        return Err("Reference text is too large.".to_string());
    }
    *state
        .reference_text
        .lock()
        .map_err(|_| "Reference lock failed.".to_string())? = Some(text);
    if show_toast {
        show_reference_toast(app)?;
    }
    Ok(())
}

fn register_reference_shortcut(
    app: &AppHandle,
    shortcut: String,
) -> std::result::Result<(), String> {
    if !matches!(shortcut.as_str(), "Ctrl+Shift+T" | "Ctrl+Alt+T") {
        return Err("Unsupported reference shortcut.".to_string());
    }
    let state = app.state::<AppState>();
    let mut current = state
        .reference_shortcut
        .lock()
        .map_err(|_| "Reference shortcut lock failed.".to_string())?;
    if *current == shortcut {
        return Ok(());
    }
    if !current.is_empty() {
        let _ = app.global_shortcut().unregister(current.as_str());
    }
    let app_handle = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut.as_str(), move |_, _, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            let app_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let state = app_handle.state::<AppState>();
                let _ = save_reference_inner(&app_handle, &state).await;
            });
        })
        .map_err(|error| error.to_string())?;
    *current = shortcut;
    Ok(())
}

#[tauri::command]
fn accept_replacement(
    app: AppHandle,
    state: State<'_, AppState>,
    replacement: String,
) -> std::result::Result<(), String> {
    let session = state
        .pending
        .lock()
        .map_err(|_| "Pending session lock failed.".to_string())?
        .take()
        .ok_or_else(|| "No pending selection.".to_string())?;
    let replacement = replacement.trim();
    if replacement.is_empty() {
        return Err("Replacement text cannot be empty.".to_string());
    }
    if replacement.len() > 100_000 {
        return Err("Replacement text is too large.".to_string());
    }
    clipboard::replace_selection(session.target, replacement).map_err(|error| error.to_string())?;
    app.get_webview_window("main")
        .ok_or_else(|| "Preview window is unavailable.".to_string())?
        .hide()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn cancel_preview(state: State<'_, AppState>) -> std::result::Result<(), String> {
    state
        .pending
        .lock()
        .map_err(|_| "Pending session lock failed.".to_string())?
        .take();
    Ok(())
}

#[tauri::command]
fn clear_reference(state: State<'_, AppState>) -> std::result::Result<(), String> {
    if let Some(session) = state
        .pending
        .lock()
        .map_err(|_| "Pending session lock failed.".to_string())?
        .as_mut()
    {
        session.reference_text = None;
    }
    state
        .reference_text
        .lock()
        .map_err(|_| "Reference lock failed.".to_string())?
        .take();
    Ok(())
}

#[tauri::command]
async fn save_reference(
    app: AppHandle,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    save_reference_inner(&app, &state).await
}

#[tauri::command]
async fn analyze_session(
    input: SessionInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> std::result::Result<ClarificationPayload, String> {
    let session = state
        .pending
        .lock()
        .map_err(|_| "Pending session lock failed.".to_string())?
        .clone()
        .ok_or_else(|| "No pending selection.".to_string())?;
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?
        .clone();
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Preview window is unavailable.".to_string())?;
    let reference = input
        .use_reference
        .then_some(session.reference_text)
        .flatten();
    agent::analyze(
        settings,
        session.original,
        &input,
        reference.as_deref(),
        &window,
    )
    .await
}

#[tauri::command]
async fn generate_replacement(
    input: SessionInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> std::result::Result<String, String> {
    let session = state
        .pending
        .lock()
        .map_err(|_| "Pending session lock failed.".to_string())?
        .clone()
        .ok_or_else(|| "No pending selection.".to_string())?;
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?
        .clone();
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Preview window is unavailable.".to_string())?;
    let event = if input.candidate {
        "regeneration-chunk"
    } else {
        "generation-chunk"
    };
    let reference = input
        .use_reference
        .then_some(session.reference_text)
        .flatten();
    let result = agent::generate(
        settings,
        session.original,
        &input,
        reference.as_deref(),
        &window,
        event,
    )
    .await;
    if let Ok(mut pending) = state.pending.lock()
        && let Some(pending) = pending.as_mut()
    {
        pending.reference_text = None;
    }
    result
}

#[tauri::command]
fn hide_main_window(app: AppHandle) -> std::result::Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "Main window is unavailable.".to_string())?
        .hide()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> std::result::Result<SettingsPayload, String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?
        .clone();
    Ok(SettingsRepository::payload(settings))
}

#[tauri::command]
fn import_agent(
    source_path: String,
    state: State<'_, AppState>,
) -> std::result::Result<SettingsPayload, String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?
        .clone();
    CatalogRepository::import(&settings.agents_root, &source_path, "AGENT.md", "Agent")?;
    Ok(SettingsRepository::payload(settings))
}

#[tauri::command]
fn import_knowledge_base(
    source_path: String,
    state: State<'_, AppState>,
) -> std::result::Result<SettingsPayload, String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?
        .clone();
    CatalogRepository::import(
        &settings.knowledge_bases_root,
        &source_path,
        "INDEX.md",
        "knowledge base",
    )?;
    Ok(SettingsRepository::payload(settings))
}

#[tauri::command]
fn delete_agent(
    id: String,
    state: State<'_, AppState>,
) -> std::result::Result<SettingsPayload, String> {
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    CatalogRepository::delete(&settings.agents_root, &id, "AGENT.md", "Agent")?;
    if settings.default_agent == id {
        settings.default_agent.clear();
        SettingsRepository::save(&settings)?;
    }
    Ok(SettingsRepository::payload(settings.clone()))
}

#[tauri::command]
fn delete_knowledge_base(
    id: String,
    state: State<'_, AppState>,
) -> std::result::Result<SettingsPayload, String> {
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    CatalogRepository::delete(
        &settings.knowledge_bases_root,
        &id,
        "INDEX.md",
        "knowledge base",
    )?;
    if settings.default_knowledge_base == id {
        settings.default_knowledge_base.clear();
        SettingsRepository::save(&settings)?;
    }
    Ok(SettingsRepository::payload(settings.clone()))
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    input: SettingsInput,
    state: State<'_, AppState>,
) -> std::result::Result<SettingsPayload, String> {
    WindowsCredentialStore::save(&input.api_key)?;
    let reference_shortcut = input.reference_shortcut.trim().to_string();
    register_reference_shortcut(&app, reference_shortcut.clone())?;
    let settings = Settings {
        model: if input.model.trim().is_empty() {
            Settings::default().model
        } else {
            input.model.trim().to_owned()
        },
        agents_root: input.agents_root.trim().to_owned(),
        knowledge_bases_root: input.knowledge_bases_root.trim().to_owned(),
        default_agent: input.default_agent,
        default_knowledge_base: input.default_knowledge_base,
        allow_network: input.allow_network,
        reference_shortcut,
        reference_capture_mode: if input.reference_capture_mode == "clipboard" {
            "clipboard".to_string()
        } else {
            "selection".to_string()
        },
    };
    SettingsRepository::save(&settings)?;
    *state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())? = settings.clone();
    Ok(SettingsRepository::payload(settings))
}

#[tauri::command]
async fn test_deepseek_connection(
    state: State<'_, AppState>,
) -> std::result::Result<String, String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?
        .clone();
    let api_key = WindowsCredentialStore::load()
        .map_err(|_| "No DeepSeek API Key is saved. Save a Key first.".to_string())?;

    deepseek::test_connection(&settings.model, &api_key).await
}

fn show_settings(app: &AppHandle) -> std::result::Result<(), String> {
    let settings = app
        .state::<AppState>()
        .settings
        .lock()
        .map_err(|_| "Unable to access settings.".to_string())?
        .clone();
    let payload = SettingsRepository::payload(settings);
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Settings window is unavailable.".to_string())?;
    window
        .emit("settings-opened", payload)
        .map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let open_panel =
                MenuItem::with_id(app, "open-control", "打开控制面板", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&open_panel, &quit])?;
            TrayIconBuilder::new()
                .tooltip("Codex 输入增强器")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open-control" => {
                        let _ = show_settings(app);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let _ = show_settings(tray.app_handle());
                    }
                })
                .build(app)?;
            let handle = app.handle().clone();
            app.global_shortcut()
                .on_shortcut("Ctrl+Alt+E", move |_, _, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let app_handle = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let captured = match tauri::async_runtime::spawn_blocking(
                            clipboard::capture_selection_on_worker,
                        )
                        .await
                        {
                            Ok(Ok(captured)) => Ok(captured),
                            Ok(Err(error)) => Err(error.message().to_string()),
                            Err(error) => Err(format!("Selection worker failed: {error}")),
                        };
                        let window = match app_handle.get_webview_window("main") {
                            Some(window) => window,
                            None => return,
                        };
                        match captured {
                            Ok(Some(captured)) => {
                                let target = captured.target;
                                let original = captured.text;
                                let state = app_handle.state::<AppState>();
                                let reference_text = state
                                    .reference_text
                                    .lock()
                                    .expect("Reference lock failed")
                                    .take();
                                *state.pending.lock().expect("Pending session lock failed") =
                                    Some(PendingSession {
                                        target,
                                        original: original.clone(),
                                        reference_text: reference_text.clone(),
                                    });
                                let settings = match state.settings.lock() {
                                    Ok(settings) => settings.clone(),
                                    Err(_) => {
                                        let _ =
                                            window.emit("capture-error", "Settings lock failed.");
                                        let _ = window.show();
                                        let _ = window.set_focus();
                                        return;
                                    }
                                };
                                let payload = preview_payload(
                                    original,
                                    String::new(),
                                    &settings,
                                    reference_text,
                                );
                                let _ = show_preview(&window, payload);
                            }
                            Ok(None) => {}
                            Err(error) => {
                                let _ = window.emit("capture-error", error);
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    });
                })?;
            let reference_shortcut = app
                .state::<AppState>()
                .settings
                .lock()
                .map_err(|_| "Settings lock failed")?
                .reference_shortcut
                .clone();
            register_reference_shortcut(app.handle(), reference_shortcut)?;
            let settings_handle = app.handle().clone();
            app.global_shortcut()
                .on_shortcut("Ctrl+Shift+Alt+S", move |_, _, event| {
                    if event.state() == ShortcutState::Pressed {
                        let _ = show_settings(&settings_handle);
                    }
                })?;
            app.global_shortcut()
                .on_shortcut("Ctrl+Alt+Q", move |app, _, event| {
                    if event.state() == ShortcutState::Pressed {
                        app.exit(0);
                    }
                })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            accept_replacement,
            cancel_preview,
            clear_reference,
            save_reference,
            analyze_session,
            generate_replacement,
            hide_main_window,
            get_settings,
            import_agent,
            import_knowledge_base,
            delete_agent,
            delete_knowledge_base,
            save_settings,
            test_deepseek_connection
        ])
        .run(tauri::generate_context!())
        .expect("Tauri application error");
}
