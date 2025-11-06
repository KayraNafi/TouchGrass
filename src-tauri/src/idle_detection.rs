use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Cross-platform idle time tracker with Wayland ext-idle-notify-v1 support
pub struct IdleDetector {
    idle_since_timestamp: Arc<AtomicU64>, // Unix timestamp when user became idle
    is_idle: Arc<AtomicBool>,
    threshold_secs: u64,
    #[cfg(target_os = "linux")]
    wayland_handle: Option<WaylandIdleHandle>,
}

#[cfg(target_os = "linux")]
struct WaylandIdleHandle {
    #[allow(dead_code)] // Kept alive to prevent thread from being dropped
    thread_handle: std::thread::JoinHandle<()>,
}

impl IdleDetector {
    pub fn new(idle_threshold_secs: u64) -> Self {
        let idle_since_timestamp = Arc::new(AtomicU64::new(0));
        let is_idle = Arc::new(AtomicBool::new(false));

        #[cfg(target_os = "linux")]
        {
            let wayland_handle = Self::setup_wayland_idle_detection(
                idle_threshold_secs,
                idle_since_timestamp.clone(),
                is_idle.clone(),
            );

            Self {
                idle_since_timestamp,
                is_idle,
                threshold_secs: idle_threshold_secs,
                wayland_handle,
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            Self {
                idle_since_timestamp,
                is_idle,
                threshold_secs: idle_threshold_secs,
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn setup_wayland_idle_detection(
        threshold_secs: u64,
        idle_since_timestamp: Arc<AtomicU64>,
        is_idle: Arc<AtomicBool>,
    ) -> Option<WaylandIdleHandle> {
        use wayland_client::{
            globals::{registry_queue_init, GlobalListContents},
            protocol::{wl_registry, wl_seat},
            Connection, Dispatch, EventQueue, QueueHandle,
        };
        use wayland_protocols::ext::idle_notify::v1::client::{
            ext_idle_notification_v1::{Event as IdleEvent, ExtIdleNotificationV1},
            ext_idle_notifier_v1::ExtIdleNotifierV1,
        };

        let conn = match Connection::connect_to_env() {
            Ok(conn) => conn,
            Err(_) => return None,
        };

        struct AppData {
            seat: Option<wl_seat::WlSeat>,
            idle_notifier: Option<ExtIdleNotifierV1>,
            idle_since_timestamp: Arc<AtomicU64>,
            is_idle: Arc<AtomicBool>,
            threshold_secs: u64,
        }

        impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for AppData {
            fn event(
                _state: &mut Self,
                _registry: &wl_registry::WlRegistry,
                _event: wl_registry::Event,
                _data: &GlobalListContents,
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
            }
        }

        impl Dispatch<wl_seat::WlSeat, ()> for AppData {
            fn event(
                _state: &mut Self,
                _seat: &wl_seat::WlSeat,
                _event: wl_seat::Event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
            }
        }

        impl Dispatch<ExtIdleNotifierV1, ()> for AppData {
            fn event(
                _state: &mut Self,
                _notifier: &ExtIdleNotifierV1,
                _event: <ExtIdleNotifierV1 as wayland_client::Proxy>::Event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
            }
        }

        impl Dispatch<ExtIdleNotificationV1, ()> for AppData {
            fn event(
                state: &mut Self,
                _notification: &ExtIdleNotificationV1,
                event: IdleEvent,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
                use std::time::SystemTime;

                match event {
                    IdleEvent::Idled => {
                        let now_secs = SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();

                        state.is_idle.store(true, Ordering::Relaxed);
                        state.idle_since_timestamp.store(
                            now_secs.saturating_sub(state.threshold_secs),
                            Ordering::Relaxed,
                        );
                    }
                    IdleEvent::Resumed => {
                        state.is_idle.store(false, Ordering::Relaxed);
                        state.idle_since_timestamp.store(0, Ordering::Relaxed);
                    }
                    _ => {}
                }
            }
        }

        let handle = std::thread::spawn(move || {
            let (globals, mut event_queue): (_, EventQueue<AppData>) =
                match registry_queue_init(&conn) {
                    Ok(result) => result,
                    Err(_) => return,
                };

            let qh = event_queue.handle();

            let mut app_data = AppData {
                seat: None,
                idle_notifier: None,
                idle_since_timestamp,
                is_idle,
                threshold_secs,
            };

            app_data.seat = globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=1, ()).ok();
            app_data.idle_notifier = globals.bind::<ExtIdleNotifierV1, _, _>(&qh, 1..=1, ()).ok();

            if app_data.seat.is_none() || app_data.idle_notifier.is_none() {
                return;
            }

            let seat = app_data.seat.as_ref().unwrap();
            let idle_notifier = app_data.idle_notifier.as_ref().unwrap();
            let timeout_ms = threshold_secs * 1000;
            let _idle_notification =
                idle_notifier.get_idle_notification(timeout_ms as u32, seat, &qh, ());

            loop {
                if event_queue.blocking_dispatch(&mut app_data).is_err() {
                    break;
                }
            }
        });

        Some(WaylandIdleHandle {
            thread_handle: handle,
        })
    }

    /// Get idle time in seconds
    pub fn get_idle_time(&self) -> Result<u64, IdleDetectionError> {
        #[cfg(target_os = "linux")]
        {
            if self.wayland_handle.is_some() {
                // Wayland idle detection is active
                if self.is_idle.load(Ordering::Relaxed) {
                    use std::time::SystemTime;

                    let idle_since = self.idle_since_timestamp.load(Ordering::Relaxed);
                    let now = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    let idle_secs = now.saturating_sub(idle_since);
                    // Return at least the threshold, since that's the minimum idle time
                    return Ok(idle_secs.max(self.threshold_secs));
                } else {
                    return Ok(0);
                }
            }

            // Fall back to X11 detection
            return self.try_x11_idle();
        }

        #[cfg(not(target_os = "linux"))]
        {
            self.try_x11_idle()
        }
    }

    fn try_x11_idle(&self) -> Result<u64, IdleDetectionError> {
        match user_idle2::UserIdle::get_time() {
            Ok(duration) => {
                let secs = duration.as_seconds();
                Ok(secs)
            }
            Err(_e) => Err(IdleDetectionError::X11Error),
        }
    }
}

#[derive(Debug)]
pub enum IdleDetectionError {
    X11Error,
}

impl std::fmt::Display for IdleDetectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdleDetectionError::X11Error => write!(f, "X11 idle detection failed"),
        }
    }
}

impl std::error::Error for IdleDetectionError {}
