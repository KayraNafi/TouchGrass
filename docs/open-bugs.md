## Open Bugs

- **Window minimize does not hide application in production**
  - **Status:** Solved
  - **Platforms:** Desktop build
  - **Details:** Fixed by implementing `WindowEvent::CloseRequested` handler that intercepts close button clicks and hides the window to tray instead of closing.

- **Inactivity detection fails to suppress reminders**  
  - **Status:** Open  
  - **Platforms:** Desktop build  
  - **Details:** Notifications continue firing even when the user remains inactive, indicating the idle threshold logic isn't pausing or resetting timers as expected.

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
