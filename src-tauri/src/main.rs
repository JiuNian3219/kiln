#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent;
mod agent_protocol;
mod app;
mod clipboard;
mod credential;
mod diagnostics;
mod provider;
mod session;
mod settings;
mod workspace;

use app::catalog_commands::*;
use app::knowledge_base_commands::*;
use app::provider_commands::*;
use app::session_commands::*;
use app::settings_commands::*;
use app::shortcuts::*;
use app::state::AppState;
use app::window::{hide_main_window, set_main_window_layout, show_settings};
use tauri::Manager;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

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
            set_main_window_layout,
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
