## Open Bugs

- **Window minimize does not hide application to tray in production**
  - **Status:** Partially Solved (Close button works, Minimize button doesn't)
  - **Platforms:** Desktop build (KDE Plasma on Wayland/X11)
  - **Details:**
    - **Close button:** Fixed by implementing `WindowEvent::CloseRequested` handler that intercepts close button clicks and hides the window to tray instead of closing.
    - **Minimize button:** Cannot be intercepted on KDE - this is a fundamental limitation of how Tauri/Winit interact with KDE's window manager (KWin).
  - **Root Cause:**
    - No `MinimizeRequested` event exists in Tauri/Winit
    - Window state queries (`is_minimized()`, `is_visible()`) return incorrect values on KDE Wayland/X11
    - KWin handles minimize action directly without properly notifying the application through Wayland protocol
    - Only generic events fire (`Focused(false)`, `Moved`, `Resized`) which also occur during normal window interactions
  - **Potential Solutions (for KDE specialist):**
    - Check if KWin exposes minimize events through Wayland protocols (e.g., `xdg-toplevel` state changes)
    - Explore KDE-specific Wayland protocols or D-Bus signals to intercept minimize
    - Investigate compositor-specific APIs to catch minimize button clicks before KWin handles them

- **Inactivity detection fails to suppress reminders**
  - **Status:** Solved
  - **Platforms:** Desktop build (Linux Wayland, Linux X11, Windows, macOS)
  - **Root Cause:** The `user-idle2` crate relied on MIT-SCREEN-SAVER X11 extension which doesn't exist on Wayland. Idle time was always returning 0 seconds on Wayland systems, so reminders were never suppressed.
  - **Fix:** Implemented proper cross-platform idle detection (`src-tauri/src/idle_detection.rs`):
    - **Linux Wayland:** Uses `ext-idle-notify-v1` protocol (supports KDE Plasma 5.27+, GNOME, Sway, Hyprland, and all wlroots-based compositors)
    - **Linux X11:** Falls back to `user-idle2` crate (MIT-SCREEN-SAVER extension)
    - **Windows:** Uses `user-idle2` crate (native Windows idle detection APIs)
    - **macOS:** Uses `user-idle2` crate (native macOS idle detection APIs)
  - **Implementation:** Event-based Wayland detection runs in dedicated thread, seamlessly falls back to polling-based detection on other platforms

- **Latest Nudge label never clears**
  - **Status:** Solved (removed feature)
  - **Platforms:** Desktop build
  - **Details:** The "Latest Nudge" section kept showing the most recent message indefinitely instead of expiring once the reminder window passed.
  - **Fix:** Removed the "Latest Nudge" section entirely as it was redundant with the "Last break" display. Added real-time updates (every 10 seconds) to all time-based displays including "Next reminder" and "Last break".

- **Notification styling feels unpolished**
  - **Status:** Solved
  - **Platforms:** Desktop build
  - **Details:** Reminder notifications were using the default system styling with minimal branding.
  - **Fix:** Added the TouchGrass app icon to notifications using the default window icon. The icon now appears alongside the notification title and body, improving brand recognition and visual polish while maintaining consistency with the OS native notification style.




Useful Commands:
- pnpm tauri icon path/to/master.png
- pnpm tauri build --bundles rpm
