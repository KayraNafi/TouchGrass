use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use rand::{rng, seq::IndexedRandom};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant, MissedTickBehavior};
use user_idle2::UserIdle;

use tauri::{
    async_runtime::{self, JoinHandle},
    AppHandle, Emitter, Manager, Wry,
};
use tauri_plugin_notification::NotificationExt;

use crate::{events, tray::TrayState};

const PREFERENCES_FILE: &str = "preferences.json";
const DEFAULT_IDLE_THRESHOLD_MINUTES: u64 = 2;
const MIN_IDLE_THRESHOLD_MINUTES: u64 = 1;
const MAX_IDLE_THRESHOLD_MINUTES: u64 = 30;
const IDLE_POLL_INTERVAL_SECS: u64 = 20;
const DEFAULT_INTERVAL_MINUTES: u64 = 30;

#[derive(Debug, Error)]
pub enum AppStateError {
    #[error("failed to resolve config directory: {0}")]
    ConfigDir(#[from] tauri::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("task join error: {0}")]
    Join(#[from] tokio::task::JoinError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub interval_minutes: u64,
    pub activity_detection: bool,
    pub sound_enabled: bool,
    pub autostart_enabled: bool,
    pub theme: ThemeMode,
    #[serde(default = "default_idle_threshold_minutes")]
    pub idle_threshold_minutes: u64,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            interval_minutes: DEFAULT_INTERVAL_MINUTES,
            activity_detection: true,
            sound_enabled: true,
            autostart_enabled: false,
            theme: ThemeMode::Dark,
            idle_threshold_minutes: DEFAULT_IDLE_THRESHOLD_MINUTES,
        }
    }
}

impl Preferences {
    pub fn interval_duration(&self) -> Duration {
        Duration::from_secs(self.interval_minutes.max(1) * 60)
    }

    pub fn idle_threshold_secs(&self) -> u64 {
        self.idle_threshold_minutes
            .clamp(MIN_IDLE_THRESHOLD_MINUTES, MAX_IDLE_THRESHOLD_MINUTES)
            .saturating_mul(60)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Dark,
    Light,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusSnapshot {
    pub paused: bool,
    pub snoozed_until: Option<DateTime<Utc>>,
    pub next_trigger_at: Option<DateTime<Utc>>,
    pub last_notification_at: Option<DateTime<Utc>>,
    pub idle_seconds: Option<u64>,
}

impl Default for StatusSnapshot {
    fn default() -> Self {
        Self {
            paused: false,
            snoozed_until: None,
            next_trigger_at: None,
            last_notification_at: None,
            idle_seconds: None,
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderPayload {
    pub message: String,
    pub sound_enabled: bool,
}

pub struct AppState {
    preferences_path: PathBuf,
    preferences: Mutex<Preferences>,
    status: Arc<Mutex<StatusSnapshot>>,
    control_tx: mpsc::Sender<ControlMessage>,
    worker_handle: Mutex<Option<JoinHandle<()>>>,
}

impl AppState {
    pub fn initialize(app: &AppHandle<Wry>) -> Result<Arc<Self>, AppStateError> {
        let config_dir = app.path().app_config_dir()?;
        fs::create_dir_all(&config_dir)?;
        let preferences_path = config_dir.join(PREFERENCES_FILE);
        let preferences = load_preferences(&preferences_path)?;

        let status = Arc::new(Mutex::new(StatusSnapshot::default()));

        let (control_tx, control_rx) = mpsc::channel(16);
        let state = Arc::new(Self {
            preferences_path,
            preferences: Mutex::new(preferences.clone()),
            status: status.clone(),
            control_tx,
            worker_handle: Mutex::new(None),
        });

        let app_handle = app.clone();

        let handle = async_runtime::spawn(async move {
            run_engine(app_handle, status, preferences, control_rx).await;
        });

        *state.worker_handle.lock().unwrap() = Some(handle);

        Ok(state)
    }

    pub fn preferences(&self) -> Preferences {
        self.preferences.lock().unwrap().clone()
    }

    pub fn status(&self) -> StatusSnapshot {
        self.status.lock().unwrap().clone()
    }

    pub async fn update_preferences(
        &self,
        app: &AppHandle<Wry>,
        update: PreferencesUpdate,
    ) -> Result<Preferences, AppStateError> {
        let mut prefs = self.preferences.lock().unwrap().clone();

        if let Some(interval) = update.interval_minutes {
            prefs.interval_minutes = interval.clamp(5, 240);
        }
        if let Some(activity_detection) = update.activity_detection {
            prefs.activity_detection = activity_detection;
        }
        if let Some(sound_enabled) = update.sound_enabled {
            prefs.sound_enabled = sound_enabled;
        }
        if let Some(autostart) = update.autostart_enabled {
            prefs.autostart_enabled = autostart;
        }
        if let Some(theme) = update.theme.clone() {
            prefs.theme = theme;
        }
        if let Some(threshold) = update.idle_threshold_minutes {
            prefs.idle_threshold_minutes = clamp_idle_threshold_minutes(threshold);
        }

        save_preferences(&self.preferences_path, &prefs)?;

        {
            let mut guard = self.preferences.lock().unwrap();
            *guard = prefs.clone();
        }

        self.control_tx
            .send(ControlMessage::PreferencesUpdated(prefs.clone()))
            .await
            .ok();

        if let Some(autostart) = update.autostart_enabled {
            apply_autostart(app, autostart);
        }

        Ok(prefs)
    }

    pub async fn set_pause(&self, paused: bool) {
        let _ = self.control_tx.send(ControlMessage::Pause(paused)).await;
    }

    pub async fn snooze(&self, duration_minutes: u64) {
        let duration = Duration::from_secs(duration_minutes.max(1) * 60);
        let _ = self.control_tx.send(ControlMessage::Snooze(duration)).await;
    }

    pub async fn clear_snooze(&self) {
        let _ = self.control_tx.send(ControlMessage::ClearSnooze).await;
    }

    pub async fn trigger_preview(&self) {
        let _ = self.control_tx.send(ControlMessage::TriggerNow).await;
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        if let Some(handle) = self.worker_handle.lock().unwrap().take() {
            handle.abort();
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesUpdate {
    pub interval_minutes: Option<u64>,
    pub activity_detection: Option<bool>,
    pub sound_enabled: Option<bool>,
    pub autostart_enabled: Option<bool>,
    pub theme: Option<ThemeMode>,
    pub idle_threshold_minutes: Option<u64>,
}

enum ControlMessage {
    PreferencesUpdated(Preferences),
    Pause(bool),
    Snooze(Duration),
    ClearSnooze,
    TriggerNow,
}

fn load_preferences(path: &Path) -> Result<Preferences, AppStateError> {
    if !path.exists() {
        return Ok(Preferences::default());
    }

    let file = File::open(path)?;
    let prefs = serde_json::from_reader(file)?;
    Ok(prefs)
}

fn save_preferences(path: &Path, prefs: &Preferences) -> Result<(), AppStateError> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, prefs)?;
    Ok(())
}

fn default_idle_threshold_minutes() -> u64 {
    DEFAULT_IDLE_THRESHOLD_MINUTES
}

fn clamp_idle_threshold_minutes(minutes: u64) -> u64 {
    minutes.clamp(MIN_IDLE_THRESHOLD_MINUTES, MAX_IDLE_THRESHOLD_MINUTES)
}

fn apply_autostart(app: &AppHandle<Wry>, enable: bool) {
    use tauri_plugin_autostart::ManagerExt;

    let manager = app.autolaunch();
    if enable {
        if let Err(err) = manager.enable() {
            let _ = app.emit(
                events::LOG_EVENT,
                events::LogPayload {
                    level: "error".into(),
                    message: format!("autostart enable failed: {err}"),
                },
            );
        }
    } else if let Err(err) = manager.disable() {
        let _ = app.emit(
            events::LOG_EVENT,
            events::LogPayload {
                level: "error".into(),
                message: format!("autostart disable failed: {err}"),
            },
        );
    }
}

async fn run_engine(
    app: AppHandle<Wry>,
    status: Arc<Mutex<StatusSnapshot>>,
    mut prefs: Preferences,
    mut control_rx: mpsc::Receiver<ControlMessage>,
) {
    apply_autostart(&app, prefs.autostart_enabled);

    let mut paused = false;
    let mut snoozed_until: Option<DateTime<Utc>> = None;
    let mut next_instant = Instant::now() + prefs.interval_duration();
    let sleep = tokio::time::sleep_until(next_instant);
    tokio::pin!(sleep);
    let mut idle_poll = tokio::time::interval(Duration::from_secs(IDLE_POLL_INTERVAL_SECS));
    idle_poll.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut was_idle = false;
    let mut last_idle_secs: Option<u64> = None;

    update_status(&app, &status, |snapshot| {
        snapshot.paused = paused;
        snapshot.snoozed_until = snoozed_until;
        snapshot.next_trigger_at = Some(timestamp_from_instant(next_instant));
        snapshot.idle_seconds = last_idle_secs;
    });

    loop {
        tokio::select! {
            _ = &mut sleep => {
                let now = Utc::now();
                let mut notify_user = !paused;
                let idle_threshold_secs = prefs.idle_threshold_secs();

                if notify_user {
                    if let Some(until) = snoozed_until {
                        if now < until {
                            notify_user = false;
                        } else {
                            snoozed_until = None;
                        }
                    }
                }

                if notify_user && prefs.activity_detection {
                    if let Ok(idle) = UserIdle::get_time() {
                        let secs = idle.as_seconds();
                        last_idle_secs = Some(secs);
                        if secs >= idle_threshold_secs {
                            notify_user = false;
                            was_idle = true;
                        } else {
                            was_idle = false;
                        }
                    }
                } else if !prefs.activity_detection {
                    last_idle_secs = None;
                }

                if notify_user {
                    send_reminder(&app, &prefs).await;
                    update_status(&app, &status, |snapshot| {
                        snapshot.last_notification_at = Some(now);
                        snapshot.idle_seconds = last_idle_secs;
                    });
                } else {
                    update_status(&app, &status, |snapshot| {
                        snapshot.idle_seconds = last_idle_secs;
                    });
                }

                next_instant = Instant::now() + prefs.interval_duration();
                sleep.as_mut().reset(next_instant);
                update_status(&app, &status, |snapshot| {
                    snapshot.paused = paused;
                    snapshot.snoozed_until = snoozed_until;
                    snapshot.next_trigger_at = if paused {
                        None
                    } else {
                        Some(timestamp_from_instant(next_instant))
                    };
                    snapshot.idle_seconds = last_idle_secs;
                });
            }
            _ = idle_poll.tick() => {
                if prefs.activity_detection {
                    if let Ok(idle) = UserIdle::get_time() {
                        let secs = idle.as_seconds();
                        last_idle_secs = Some(secs);
                        let idle_now = secs >= prefs.idle_threshold_secs();
                        let mut updated_next = false;
                        if idle_now {
                            was_idle = true;
                        } else if was_idle {
                            was_idle = false;
                            if !paused {
                                let now = Utc::now();
                                if let Some(until) = snoozed_until {
                                    if until <= now {
                                        snoozed_until = None;
                                    }
                                }
                                let snooze_active = snoozed_until.map(|until| until > Utc::now()).unwrap_or(false);
                                if !snooze_active {
                                    next_instant = Instant::now() + prefs.interval_duration();
                                    sleep.as_mut().reset(next_instant);
                                    updated_next = true;
                                }
                            }
                        }

                        update_status(&app, &status, |snapshot| {
                            snapshot.idle_seconds = last_idle_secs;
                            snapshot.paused = paused;
                            snapshot.snoozed_until = snoozed_until;
                            if paused {
                                snapshot.next_trigger_at = None;
                            } else if updated_next {
                                snapshot.next_trigger_at = Some(timestamp_from_instant(next_instant));
                            }
                        });
                    }
                } else if last_idle_secs.is_some() || was_idle {
                    last_idle_secs = None;
                    was_idle = false;
                    update_status(&app, &status, |snapshot| {
                        snapshot.idle_seconds = last_idle_secs;
                    });
                }
            }
            Some(msg) = control_rx.recv() => {
                match msg {
                    ControlMessage::PreferencesUpdated(new_prefs) => {
                        prefs = new_prefs;
                        next_instant = Instant::now() + prefs.interval_duration();
                        sleep.as_mut().reset(next_instant);
                        update_status(&app, &status, |snapshot| {
                            snapshot.next_trigger_at = Some(timestamp_from_instant(next_instant));
                            snapshot.idle_seconds = last_idle_secs;
                        });
                    }
                    ControlMessage::Pause(flag) => {
                        paused = flag;
                        if !paused {
                            next_instant = Instant::now() + prefs.interval_duration();
                            sleep.as_mut().reset(next_instant);
                        }
                        update_status(&app, &status, |snapshot| {
                            snapshot.paused = paused;
                            snapshot.next_trigger_at = if paused {
                                None
                            } else {
                                Some(timestamp_from_instant(next_instant))
                            };
                            snapshot.idle_seconds = last_idle_secs;
                        });
                    }
                    ControlMessage::Snooze(duration) => {
                        let until = Utc::now() + chrono::Duration::from_std(duration).unwrap();
                        snoozed_until = Some(until);
                        next_instant = Instant::now() + duration;
                        sleep.as_mut().reset(next_instant);
                        update_status(&app, &status, |snapshot| {
                            snapshot.snoozed_until = snoozed_until;
                            snapshot.next_trigger_at = Some(timestamp_from_instant(next_instant));
                            snapshot.idle_seconds = last_idle_secs;
                        });
                    }
                    ControlMessage::ClearSnooze => {
                        snoozed_until = None;
                        if !paused {
                            next_instant = Instant::now() + prefs.interval_duration();
                            sleep.as_mut().reset(next_instant);
                        }
                        update_status(&app, &status, |snapshot| {
                            snapshot.snoozed_until = None;
                            snapshot.next_trigger_at = if paused {
                                None
                            } else {
                                Some(timestamp_from_instant(next_instant))
                            };
                            snapshot.idle_seconds = last_idle_secs;
                        });
                    }
                    ControlMessage::TriggerNow => {
                        send_reminder(&app, &prefs).await;
                        let now = Utc::now();
                        update_status(&app, &status, |snapshot| {
                            snapshot.last_notification_at = Some(now);
                            snapshot.idle_seconds = last_idle_secs;
                        });
                        next_instant = Instant::now() + prefs.interval_duration();
                        sleep.as_mut().reset(next_instant);
                        update_status(&app, &status, |snapshot| {
                            snapshot.next_trigger_at = Some(timestamp_from_instant(next_instant));
                            snapshot.idle_seconds = last_idle_secs;
                        });
                    }
                }
            }
        }
    }
}

fn timestamp_from_instant(instant: Instant) -> DateTime<Utc> {
    let now = Instant::now();
    let offset = if instant >= now {
        instant - now
    } else {
        Duration::from_secs(0)
    };
    Utc::now() + chrono::Duration::from_std(offset).unwrap_or_default()
}

fn update_status<F>(app: &AppHandle<Wry>, status: &Arc<Mutex<StatusSnapshot>>, mut update_fn: F)
where
    F: FnMut(&mut StatusSnapshot),
{
    let snapshot = {
        let mut guard = status.lock().unwrap();
        update_fn(&mut guard);
        guard.clone()
    };

    if let Some(tray_state) = app.try_state::<TrayState>() {
        tray_state.sync(&snapshot);
    }

    let _ = app.emit(
        events::STATUS_EVENT,
        events::StatusPayload { status: snapshot },
    );
}

async fn send_reminder(app: &AppHandle<Wry>, prefs: &Preferences) {
    let message = choose_reminder_message();
    let notification_result = app
        .notification()
        .builder()
        .title("TouchGrass")
        .body(message.clone())
        .show();

    if let Err(err) = notification_result {
        let _ = app.emit(
            events::LOG_EVENT,
            events::LogPayload {
                level: "error".into(),
                message: format!("notification error: {err}"),
            },
        );
    }

    let _ = app.emit(
        events::REMINDER_EVENT,
        ReminderPayload {
            message,
            sound_enabled: prefs.sound_enabled,
        },
    );
}

fn choose_reminder_message() -> String {
    const MESSAGES: &[&str] = &[
        "Stand up before you photosynthesize.",
        "Touch grass (nearby plant also counts).",
        "Keyboard's hot, legs are not.",
        "Blink like you mean it: 10x.",
        "Break speedrun in 30s. Go.",
        "Free DLC: posture.",
        "Up. Now. Your chair has attachment issues.",
        "Stand before you grow roots.",
        "Blink or become a raisin.",
        "Walk away like the main character.",
        "Your spine filed a ticket.",
        "Walk. The chair will cope.",
        "Your posture called HR.",
        "Side quest: 30s breathing.",
        "Keyboard is not a life partner.",
        "AFK or AF-ache.",
        "Stare at something >20ft, not your soul.",
        "Load-bearing human requires maintenance.",
    ];

    let mut rng = rng();
    MESSAGES
        .choose(&mut rng)
        .unwrap_or(&"Time for a quick reset.")
        .to_string()
}
