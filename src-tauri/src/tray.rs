use std::sync::Arc;

use tauri::{
    menu::{Menu, MenuBuilder, MenuItemKind},
    tray::TrayIconBuilder,
    AppHandle, Manager, Wry,
};

use crate::app_state::{AppState, StatusSnapshot};

const TRAY_ID: &str = "touchgrass-tray";
const MENU_OPEN: &str = "open-settings";
const MENU_PAUSE: &str = "toggle-pause";
const MENU_SNOOZE_5: &str = "snooze-5";
const MENU_SNOOZE_15: &str = "snooze-15";
const MENU_QUIT: &str = "quit";

#[derive(Clone)]
pub struct TrayState {
    menu: Menu<Wry>,
}

impl TrayState {
    pub fn new(menu: Menu<Wry>) -> Self {
        Self { menu }
    }

    pub fn sync(&self, status: &StatusSnapshot) {
        if let Some(MenuItemKind::Check(check_item)) = self.menu.get(MENU_PAUSE) {
            let paused = status.paused;
            let label = if paused {
                "Resume reminders"
            } else {
                "Pause reminders"
            };
            let _ = check_item.set_checked(paused);
            let _ = check_item.set_text(label);
        }
    }
}

pub fn setup_tray(app: &AppHandle<Wry>, state: Arc<AppState>) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .text(MENU_OPEN, "Open TouchGrass")
        .separator()
        .check(MENU_PAUSE, "Pause reminders")
        .separator()
        .text(MENU_SNOOZE_5, "Snooze 5 minutes")
        .text(MENU_SNOOZE_15, "Snooze 15 minutes")
        .separator()
        .text(MENU_QUIT, "Quit")
        .build()?;

    let tray_state = TrayState::new(menu.clone());
    app.manage(tray_state.clone());

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("TouchGrass");

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    let state_for_menu = state.clone();

    builder
        .on_menu_event(move |app_handle, event| {
            handle_menu_event(app_handle, &state_for_menu, event);
        })
        .build(app)?;

    tray_state.sync(&state.status());

    Ok(())
}

fn handle_menu_event(app: &AppHandle<Wry>, state: &Arc<AppState>, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        MENU_OPEN => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        MENU_PAUSE => {
            let paused = state.status().paused;
            let state = Arc::clone(state);
            let app_handle = app.clone();
            tauri::async_runtime::spawn(async move {
                state.set_pause(!paused).await;
                if let Some(tray_state) = app_handle.try_state::<TrayState>() {
                    tray_state.sync(&state.status());
                }
            });
        }
        MENU_SNOOZE_5 => {
            let state = Arc::clone(state);
            tauri::async_runtime::spawn(async move {
                state.snooze(5).await;
            });
        }
        MENU_SNOOZE_15 => {
            let state = Arc::clone(state);
            tauri::async_runtime::spawn(async move {
                state.snooze(15).await;
            });
        }
        MENU_QUIT => {
            app.exit(0);
        }
        _ => {}
    }
}
