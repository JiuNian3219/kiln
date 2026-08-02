use super::state::{AppState, PendingSession, cached_catalog};
use super::window::{preview_payload, save_reference_inner, show_preview, show_settings};
use crate::clipboard;
use crate::settings::{Settings, feature_enabled, shortcut_for};
use std::collections::HashMap;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

fn size_bucket(size: usize) -> &'static str {
    match size {
        0..=99 => "0-99",
        100..=999 => "100-999",
        1_000..=9_999 => "1k-9k",
        _ => "10k+",
    }
}

pub(crate) fn trigger_read_selection(app_handle: AppHandle) {
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
        let Some(window) = app_handle.get_webview_window("main") else {
            return;
        };
        match captured {
            Ok(Some(captured)) => {
                let state = app_handle.state::<AppState>();
                let original = captured.text;
                let reference_text = state
                    .reference_text
                    .lock()
                    .expect("Reference lock failed")
                    .take();
                *state.pending.lock().expect("Pending session lock failed") =
                    Some(PendingSession {
                        id: state.diagnostics.new_session_id(),
                        target: captured.target,
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
                let catalog = match cached_catalog(&state) {
                    Ok(catalog) => catalog,
                    Err(error) => {
                        let _ = window.emit("capture-error", error);
                        let _ = window.show();
                        let _ = window.set_focus();
                        return;
                    }
                };
                let _ = show_preview(
                    &window,
                    preview_payload(original, String::new(), &settings, &catalog, reference_text),
                );
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

pub(crate) fn reregister_shortcuts(app: &AppHandle, settings: &Settings) -> Result<(), String> {
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

pub(crate) fn unregister_global_shortcuts(app: &AppHandle) -> Result<(), String> {
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
pub(crate) fn suspend_global_shortcuts(app: AppHandle) -> Result<(), String> {
    unregister_global_shortcuts(&app)
}

#[tauri::command]
pub(crate) fn resume_global_shortcuts(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?
        .clone();
    reregister_shortcuts(&app, &settings)
}
