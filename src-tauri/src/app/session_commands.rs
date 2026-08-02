use super::state::AppState;
use super::window::save_reference_inner;
use crate::session::{ClarificationPayload, SessionInput};
use crate::{agent, clipboard};
use std::time::Instant;
use tauri::{AppHandle, Manager, State};

fn size_bucket(size: usize) -> &'static str {
    match size {
        0..=99 => "0-99",
        100..=999 => "100-999",
        1_000..=9_999 => "1k-9k",
        _ => "10k+",
    }
}

#[tauri::command]
pub(crate) fn accept_replacement(
    app: AppHandle,
    state: State<'_, AppState>,
    replacement: String,
) -> Result<(), String> {
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
pub(crate) fn cancel_preview(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let session_id = state
        .pending
        .lock()
        .map_err(|_| "Pending session lock failed.".to_string())?
        .take()
        .map(|session| session.id);
    state.diagnostics.info(
        "preview.cancel_requested",
        session_id.as_deref(),
        serde_json::json!({}),
    );
    if let Some(session_id) = session_id.as_deref() {
        state
            .diagnostics
            .info("session.cancelled", Some(session_id), serde_json::json!({}));
    }
    let result = app
        .get_webview_window("main")
        .ok_or_else(|| "Preview window is unavailable.".to_string())?
        .hide()
        .map_err(|error| error.to_string());
    match &result {
        Ok(()) => state.diagnostics.info(
            "preview.cancel_hidden",
            session_id.as_deref(),
            serde_json::json!({}),
        ),
        Err(_) => state.diagnostics.error(
            "preview.cancel_hide_failed",
            session_id.as_deref(),
            "E-PREVIEW-HIDE-001",
            serde_json::json!({ "errorType": "window" }),
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn clear_reference(state: State<'_, AppState>) -> Result<(), String> {
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
pub(crate) async fn save_reference(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    save_reference_inner(&app, &state).await
}

#[tauri::command]
pub(crate) async fn analyze_session(
    input: SessionInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ClarificationPayload, String> {
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
        .then_some(session.reference_text.clone())
        .flatten();
    let started = Instant::now();
    state.diagnostics.info("agent.analysis_started", Some(&session.id), serde_json::json!({ "usesAgent": input.use_agent, "knowledgeBaseCount": input.knowledge_base_ids.len(), "usesReference": input.use_reference }));
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
        Ok(payload) => state.diagnostics.info("agent.analysis_completed", Some(&session.id), serde_json::json!({ "questionCount": payload.questions.len(), "directReplacement": payload.replacement.is_some(), "durationMs": started.elapsed().as_millis() })),
        Err(_) => state.diagnostics.error("agent.analysis_failed", Some(&session.id), "E-ANALYZE-001", serde_json::json!({ "durationMs": started.elapsed().as_millis() })),
    }
    if result
        .as_ref()
        .ok()
        .and_then(|payload| payload.replacement.as_ref())
        .is_some()
        && let Ok(mut pending) = state.pending.lock()
        && let Some(pending) = pending.as_mut()
    {
        pending.reference_text = None;
    }
    result
}

#[tauri::command]
pub(crate) async fn generate_replacement(
    input: SessionInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
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
        .then_some(session.reference_text.clone())
        .flatten();
    let started = Instant::now();
    state.diagnostics.info("agent.generation_started", Some(&session.id), serde_json::json!({ "candidate": input.candidate, "answerCount": input.answers.len(), "knowledgeBaseCount": input.knowledge_base_ids.len() }));
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
        Ok(replacement) => state.diagnostics.info("agent.generation_completed", Some(&session.id), serde_json::json!({ "replacementSizeBucket": size_bucket(replacement.len()), "durationMs": started.elapsed().as_millis() })),
        Err(_) => state.diagnostics.error("agent.generation_failed", Some(&session.id), "E-GENERATE-001", serde_json::json!({ "durationMs": started.elapsed().as_millis() })),
    }
    if let Ok(mut pending) = state.pending.lock()
        && let Some(pending) = pending.as_mut()
    {
        pending.reference_text = None;
    }
    result
}
