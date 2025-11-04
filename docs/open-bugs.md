## Open Bugs

- **Window minimize does not hide application in production**
  - **Status:** Solved
  - **Platforms:** Desktop build
  - **Details:** Fixed by implementing `WindowEvent::CloseRequested` handler that intercepts close button clicks and hides the window to tray instead of closing.

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
  - **Status:** Open  
  - **Platforms:** Desktop build  
  - **Details:** The “Latest Nudge” section keeps showing the most recent message indefinitely instead of expiring once the reminder window passes.

- **Notification styling feels unpolished**  
  - **Status:** Open  
  - **Platforms:** Desktop build  
  - **Details:** Reminder notifications still use the default system styling with minimal branding. Explore richer layouts (custom icon, large body copy, action buttons) while remaining consistent with the OS look and feel.




Useful Commands:
- pnpm tauri icon path/to/master.png
- pnpm tauri build --bundles rpm
