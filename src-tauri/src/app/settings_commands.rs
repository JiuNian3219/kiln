use super::state::{AppState, refresh_catalog};
use super::window::settings_payload;
use crate::settings::{
    FeatureAndShortcutSaveResult, FeatureAndShortcutSettings, SettingsRepository, feature_enabled,
    valid_knowledge_base_inline_token_limit, validate_shortcut,
};
use std::collections::HashMap;
use tauri::{AppHandle, State};

fn safe_client_error_type(error_type: Option<&str>) -> &'static str {
    match error_type {
        Some("RangeError") => "RangeError",
        Some("TypeError") => "TypeError",
        Some("Error") => "Error",
        _ => "Unknown",
    }
}

#[tauri::command]
pub(crate) fn get_settings(
    state: State<'_, AppState>,
) -> Result<crate::settings::SettingsPayload, String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?
        .clone();
    Ok(settings_payload(
        settings.clone(),
        refresh_catalog(&state, &settings)?,
    ))
}

#[tauri::command]
pub(crate) fn get_feature_and_shortcut_settings(
    state: State<'_, AppState>,
) -> Result<FeatureAndShortcutSettings, String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    Ok(FeatureAndShortcutSettings {
        feature_toggles: settings.feature_toggles.clone(),
        shortcuts: settings.shortcuts.clone(),
        reference_shortcut: settings.reference_shortcut.clone(),
        reference_capture_mode: settings.reference_capture_mode.clone(),
        knowledge_base_inline_token_limit: settings.knowledge_base_inline_token_limit,
    })
}

#[tauri::command]
pub(crate) fn save_feature_and_shortcut_settings(
    input: FeatureAndShortcutSettings,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<FeatureAndShortcutSaveResult, String> {
    let mut field_errors = HashMap::new();
    for key in ["read-selection", "open-control-panel", "quit-app"] {
        if let Err(error) = validate_shortcut(
            input
                .shortcuts
                .get(key)
                .map(String::as_str)
                .unwrap_or_default(),
        ) {
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
    if !valid_knowledge_base_inline_token_limit(input.knowledge_base_inline_token_limit) {
        field_errors.insert(
            "knowledgeBaseInlineTokenLimit".to_string(),
            "直接注入上限必须介于 500 到 8000 tokens。".to_string(),
        );
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
    settings.knowledge_base_inline_token_limit = input.knowledge_base_inline_token_limit;
    settings.allow_network = feature_enabled(&settings, "network-search");
    super::shortcuts::reregister_shortcuts(&app, &settings)?;
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
            knowledge_base_inline_token_limit: settings.knowledge_base_inline_token_limit,
        },
    })
}

#[tauri::command]
pub(crate) fn get_diagnostics(
    state: State<'_, AppState>,
) -> crate::diagnostics::DiagnosticsPayload {
    state.diagnostics.payload()
}

#[tauri::command]
pub(crate) fn clear_diagnostics(
    state: State<'_, AppState>,
) -> crate::diagnostics::DiagnosticsPayload {
    state.diagnostics.clear();
    state.diagnostics.payload()
}

#[tauri::command]
pub(crate) fn report_client_diagnostic(
    kind: String,
    error_type: Option<String>,
    state: State<'_, AppState>,
) {
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
        "preview_cancel_clicked" => {
            state
                .diagnostics
                .info("preview.cancel_clicked", None, serde_json::json!({}))
        }
        "preview_continue_clicked" => {
            state
                .diagnostics
                .info("preview.continue_clicked", None, serde_json::json!({}))
        }
        "preview_cancel_failed" => state.diagnostics.error(
            "preview.cancel_client_failed",
            None,
            "E-PREVIEW-CANCEL-CLIENT-001",
            serde_json::json!({ "errorType": safe_client_error_type(error_type.as_deref()) }),
        ),
        _ => {}
    }
}
