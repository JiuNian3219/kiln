#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
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
mod diagnostics;
mod provider;
mod session;
mod settings;
mod workspace;

use credential::WindowsCredentialStore;
use diagnostics::{Diagnostics, DiagnosticsPayload};
use session::{ClarificationPayload, SessionInput};
use settings::{
    CatalogEntry, CatalogRepository, Combination, CombinationInput, FeatureAndShortcutSaveResult,
    FeatureAndShortcutSettings, ModelProvider, ModelProviderInput, Settings, SettingsPayload,
    SettingsRepository, active_model_provider, discover_catalog, discover_knowledge_bases,
    feature_enabled, knowledge_base_index_candidates, knowledge_base_index_material,
    save_generated_knowledge_base_index, shortcut_for, validate_shortcut,
};

struct AppState {
    diagnostics: Diagnostics,
    pending: Mutex<Option<PendingSession>>,
    reference_text: Mutex<Option<String>>,
    registered_shortcuts: Mutex<Vec<String>>,
    settings: Mutex<Settings>,
    toast_generation: Arc<AtomicU64>,
}

#[derive(Clone)]
struct PendingSession {
    id: String,
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
    combinations: Vec<Combination>,
    selected_combination_id: String,
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
            diagnostics: Diagnostics::new(),
            pending: Mutex::new(None),
            reference_text: Mutex::new(None),
            registered_shortcuts: Mutex::new(Vec::new()),
            settings: Mutex::new(settings),
            toast_generation: Arc::new(AtomicU64::new(0)),
        }
    }
}

#[allow(clippy::obfuscated_if_else)]
fn preview_payload(
    original: String,
    replacement: String,
    settings: &Settings,
    reference_text: Option<String>,
) -> PreviewPayload {
    let agents = discover_catalog(&settings.agents_root, "AGENT.md");
    let knowledge_bases = discover_knowledge_bases(settings);
    let combinations = settings
        .combinations
        .iter()
        .filter(|combination| {
            agents.iter().any(|agent| agent.id == combination.agent_id)
                && !combination.knowledge_base_ids.is_empty()
                && combination.knowledge_base_ids.iter().all(|id| {
                    knowledge_bases.iter().any(|base| {
                        base.id == *id && base.index_status.as_deref() != Some("缺少 INDEX")
                    })
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    let selected_combination_id = combinations
        .iter()
        .any(|item| item.id == settings.default_combination)
        .then_some(settings.default_combination.clone())
        .unwrap_or_default();
    let selected = combinations
        .iter()
        .find(|item| item.id == selected_combination_id);
    let selected_agent_id = selected
        .map(|item| item.agent_id.clone())
        .unwrap_or_default();
    let selected_knowledge_base_ids = selected
        .map(|item| item.knowledge_base_ids.clone())
        .unwrap_or_default();
    PreviewPayload {
        original,
        replacement,
        use_agent: !selected_agent_id.is_empty(),
        use_knowledge_base: !selected_knowledge_base_ids.is_empty(),
        use_network: settings.allow_network,
        network_available: settings.allow_network,
        agents,
        knowledge_bases,
        selected_agent_id,
        selected_knowledge_base_ids,
        combinations,
        selected_combination_id,
        reference_active: reference_text.is_some(),
        reference_text,
    }
}

fn size_bucket(size: usize) -> &'static str {
    match size {
        0..=99 => "0-99",
        100..=999 => "100-999",
        1_000..=9_999 => "1k-9k",
        _ => "10k+",
    }
}

fn safe_client_error_type(error_type: Option<&str>) -> &'static str {
    match error_type {
        Some("RangeError") => "RangeError",
        Some("TypeError") => "TypeError",
        Some("Error") => "Error",
        _ => "Unknown",
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

fn show_reference_toast(app: &AppHandle, state: &AppState) -> std::result::Result<(), String> {
    let generation = state.toast_generation.fetch_add(1, Ordering::SeqCst) + 1;
    if let Some(existing) = app.get_webview_window("reference-toast") {
        let _ = existing.close();
    }
    let toast =
        WebviewWindowBuilder::new(app, "reference-toast", WebviewUrl::App("toast.html".into()))
            .title("参考文本已保存")
            .inner_size(300.0, 50.0)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .focusable(false)
            .visible(false)
            .build()
            .map_err(|error| error.to_string())?;
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let position = monitor.position();
        let size = monitor.size();
        let toast_size = toast
            .outer_size()
            .unwrap_or_else(|_| toast.inner_size().unwrap_or_default());
        let _ = toast.set_position(PhysicalPosition::new(
            position.x + (size.width as i32 - toast_size.width as i32) / 2,
            position.y + size.height as i32 / 4,
        ));
    }
    let toast_generation = Arc::clone(&state.toast_generation);
    std::thread::spawn(move || {
        // Build the WebView while hidden so its first paint cannot flash as an empty frame.
        // The static page has no network work; this short warm-up is enough for its CSS to load.
        std::thread::sleep(std::time::Duration::from_millis(450));
        if toast_generation.load(Ordering::SeqCst) != generation {
            let _ = toast.close();
            return;
        }
        let _ = toast.show();
        // Keep the host only for the CSS animation (2.55 s), then close it immediately.
        std::thread::sleep(std::time::Duration::from_millis(2700));
        if toast_generation.load(Ordering::SeqCst) == generation {
            let _ = toast.close();
        }
    });
    Ok(())
}

async fn save_reference_inner(
    app: &AppHandle,
    state: &AppState,
) -> std::result::Result<(), String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?
        .clone();
    if !feature_enabled(&settings, "reference-context") {
        return Ok(());
    }
    let (text, show_toast) = capture_reference_text(&settings.reference_capture_mode).await?;
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
        show_reference_toast(app, state)?;
    }
    Ok(())
}

fn trigger_read_selection(app_handle: AppHandle) {
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
                        id: state.diagnostics.new_session_id(),
                        target,
                        original: original.clone(),
                        reference_text: reference_text.clone(),
                    });
                let session_id = state
                    .pending
                    .lock()
                    .expect("Pending session lock failed")
                    .as_ref()
                    .map(|session| session.id.clone());
                state.diagnostics.info(
                    "selection.captured",
                    session_id.as_deref(),
                    serde_json::json!({ "selectionSizeBucket": size_bucket(original.len()) }),
                );
                let settings = match state.settings.lock() {
                    Ok(settings) => settings.clone(),
                    Err(_) => {
                        let _ = window.emit("capture-error", "Settings lock failed.");
                        let _ = window.show();
                        let _ = window.set_focus();
                        return;
                    }
                };
                let payload = preview_payload(original, String::new(), &settings, reference_text);
                let _ = show_preview(&window, payload);
            }
            Ok(None) => {}
            Err(error) => {
                app_handle.state::<AppState>().diagnostics.error(
                    "selection.capture_failed",
                    None,
                    "E-SELECTION-CAPTURE-001",
                    serde_json::json!({ "errorType": "clipboard" }),
                );
                let _ = window.emit("capture-error", error);
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
    });
}

fn reregister_shortcuts(app: &AppHandle, settings: &Settings) -> std::result::Result<(), String> {
    let mut actions = vec![
        (shortcut_for(settings, "read-selection"), "read-selection"),
        (
            shortcut_for(settings, "open-control-panel"),
            "open-control-panel",
        ),
        (shortcut_for(settings, "quit-app"), "quit-app"),
    ];
    if feature_enabled(settings, "reference-context") {
        actions.push((settings.reference_shortcut.clone(), "reference-context"));
    }
    unregister_global_shortcuts(app)?;
    let mut resolved = HashMap::new();
    for (shortcut, action) in actions {
        resolved.insert(shortcut, action);
    }
    for (shortcut, action) in &resolved {
        let shortcut_key = shortcut.clone();
        let action = *action;
        app.global_shortcut()
            .on_shortcut(shortcut.as_str(), move |app, _, event| {
                if event.state() != ShortcutState::Pressed {
                    return;
                }
                match action {
                    "read-selection" => trigger_read_selection(app.clone()),
                    "open-control-panel" => {
                        let _ = show_settings(app);
                    }
                    "quit-app" => app.exit(0),
                    "reference-context" => {
                        let app_handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = app_handle.state::<AppState>();
                            let _ = save_reference_inner(&app_handle, &state).await;
                        });
                    }
                    _ => {}
                }
            })
            .map_err(|error| format!("Unable to register {shortcut_key}: {error}"))?;
        app.state::<AppState>()
            .registered_shortcuts
            .lock()
            .map_err(|_| "Shortcut lock failed.".to_string())?
            .push(shortcut.clone());
    }
    Ok(())
}

fn unregister_global_shortcuts(app: &AppHandle) -> std::result::Result<(), String> {
    let state = app.state::<AppState>();
    let mut registered = state
        .registered_shortcuts
        .lock()
        .map_err(|_| "Shortcut lock failed.".to_string())?;
    for shortcut in registered.drain(..) {
        let _ = app.global_shortcut().unregister(shortcut.as_str());
    }
    Ok(())
}

#[tauri::command]
fn suspend_global_shortcuts(app: AppHandle) -> std::result::Result<(), String> {
    unregister_global_shortcuts(&app)
}

#[tauri::command]
fn resume_global_shortcuts(
    app: AppHandle,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?
        .clone();
    reregister_shortcuts(&app, &settings)
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
    state.diagnostics.info(
        "selection.replace_requested",
        Some(&session.id),
        serde_json::json!({ "replacementSizeBucket": size_bucket(replacement.len()) }),
    );
    let replacement = replacement.trim();
    if replacement.is_empty() {
        state.diagnostics.error(
            "selection.replace_rejected",
            Some(&session.id),
            "E-REPLACE-EMPTY-001",
            serde_json::json!({}),
        );
        return Err("Replacement text cannot be empty.".to_string());
    }
    if replacement.len() > 100_000 {
        state.diagnostics.error(
            "selection.replace_rejected",
            Some(&session.id),
            "E-REPLACE-LIMIT-001",
            serde_json::json!({}),
        );
        return Err("Replacement text is too large.".to_string());
    }
    if let Err(error) = clipboard::replace_selection(session.target, replacement) {
        state.diagnostics.error(
            "selection.replace_failed",
            Some(&session.id),
            "E-REPLACE-FOCUS-001",
            serde_json::json!({ "errorType": "clipboard" }),
        );
        return Err(error.to_string());
    }
    state.diagnostics.info(
        "selection.replace_completed",
        Some(&session.id),
        serde_json::json!({}),
    );
    app.get_webview_window("main")
        .ok_or_else(|| "Preview window is unavailable.".to_string())?
        .hide()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn cancel_preview(state: State<'_, AppState>) -> std::result::Result<(), String> {
    if let Some(session) = state
        .pending
        .lock()
        .map_err(|_| "Pending session lock failed.".to_string())?
        .take()
    {
        state.diagnostics.info(
            "session.cancelled",
            Some(&session.id),
            serde_json::json!({}),
        );
    }
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
    let started = Instant::now();
    state.diagnostics.info(
        "agent.analysis_started",
        Some(&session.id),
        serde_json::json!({
            "usesAgent": input.use_agent,
            "knowledgeBaseCount": input.knowledge_base_ids.len(),
            "usesReference": input.use_reference,
        }),
    );
    let result = agent::analyze(
        settings,
        session.original,
        &input,
        reference.as_deref(),
        &window,
        &state.diagnostics,
        &session.id,
    )
    .await;
    match &result {
        Ok(payload) => state.diagnostics.info(
            "agent.analysis_completed",
            Some(&session.id),
            serde_json::json!({
                "questionCount": payload.questions.len(),
                "durationMs": started.elapsed().as_millis(),
            }),
        ),
        Err(_) => state.diagnostics.error(
            "agent.analysis_failed",
            Some(&session.id),
            "E-ANALYZE-001",
            serde_json::json!({ "durationMs": started.elapsed().as_millis() }),
        ),
    }
    result
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
    let reference = input
        .use_reference
        .then_some(session.reference_text)
        .flatten();
    let started = Instant::now();
    state.diagnostics.info(
        "agent.generation_started",
        Some(&session.id),
        serde_json::json!({
            "candidate": input.candidate,
            "answerCount": input.answers.len(),
            "knowledgeBaseCount": input.knowledge_base_ids.len(),
        }),
    );
    let result = agent::generate(
        settings,
        session.original,
        &input,
        reference.as_deref(),
        &window,
        &state.diagnostics,
        &session.id,
    )
    .await;
    match &result {
        Ok(replacement) => state.diagnostics.info(
            "agent.generation_completed",
            Some(&session.id),
            serde_json::json!({
                "replacementSizeBucket": size_bucket(replacement.len()),
                "durationMs": started.elapsed().as_millis(),
            }),
        ),
        Err(_) => state.diagnostics.error(
            "agent.generation_failed",
            Some(&session.id),
            "E-GENERATE-001",
            serde_json::json!({ "durationMs": started.elapsed().as_millis() }),
        ),
    }
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
fn get_feature_and_shortcut_settings(
    state: State<'_, AppState>,
) -> std::result::Result<FeatureAndShortcutSettings, String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    Ok(FeatureAndShortcutSettings {
        feature_toggles: settings.feature_toggles.clone(),
        shortcuts: settings.shortcuts.clone(),
        reference_shortcut: settings.reference_shortcut.clone(),
        reference_capture_mode: settings.reference_capture_mode.clone(),
    })
}

#[tauri::command]
fn save_feature_and_shortcut_settings(
    input: FeatureAndShortcutSettings,
    app: AppHandle,
    state: State<'_, AppState>,
) -> std::result::Result<FeatureAndShortcutSaveResult, String> {
    let mut field_errors = HashMap::new();
    for key in ["read-selection", "open-control-panel", "quit-app"] {
        let value = input
            .shortcuts
            .get(key)
            .map(String::as_str)
            .unwrap_or_default();
        if let Err(error) = validate_shortcut(value) {
            field_errors.insert(key.to_string(), error);
        }
    }
    let reference_enabled = input
        .feature_toggles
        .get("reference-context")
        .copied()
        .unwrap_or(true);
    if reference_enabled && let Err(error) = validate_shortcut(&input.reference_shortcut) {
        field_errors.insert("referenceShortcut".to_string(), error);
    }
    if !field_errors.is_empty() {
        return Ok(FeatureAndShortcutSaveResult {
            success: false,
            field_errors,
            settings: input,
        });
    }
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    settings.feature_toggles = input.feature_toggles.clone();
    settings.shortcuts = input.shortcuts.clone();
    settings.reference_shortcut = input.reference_shortcut.trim().to_string();
    settings.reference_capture_mode = if input.reference_capture_mode == "clipboard" {
        "clipboard".to_string()
    } else {
        "selection".to_string()
    };
    settings.allow_network = feature_enabled(&settings, "network-search");
    reregister_shortcuts(&app, &settings)?;
    SettingsRepository::save(&settings)?;
    state
        .diagnostics
        .info("features_and_shortcuts.saved", None, serde_json::json!({}));
    Ok(FeatureAndShortcutSaveResult {
        success: true,
        field_errors,
        settings: FeatureAndShortcutSettings {
            feature_toggles: settings.feature_toggles.clone(),
            shortcuts: settings.shortcuts.clone(),
            reference_shortcut: settings.reference_shortcut.clone(),
            reference_capture_mode: settings.reference_capture_mode.clone(),
        },
    })
}

#[tauri::command]
fn get_diagnostics(state: State<'_, AppState>) -> DiagnosticsPayload {
    state.diagnostics.payload()
}

#[tauri::command]
fn clear_diagnostics(state: State<'_, AppState>) -> DiagnosticsPayload {
    state.diagnostics.clear();
    state.diagnostics.payload()
}

/// Receives only a small, fixed client-side event vocabulary. Never accept an
/// arbitrary JavaScript error message because it might include user text.
#[tauri::command]
fn report_client_diagnostic(kind: String, error_type: Option<String>, state: State<'_, AppState>) {
    match kind.as_str() {
        "directory_picker_requested" => {
            state
                .diagnostics
                .info("directory_picker.requested", None, serde_json::json!({}))
        }
        "directory_picker_cancelled" => {
            state
                .diagnostics
                .info("directory_picker.cancelled", None, serde_json::json!({}))
        }
        "directory_picker_failed" => state.diagnostics.error(
            "directory_picker.failed",
            None,
            "E-DIALOG-OPEN-001",
            serde_json::json!({ "errorType": safe_client_error_type(error_type.as_deref()) }),
        ),
        _ => {}
    }
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
    CatalogRepository::import_agent_file(&settings.agents_root, &source_path)?;
    Ok(SettingsRepository::payload(settings))
}

#[tauri::command]
fn import_knowledge_bases(
    source_paths: Vec<String>,
    state: State<'_, AppState>,
) -> std::result::Result<SettingsPayload, String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?
        .clone();
    CatalogRepository::import_knowledge_bases(&settings.knowledge_bases_root, &source_paths)?;
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
    CatalogRepository::delete_directory(&settings.agents_root, &id, "Agent")?;
    settings.combinations.retain(|item| item.agent_id != id);
    if !settings
        .combinations
        .iter()
        .any(|item| item.id == settings.default_combination)
    {
        settings.default_combination.clear();
    }
    SettingsRepository::save(&settings)?;
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
    CatalogRepository::delete_directory(&settings.knowledge_bases_root, &id, "知识库")?;
    settings.knowledge_base_indexes.remove(&id);
    settings
        .combinations
        .retain(|item| !item.knowledge_base_ids.iter().any(|base| base == &id));
    if !settings
        .combinations
        .iter()
        .any(|item| item.id == settings.default_combination)
    {
        settings.default_combination.clear();
    }
    SettingsRepository::save(&settings)?;
    Ok(SettingsRepository::payload(settings.clone()))
}

#[tauri::command]
fn save_combination(
    input: CombinationInput,
    state: State<'_, AppState>,
) -> std::result::Result<SettingsPayload, String> {
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    let name = input.name.trim();
    if name.is_empty() || input.agent_id.trim().is_empty() || input.knowledge_base_ids.is_empty() {
        return Err("组合必须包含名称、一个 Agent 和至少一个知识库。".to_string());
    }
    let agents = discover_catalog(&settings.agents_root, "AGENT.md");
    let knowledge_bases = discover_knowledge_bases(&settings);
    if !agents.iter().any(|item| item.id == input.agent_id)
        || input.knowledge_base_ids.iter().any(|id| {
            !knowledge_bases
                .iter()
                .any(|item| item.id == *id && item.index_status.as_deref() != Some("缺少 INDEX"))
        })
    {
        return Err("组合引用了不可用或缺少 INDEX 的资料。".to_string());
    }
    let id = if input.id.trim().is_empty() {
        format!(
            "combo-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        )
    } else {
        input.id
    };
    let combination = Combination {
        id: id.clone(),
        name: name.to_string(),
        agent_id: input.agent_id,
        knowledge_base_ids: input.knowledge_base_ids,
    };
    if let Some(existing) = settings.combinations.iter_mut().find(|item| item.id == id) {
        *existing = combination;
    } else {
        settings.combinations.push(combination);
    }
    SettingsRepository::save(&settings)?;
    Ok(SettingsRepository::payload(settings.clone()))
}

#[tauri::command]
fn delete_combination(
    id: String,
    state: State<'_, AppState>,
) -> std::result::Result<SettingsPayload, String> {
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    settings.combinations.retain(|item| item.id != id);
    if settings.default_combination == id {
        settings.default_combination.clear();
    }
    SettingsRepository::save(&settings)?;
    Ok(SettingsRepository::payload(settings.clone()))
}

#[tauri::command]
fn set_default_combination(
    id: String,
    state: State<'_, AppState>,
) -> std::result::Result<SettingsPayload, String> {
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    if !id.is_empty() && !settings.combinations.iter().any(|item| item.id == id) {
        return Err("组合不存在。".to_string());
    }
    settings.default_combination = id;
    SettingsRepository::save(&settings)?;
    Ok(SettingsRepository::payload(settings.clone()))
}

#[tauri::command]
fn get_knowledge_base_index_candidates(
    id: String,
    state: State<'_, AppState>,
) -> std::result::Result<Vec<String>, String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    knowledge_base_index_candidates(&settings, &id)
}

#[tauri::command]
fn set_knowledge_base_index(
    id: String,
    mode: String,
    manual_path: Option<String>,
    state: State<'_, AppState>,
) -> std::result::Result<SettingsPayload, String> {
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    if mode == "manual" {
        let path = manual_path.unwrap_or_default();
        if !knowledge_base_index_candidates(&settings, &id)?
            .iter()
            .any(|item| item == &path)
        {
            return Err("请选择知识库内的 Markdown 或文本文件作为索引。".to_string());
        }
        settings.knowledge_base_indexes.insert(
            id,
            settings::KnowledgeBaseIndex {
                mode,
                manual_path: path,
            },
        );
    } else {
        settings.knowledge_base_indexes.remove(&id);
    }
    SettingsRepository::save(&settings)?;
    Ok(SettingsRepository::payload(settings.clone()))
}

#[tauri::command]
async fn generate_knowledge_base_index(
    id: String,
    state: State<'_, AppState>,
) -> std::result::Result<SettingsPayload, String> {
    let (material, provider) = {
        let settings = state
            .settings
            .lock()
            .map_err(|_| "Settings lock failed.".to_string())?;
        (
            knowledge_base_index_material(&settings, &id)?,
            active_model_provider(&settings)?,
        )
    };
    let api_key = WindowsCredentialStore::load_for(&provider.id)?;
    let client = provider::client(std::time::Duration::from_secs(60))?;
    let content = provider::text(
        &client,
        &provider,
        &api_key,
        &[
            serde_json::json!({"role":"system","content":"根据给出的资料文件清单和有限摘录，生成简洁的中文知识库索引。说明主题、适用范围，并列出每个文件的用途。不要编造文件中没有的信息。仅输出 Markdown。"}),
            serde_json::json!({"role":"user","content":material}),
        ],
        1200,
        false,
    )
    .await?;
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    save_generated_knowledge_base_index(&mut settings, id, content)?;
    SettingsRepository::save(&settings)?;
    Ok(SettingsRepository::payload(settings.clone()))
}

async fn test_provider_connection(provider: &ModelProvider) -> std::result::Result<String, String> {
    let api_key = WindowsCredentialStore::load_for(&provider.id)?;
    let client = provider::client(std::time::Duration::from_secs(20))?;
    let content = provider::text(
        &client,
        provider,
        &api_key,
        &[
            serde_json::json!({"role":"system","content":"Reply with exactly OK."}),
            serde_json::json!({"role":"user","content":"OK"}),
        ],
        16,
        false,
    )
    .await?;
    Ok(content)
}

fn provider_id_from_name(name: &str, existing: &[ModelProvider]) -> String {
    let cleaned = name
        .chars()
        .filter_map(|character| {
            character
                .is_ascii_alphanumeric()
                .then(|| character.to_ascii_lowercase())
                .or_else(|| matches!(character, ' ' | '-' | '_').then_some('-'))
        })
        .collect::<String>();
    let base = cleaned.trim_matches('-');
    let base = if base.is_empty() { "provider" } else { base };
    let mut index = 1_u32;
    loop {
        let candidate = if index == 1 {
            base.to_string()
        } else {
            format!("{base}-{index}")
        };
        if !existing.iter().any(|provider| provider.id == candidate) {
            return candidate;
        }
        index += 1;
    }
}

#[tauri::command]
fn save_model_provider(
    input: ModelProviderInput,
    state: State<'_, AppState>,
) -> std::result::Result<SettingsPayload, String> {
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    let id = if input.id.trim().is_empty() {
        provider_id_from_name(input.name.trim(), &settings.model_providers)
    } else {
        input.id.trim().to_string()
    };
    let provider = ModelProvider {
        id: id.clone(),
        name: input.name.trim().to_string(),
        protocol: input.protocol.trim().to_string(),
        base_url: input.base_url.trim().trim_end_matches('/').to_string(),
        model: input.model.trim().to_string(),
    };
    provider::validate_provider(&provider)?;
    if let Some(existing) = settings
        .model_providers
        .iter_mut()
        .find(|existing| existing.id == id)
    {
        *existing = provider;
    } else {
        settings.model_providers.push(provider);
    }
    if settings.default_model_provider.is_empty() {
        settings.default_model_provider = id.clone();
    }
    WindowsCredentialStore::save_for(&id, &input.api_key)?;
    SettingsRepository::save(&settings)?;
    Ok(SettingsRepository::payload(settings.clone()))
}

#[tauri::command]
fn delete_model_provider(
    id: String,
    state: State<'_, AppState>,
) -> std::result::Result<SettingsPayload, String> {
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    if !settings
        .model_providers
        .iter()
        .any(|provider| provider.id == id)
    {
        return Err("AI 服务不存在。".to_string());
    }
    if settings.model_providers.len() == 1 {
        return Err("请至少保留一个 AI 服务。".to_string());
    }
    settings
        .model_providers
        .retain(|provider| provider.id != id);
    if settings.default_model_provider == id {
        settings.default_model_provider = settings
            .model_providers
            .first()
            .map(|provider| provider.id.clone())
            .unwrap_or_default();
    }
    WindowsCredentialStore::delete_for(&id)?;
    SettingsRepository::save(&settings)?;
    Ok(SettingsRepository::payload(settings.clone()))
}

#[tauri::command]
fn set_default_model_provider(
    id: String,
    state: State<'_, AppState>,
) -> std::result::Result<SettingsPayload, String> {
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    if !settings
        .model_providers
        .iter()
        .any(|provider| provider.id == id)
    {
        return Err("AI 服务不存在。".to_string());
    }
    settings.default_model_provider = id;
    SettingsRepository::save(&settings)?;
    Ok(SettingsRepository::payload(settings.clone()))
}

#[tauri::command]
async fn test_model_provider(
    id: String,
    state: State<'_, AppState>,
) -> std::result::Result<String, String> {
    let provider = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?
        .model_providers
        .iter()
        .find(|provider| provider.id == id)
        .cloned()
        .ok_or_else(|| "AI 服务不存在。".to_string())?;
    test_provider_connection(&provider).await
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
            let loaded_settings = app
                .state::<AppState>()
                .settings
                .lock()
                .map_err(|_| "Settings lock failed")?
                .clone();
            reregister_shortcuts(app.handle(), &loaded_settings)?;
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
            get_diagnostics,
            clear_diagnostics,
            report_client_diagnostic,
            get_settings,
            get_feature_and_shortcut_settings,
            save_feature_and_shortcut_settings,
            suspend_global_shortcuts,
            resume_global_shortcuts,
            import_agent,
            import_knowledge_bases,
            delete_agent,
            delete_knowledge_base,
            save_combination,
            delete_combination,
            set_default_combination,
            get_knowledge_base_index_candidates,
            set_knowledge_base_index,
            generate_knowledge_base_index,
            save_model_provider,
            delete_model_provider,
            set_default_model_provider,
            test_model_provider
        ])
        .run(tauri::generate_context!())
        .expect("Tauri application error");
}
