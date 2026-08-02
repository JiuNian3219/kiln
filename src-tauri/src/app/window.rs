use super::state::{AppState, CatalogSnapshot, refresh_catalog};
use crate::clipboard;
use crate::settings::{
    CatalogEntry, Combination, Settings, SettingsPayload, SettingsRepository,
    combination_agent_available, feature_enabled, is_general_enhancement_agent,
};
use serde::Serialize;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub(crate) const PREVIEW_WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(560.0, 430.0);
pub(crate) const CONTROL_PANEL_WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(760.0, 620.0);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewPayload {
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

pub(crate) fn settings_payload(settings: Settings, catalog: CatalogSnapshot) -> SettingsPayload {
    SettingsRepository::payload_from_catalog(settings, catalog.agents, catalog.knowledge_bases)
}

#[allow(clippy::obfuscated_if_else)]
pub(crate) fn preview_payload(
    original: String,
    replacement: String,
    settings: &Settings,
    catalog: &CatalogSnapshot,
    reference_text: Option<String>,
) -> PreviewPayload {
    let combinations = settings
        .combinations
        .iter()
        .filter(|combination| {
            combination_agent_available(&combination.agent_id, &catalog.agents)
                && !combination.knowledge_base_ids.is_empty()
                && combination.knowledge_base_ids.iter().all(|id| {
                    catalog.knowledge_bases.iter().any(|base| {
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
        agents: catalog.agents.clone(),
        knowledge_bases: catalog.knowledge_bases.clone(),
        combinations,
        selected_combination_id,
        use_agent: !selected_agent_id.is_empty()
            && !is_general_enhancement_agent(&selected_agent_id),
        use_knowledge_base: !selected_knowledge_base_ids.is_empty(),
        use_network: settings.allow_network,
        network_available: settings.allow_network,
        selected_agent_id,
        selected_knowledge_base_ids,
        reference_active: reference_text.is_some(),
        reference_text,
    }
}

pub(crate) fn show_preview(window: &WebviewWindow, payload: PreviewPayload) -> Result<(), String> {
    window
        .set_size(PREVIEW_WINDOW_SIZE)
        .map_err(|error| error.to_string())?;
    window.center().map_err(|error| error.to_string())?;
    window
        .emit("selection-captured", payload)
        .map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

async fn capture_reference_text(mode: &str) -> Result<(Option<String>, bool), String> {
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

fn show_reference_toast(app: &AppHandle, state: &AppState) -> Result<(), String> {
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
        std::thread::sleep(std::time::Duration::from_millis(450));
        if toast_generation.load(Ordering::SeqCst) != generation {
            let _ = toast.close();
            return;
        }
        let _ = toast.show();
        std::thread::sleep(std::time::Duration::from_millis(2700));
        if toast_generation.load(Ordering::SeqCst) == generation {
            let _ = toast.close();
        }
    });
    Ok(())
}

pub(crate) async fn save_reference_inner(app: &AppHandle, state: &AppState) -> Result<(), String> {
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

pub(crate) fn show_settings(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Unable to access settings.".to_string())?
        .clone();
    let payload = settings_payload(settings.clone(), refresh_catalog(&state, &settings)?);
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Settings window is unavailable.".to_string())?;
    window
        .set_size(CONTROL_PANEL_WINDOW_SIZE)
        .map_err(|error| error.to_string())?;
    window.center().map_err(|error| error.to_string())?;
    window
        .emit("settings-opened", payload)
        .map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn set_main_window_layout(layout: String, app: AppHandle) -> Result<(), String> {
    let size = match layout.as_str() {
        "preview" => PREVIEW_WINDOW_SIZE,
        "control" => CONTROL_PANEL_WINDOW_SIZE,
        _ => return Err("Unsupported window layout.".to_string()),
    };
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window is unavailable.".to_string())?;
    window.set_size(size).map_err(|error| error.to_string())?;
    window.center().map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn hide_main_window(app: AppHandle) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "Main window is unavailable.".to_string())?
        .hide()
        .map_err(|error| error.to_string())
}
