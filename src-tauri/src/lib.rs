mod app_state;
mod events;
mod idle_detection;
mod tray;

use std::sync::Arc;

use app_state::{AppState, Preferences, PreferencesUpdate, StatusSnapshot};
use events::StatusPayload;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent, Wry};
use tauri_plugin_autostart::MacosLauncher;
#[cfg(desktop)]
use tauri_plugin_updater::Builder as UpdaterBuilder;

type CommandResult<T> = Result<T, String>;

#[tauri::command]
async fn get_preferences(state: State<'_, Arc<AppState>>) -> CommandResult<Preferences> {
    Ok(state.preferences())
}

#[tauri::command]
async fn update_preferences(
    app: AppHandle<Wry>,
    state: State<'_, Arc<AppState>>,
    update: PreferencesUpdate,
) -> CommandResult<Preferences> {
    state
        .update_preferences(&app, update)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_status(state: State<'_, Arc<AppState>>) -> CommandResult<StatusSnapshot> {
    Ok(state.status())
}

#[tauri::command]
async fn set_pause_state(state: State<'_, Arc<AppState>>, paused: bool) -> CommandResult<()> {
    state.set_pause(paused).await;
    Ok(())
}

#[tauri::command]
async fn snooze_for_minutes(state: State<'_, Arc<AppState>>, minutes: u64) -> CommandResult<()> {
    state.snooze(minutes).await;
    Ok(())
}

#[tauri::command]
async fn clear_snooze(state: State<'_, Arc<AppState>>) -> CommandResult<()> {
    state.clear_snooze().await;
    Ok(())
}

#[tauri::command]
async fn trigger_preview(state: State<'_, Arc<AppState>>) -> CommandResult<()> {
    state.trigger_preview().await;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            get_preferences,
            update_preferences,
            get_status,
            set_pause_state,
            snooze_for_minutes,
            clear_snooze,
            trigger_preview
        ])
        .setup(|app| {
            #[cfg(desktop)]
            {
                app.handle()
                    .plugin(UpdaterBuilder::new().build())
                    .map_err(|e| boxed(e))?;
            }

            let app_handle = app.handle();
            let state = AppState::initialize(&app_handle).map_err(|e| boxed(e))?;
            let tray_state = state.clone();

            app.manage(state.clone());

            tray::setup_tray(&app_handle, tray_state).map_err(|e| boxed(e))?;

            // Check if launched with --autostart flag (from login)
            let args: Vec<String> = std::env::args().collect();
            let is_autostart = args.iter().any(|arg| arg == "--autostart");

            if is_autostart {
                // Hide window on autostart - run in tray only
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            // Emit initial status for UI bootstrap.
            let _ = app.emit(
                events::STATUS_EVENT,
                StatusPayload {
                    status: state.status(),
                },
            );

            Ok(())
        })
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "quit" {
                app.exit(0);
            }
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }

            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    // Prevent the window from closing, hide it instead
                    api.prevent_close();
                    let _ = window.hide();
                }
                WindowEvent::Resized(_) => {
                    // Also handle minimize button (fallback for platforms that emit this)
                    if let Ok(true) = window.is_minimized() {
                        let _ = window.hide();
                    }
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|app, event| {
            if let Some(tray) = app.tray_by_id(event.id()) {
                let _ = tray.set_visible(true);
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running TouchGrass");
}

fn boxed<E: std::error::Error + 'static>(err: E) -> Box<dyn std::error::Error> {
    Box::new(err)
}
