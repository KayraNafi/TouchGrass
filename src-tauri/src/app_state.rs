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

use tauri::{
    async_runtime::{self, JoinHandle},
    AppHandle, Emitter, Manager, Wry,
};
use tauri_plugin_notification::NotificationExt;

#[cfg(target_os = "linux")]
use notify_rust::Notification as LinuxNotification;

use crate::{events, idle_detection::IdleDetector, tray::TrayState};

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
            autostart_enabled: true, // Enable by default for automatic reminders
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
            prefs.interval_minutes = interval.clamp(2, 240);
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

    pub async fn skip_current_break(&self) {
        let _ = self.control_tx.send(ControlMessage::SkipCurrent).await;
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
    SkipCurrent,
    TriggerNow,
}

fn load_preferences(path: &Path) -> Result<Preferences, AppStateError> {
    if !path.exists() {
        return Ok(Preferences::default());
    }

    let contents = fs::read_to_string(path)?;
    match serde_json::from_str::<Preferences>(&contents) {
        Ok(prefs) => Ok(prefs),
        Err(err) => {
            eprintln!("TouchGrass: preferences.json was invalid ({err}); restoring defaults.");
            backup_corrupt_preferences(path);
            let defaults = Preferences::default();
            save_preferences(path, &defaults)?;
            Ok(defaults)
        }
    }
}

fn save_preferences(path: &Path, prefs: &Preferences) -> Result<(), AppStateError> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, prefs)?;
    Ok(())
}

fn backup_corrupt_preferences(path: &Path) {
    let mut backup_path = path.with_extension("json.corrupt");
    if backup_path.exists() {
        let mut counter = 1;
        loop {
            let candidate = path.with_extension(format!("json.corrupt.{counter}"));
            if !candidate.exists() {
                backup_path = candidate;
                break;
            }
            counter += 1;
        }
    }

    match fs::rename(path, &backup_path) {
        Ok(_) => eprintln!(
            "TouchGrass: moved corrupt preferences to {}",
            backup_path.display()
        ),
        Err(err) => {
            eprintln!("TouchGrass: failed to backup corrupt preferences ({err}); removing file.");
            let _ = fs::remove_file(path);
        }
    }
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

    let idle_detector = IdleDetector::new(prefs.idle_threshold_secs());

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
                    if let Ok(secs) = idle_detector.get_idle_time() {
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
                    if let Ok(secs) = idle_detector.get_idle_time() {
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
                        let now = Utc::now();
                        let mut recalculated_next = Instant::now() + prefs.interval_duration();
                        if let Some(until) = snoozed_until {
                            if until > now {
                                if let Ok(wait) = (until - now).to_std() {
                                    recalculated_next = Instant::now() + wait;
                                } else {
                                    recalculated_next = Instant::now();
                                }
                            } else {
                                snoozed_until = None;
                            }
                        }
                        next_instant = recalculated_next;
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
                    ControlMessage::SkipCurrent => {
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

    // Try multiple icon paths
    let icon_path = [
        // Try from Cargo manifest directory (dev mode - this is src-tauri/)
        std::env::var("CARGO_MANIFEST_DIR")
            .ok()
            .map(|dir| std::path::PathBuf::from(dir).join("icons/128x128.png")),
        // Try resource directory (production)
        app.path()
            .resource_dir()
            .ok()
            .map(|d| d.join("icons/128x128.png")),
        // Try relative to current working directory
        Some(std::path::PathBuf::from("src-tauri/icons/128x128.png")),
        // Try from current executable directory
        std::env::current_exe().ok().and_then(|exe| {
            let icon = exe.parent()?.join("icons/128x128.png");
            Some(icon)
        }),
    ]
    .into_iter()
    .flatten()
    .find(|p| {
        let exists = p.exists();
        if exists {
            eprintln!("TouchGrass: Found icon at: {}", p.display());
        }
        exists
    })
    .and_then(|p| p.canonicalize().ok())
    .map(|p| p.to_string_lossy().to_string())
    .unwrap_or_else(|| {
        eprintln!("TouchGrass: No icon found, using fallback 'touchgrass'");
        "touchgrass".to_string()
    });

    eprintln!("TouchGrass: Using notification icon path: {}", icon_path);

    #[cfg(target_os = "linux")]
    let app_state = app
        .try_state::<Arc<AppState>>()
        .map(|state| state.inner().clone());

    #[cfg(target_os = "linux")]
    let handled_by_native_actions =
        match show_linux_notification_with_actions(app, &message, &icon_path, app_state.clone()) {
            Ok(()) => true,
            Err(err) => {
                eprintln!("TouchGrass: linux notification with actions failed: {err}");
                let _ = app.emit(
                    events::LOG_EVENT,
                    events::LogPayload {
                        level: "error".into(),
                        message: format!("notification action setup failed: {err}"),
                    },
                );
                false
            }
        };

    #[cfg(not(target_os = "linux"))]
    let handled_by_native_actions = false;

    if !handled_by_native_actions {
        // Build notification with app icon (fallback without action buttons)
        let notification_result = app
            .notification()
            .builder()
            .title("TouchGrass")
            .body(message.clone())
            .icon(icon_path.clone())
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
    }

    let _ = app.emit(
        events::REMINDER_EVENT,
        ReminderPayload {
            message,
            sound_enabled: prefs.sound_enabled,
        },
    );
}

#[cfg(target_os = "linux")]
fn show_linux_notification_with_actions(
    app: &AppHandle<Wry>,
    message: &str,
    icon_path: &str,
    state: Option<Arc<AppState>>,
) -> Result<(), notify_rust::error::Error> {
    const ACTION_REMIND_IN_FIVE: &str = "touchgrass.remind_in_5";
    const ACTION_SKIP_BREAK: &str = "touchgrass.skip_break";

    const REMIND_VARIANTS: &[(&str, &str)] = &[
        (
            "Give me five",
            "Notification action: Give me five - stretch IOU noted.",
        ),
        (
            "Hit me in five",
            "Notification action: Hit me in five - calendar set to wiggle.",
        ),
        (
            "Let me finish this",
            "Notification action: Let me finish this - timer's waiting with sass.",
        ),
        (
            "Nudge me in five",
            "Notification action: Nudge me in five - snooze engaged, zen pending.",
        ),
        (
            "Back in five",
            "Notification action: Back in five - chair misses you already.",
        ),
        (
            "Ping me in five",
            "Notification action: Ping me in five - reminder primed and ticking.",
        ),
        (
            "Five-minute breather",
            "Notification action: Five-minute breather - lungs scheduled.",
        ),
        (
            "BRB - 5",
            "Notification action: BRB - 5 - calendar winked, timer reset.",
        ),
        (
            "Snooze (5m)",
            "Notification action: Snooze (5m) - cushions fluffing virtually.",
        ),
        (
            "Circle back in 5",
            "Notification action: Circle back in 5 - orbit plotted.",
        ),
        (
            "Tap me in five",
            "Notification action: Tap me in five - coach has the whistle.",
        ),
        (
            "Five more, coach",
            "Notification action: Five more, coach - hustle annotated.",
        ),
        (
            "Hold my coffee (5m)",
            "Notification action: Hold my coffee - countdown steaming.",
        ),
        (
            "One more commit (5m)",
            "Notification action: One more commit - git blame accepted.",
        ),
        (
            "Let me wrap up (5m)",
            "Notification action: Wrap up (5m) - ribbon pending.",
        ),
        (
            "After this build (5m)",
            "Notification action: After this build - CI/CD bribed.",
        ),
        (
            "After this test (5m)",
            "Notification action: After this test - assertions appeased.",
        ),
        (
            "After this call (5m)",
            "Notification action: After this call - small talk queued.",
        ),
        (
            "Remind in five",
            "Notification action: Remind in five - patience, grasshopper.",
        ),
        (
            "Later - five",
            "Notification action: Later - five - calendar gave a nod.",
        ),
        (
            "Five ticks, please",
            "Notification action: Five ticks - metronome set.",
        ),
        (
            "Back shortly (5m)",
            "Notification action: Back shortly - away message drafted.",
        ),
        (
            "Give me 5 min",
            "Notification action: Give me 5 min - sand timer flipped.",
        ),
        (
            "Hit snooze (5m)",
            "Notification action: Hit snooze - alarm tucked in.",
        ),
    ];

    const SKIP_VARIANTS: &[(&str, &str)] = &[
        (
            "Skip this lap",
            "Notification action: Skip this lap. Hustle responsibly.",
        ),
        (
            "Skip - boss cameo",
            "Notification action: Skip - noted, boss cameo logged.",
        ),
        (
            "Skip, still grinding",
            "Notification action: Skip - grind streak acknowledged.",
        ),
        (
            "Skip this one",
            "Notification action: Skip - this round benched.",
        ),
        (
            "Skip - on a roll",
            "Notification action: Skip - momentum protected.",
        ),
        (
            "Skip - deep focus",
            "Notification action: Skip - tunnel vision honored.",
        ),
        (
            "Skip - deadline sprint",
            "Notification action: Skip - sprint shoes laced.",
        ),
        (
            "Skip - meeting just started",
            "Notification action: Skip - calendar drama respected.",
        ),
        (
            "Skip - quick call",
            "Notification action: Skip - headset hair justified.",
        ),
        (
            "Skip - compiling",
            "Notification action: Skip - compiler chanting arcana.",
        ),
        (
            "Skip - shipping now",
            "Notification action: Skip - release train departing.",
        ),
        (
            "Skip - demo time",
            "Notification action: Skip - stage lights warmed.",
        ),
        (
            "Skip - eyes on logs",
            "Notification action: Skip - log rain interpreted.",
        ),
        (
            "Skip - pair session",
            "Notification action: Skip - duo mode enabled.",
        ),
        (
            "Skip - network flaky",
            "Notification action: Skip - packets doing parkour.",
        ),
        (
            "Skip - not now",
            "Notification action: Skip - vibes evaluated.",
        ),
        (
            "Skip - almost done",
            "Notification action: Skip - finish line in sight.",
        ),
        (
            "Skip - coffee run",
            "Notification action: Skip - caffeine diplomacy underway.",
        ),
        (
            "Skip - writing email",
            "Notification action: Skip - subject line negotiating.",
        ),
        (
            "Skip - keyboard on fire",
            "Notification action: Skip - typing WPM illegal.",
        ),
        (
            "Skip - late-night grind",
            "Notification action: Skip - owls co-signed.",
        ),
        (
            "Skip - screen share",
            "Notification action: Skip - pixels in public.",
        ),
        (
            "Skip - standup soon",
            "Notification action: Skip - jokes rehearsed.",
        ),
    ];

    let mut rng = rng();
    let (remind_label, remind_log) = REMIND_VARIANTS.choose(&mut rng).copied().unwrap_or((
        "Give me five",
        "Notification action: Give me five - stretch IOU noted.",
    ));
    let (skip_label, skip_log) = SKIP_VARIANTS.choose(&mut rng).copied().unwrap_or((
        "Skip this lap",
        "Notification action: Skip this lap. Hustle responsibly.",
    ));

    let handle = LinuxNotification::new()
        .summary("TouchGrass")
        .body(message)
        .icon(icon_path)
        .action(ACTION_REMIND_IN_FIVE, remind_label)
        .action(ACTION_SKIP_BREAK, skip_label)
        .show()?;

    let app_for_actions = app.clone();
    let state_for_actions = state.clone();
    let remind_log = remind_log;
    let skip_log = skip_log;

    async_runtime::spawn_blocking(move || {
        handle.wait_for_action(move |identifier| {
            let app_handle = app_for_actions.clone();
            let state_arc = state_for_actions.clone();

            match identifier {
                ACTION_REMIND_IN_FIVE => {
                    if let Some(state) = state_arc.clone() {
                        async_runtime::spawn(async move {
                            state.snooze(5).await;
                        });
                    }
                    let _ = app_handle.emit(
                        events::LOG_EVENT,
                        events::LogPayload {
                            level: "info".into(),
                            message: remind_log.into(),
                        },
                    );
                }
                ACTION_SKIP_BREAK => {
                    if let Some(state) = state_arc {
                        async_runtime::spawn(async move {
                            state.skip_current_break().await;
                        });
                    }
                    let _ = app_handle.emit(
                        events::LOG_EVENT,
                        events::LogPayload {
                            level: "info".into(),
                            message: skip_log.into(),
                        },
                    );
                }
                _ => {}
            }
        });
    });

    Ok(())
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
