<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";

  type Preferences = {
    intervalMinutes: number;
    activityDetection: boolean;
    soundEnabled: boolean;
    autostartEnabled: boolean;
    theme: "dark" | "light";
    idleThresholdMinutes: number;
  };

  type Status = {
    paused: boolean;
    snoozedUntil: string | null;
    nextTriggerAt: string | null;
    lastNotificationAt: string | null;
    idleSeconds: number | null;
  };

  type StatusEvent = { status: Status };
  type ReminderEvent = { message: string; soundEnabled: boolean };

  const intervalPresets = [15, 25, 30, 45, 60, 90];

  let preferences = $state<Preferences | null>(null);
  let status = $state<Status | null>(null);
  let customInterval = $state<number | null>(null);
  let isLoading = $state(true);
  let pending = $state(false);
  let toastMessage = $state<string | null>(null);
  let lastReminderMessage = $state<string | null>(null);
  let lastReminderAt = $state<Date | null>(null);

  let audioContext: AudioContext | null = null;
  let unlistenStatus: UnlistenFn | null = null;
  let unlistenReminder: UnlistenFn | null = null;

  onMount(async () => {
    await loadInitial();
    await setupListeners();
  });

  onDestroy(() => {
    unlistenStatus?.();
    unlistenReminder?.();
  });

  async function loadInitial() {
    try {
      const [prefs, stat] = await Promise.all([
        invoke<Preferences>("get_preferences"),
        invoke<Status>("get_status"),
      ]);
      preferences = prefs;
      status = stat;
      customInterval = intervalPresets.includes(prefs.intervalMinutes)
        ? null
        : prefs.intervalMinutes;
    } catch (error) {
      console.error("TouchGrass: failed to load preferences", error);
      showToast("Could not load preferences");
    } finally {
      isLoading = false;
    }
  }

  async function setupListeners() {
    unlistenStatus = await listen<StatusEvent>("touchgrass://status", (event) => {
      status = event.payload.status;
    });

    unlistenReminder = await listen<ReminderEvent>(
      "touchgrass://reminder",
      (event) => handleReminder(event.payload),
    );
  }

  function ensureAudioContext() {
    if (!audioContext && typeof window !== "undefined") {
      audioContext = new AudioContext();
    }
    return audioContext;
  }

  async function playChime() {
    const ctx = ensureAudioContext();
    if (!ctx) return;
    if (ctx.state === "suspended") {
      try {
        await ctx.resume();
      } catch (error) {
        console.warn("TouchGrass: unable to resume audio context", error);
      }
    }

    const now = ctx.currentTime;
    const oscillator = ctx.createOscillator();
    const gain = ctx.createGain();

    oscillator.type = "sine";
    oscillator.frequency.setValueAtTime(880, now);

    gain.gain.setValueAtTime(0.0001, now);
    gain.gain.exponentialRampToValueAtTime(0.18, now + 0.03);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + 1.1);

    oscillator.connect(gain).connect(ctx.destination);
    oscillator.start(now);
    oscillator.stop(now + 1.1);
  }

  function handleReminder(payload: ReminderEvent) {
    lastReminderMessage = payload.message;
    lastReminderAt = new Date();
    if (payload.soundEnabled) {
      playChime();
    }
  }

  async function applyPreference(update: Partial<Preferences>) {
    if (!preferences) return;
    pending = true;
    try {
      const updated = await invoke<Preferences>("update_preferences", {
        update,
      });
      preferences = updated;
      if (update.intervalMinutes !== undefined) {
        customInterval = intervalPresets.includes(updated.intervalMinutes)
          ? null
          : updated.intervalMinutes;
      }
    } catch (error) {
      console.error("TouchGrass: failed to persist preference", error);
      showToast("Preference could not be saved");
    } finally {
      pending = false;
    }
  }

  function handleIntervalSelect(value: number) {
    if (!preferences || pending) return;
    applyPreference({ intervalMinutes: value });
  }

  function handleCustomIntervalChange(value: number) {
    if (!preferences || pending) return;
    if (Number.isNaN(value)) return;
    const clamped = Math.min(Math.max(value, 5), 240);
    customInterval = clamped;
    applyPreference({ intervalMinutes: clamped });
  }

  async function toggleActivityDetection(enabled: boolean) {
    await applyPreference({ activityDetection: enabled });
  }

  async function toggleSound(enabled: boolean) {
    await applyPreference({ soundEnabled: enabled });
  }

  async function toggleAutostart(enabled: boolean) {
    await applyPreference({ autostartEnabled: enabled });
  }

  async function setTheme(theme: "dark" | "light") {
    await applyPreference({ theme });
  }

  async function setIdleThreshold(minutes: number) {
    if (!preferences || pending) return;
    if (Number.isNaN(minutes)) return;
    const rounded = Math.round(minutes);
    const clamped = Math.min(Math.max(rounded, 1), 30);
    await applyPreference({ idleThresholdMinutes: clamped });
  }

  async function togglePause() {
    if (!status) return;
    const next = !status.paused;
    try {
      await invoke<void>("set_pause_state", { paused: next });
      status = { ...status, paused: next };
      showToast(next ? "Reminders paused" : "Reminders resumed");
    } catch (error) {
      console.error("TouchGrass: failed to toggle pause", error);
      showToast("Could not pause reminders");
    }
  }

  async function snooze(minutes: number) {
    try {
      await invoke<void>("snooze_for_minutes", { minutes });
      showToast(`Snoozed for ${minutes} minutes`);
    } catch (error) {
      console.error("TouchGrass: failed to snooze", error);
      showToast("Could not snooze reminders");
    }
  }

  async function clearSnooze() {
    try {
      await invoke<void>("clear_snooze");
      showToast("Snooze cancelled");
    } catch (error) {
      console.error("TouchGrass: failed to clear snooze", error);
      showToast("Could not cancel snooze");
    }
  }

  async function triggerPreview() {
    try {
      await invoke<void>("trigger_preview");
      showToast("Preview reminder sent");
    } catch (error) {
      console.error("TouchGrass: failed to send preview", error);
      showToast("Could not send preview reminder");
    }
  }

  function showToast(message: string) {
    toastMessage = message;
    setTimeout(() => {
      toastMessage = null;
    }, 2200);
  }

  function formatNextLabel(): string {
    if (!status) return "Loading…";
    if (status.paused) return "Paused";
    if (status.snoozedUntil) {
      const until = parseDate(status.snoozedUntil);
      return `Snoozed until ${formatClock(until)}`;
    }
    const next = parseDate(status.nextTriggerAt);
    if (!next) return "Calculating…";
    return `${formatRelative(next)} (${formatClock(next)})`;
  }

  function formatLastLabel(): string {
    if (!status || !status.lastNotificationAt) return "Not yet";
    const last = parseDate(status.lastNotificationAt);
    return `${formatRelative(last)} (${formatClock(last)})`;
  }

  function formatIdleLabel(): string {
    if (status?.idleSeconds == null) return "No activity data";
    const minutes = Math.floor(status.idleSeconds / 60);
    if (minutes <= 0) return "Active now";
    if (minutes === 1) return "Idle for 1 minute";
    return `Idle for ${minutes} minutes`;
  }

  function lastNudgeDate(): Date | null {
    if (lastReminderAt) return lastReminderAt;
    return parseDate(status?.lastNotificationAt ?? null);
  }

  const lastNudge = $derived(lastNudgeDate());

  function isSnoozedActive(): boolean {
    if (!status?.snoozedUntil) return false;
    const until = parseDate(status.snoozedUntil);
    if (!until) return false;
    return until.getTime() > Date.now();
  }

  function parseDate(value: string | null): Date | null {
    if (!value) return null;
    const parsed = new Date(value);
    return Number.isNaN(parsed.valueOf()) ? null : parsed;
  }

  function formatRelative(date: Date | null): string {
    if (!date) return "";
    const diffMinutes = Math.round((date.getTime() - Date.now()) / 60000);
    if (diffMinutes === 0) return "Now";
    if (diffMinutes === 1) return "In 1 minute";
    if (diffMinutes > 1) return `In ${diffMinutes} minutes`;
    const past = Math.abs(diffMinutes);
    if (past === 1) return "1 minute ago";
    return `${past} minutes ago`;
  }

  function formatClock(date: Date | null): string {
    if (!date) return "";
    return date.toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
      hourCycle: "h23",
    });
  }

  $effect(() => {
    if (preferences) {
      document.documentElement.dataset.theme = preferences.theme;
    }
  });
</script>

<main class="app">
  <header class="top">
    <div class="top__brand">
      <span class="top__badge">TouchGrass</span>
      <h1>Break reminders, without the clutter.</h1>
      <p>Set it once and let quick nudges keep you moving.</p>
    </div>
    <div class="top__actions">
      <label
        class="theme-switch"
        class:theme-switch--disabled={pending || !preferences}
      >
        <input
          type="checkbox"
          checked={preferences?.theme !== "light"}
          onchange={(event) => setTheme(event.currentTarget.checked ? "dark" : "light")}
          disabled={pending || !preferences}
        />
        <span
          class="theme-switch__track"
          data-active={preferences?.theme !== "light"}
        >
          <span class="theme-switch__thumb"></span>
        </span>
        <span class="theme-switch__caption">
          {preferences?.theme === "light" ? "Light" : "Dark"} mode
        </span>
      </label>

    </div>
  </header>

  <section class="top-panel">
    <div class="summary">
      <div class="summary__item">
        <div class="summary__line">
          <span class="summary__label">Next reminder</span>
          <span class="summary__value">{formatNextLabel()}</span>
        </div>
        {#if status?.idleSeconds !== null}
          <span class="summary__meta">{formatIdleLabel()}</span>
        {/if}
      </div>

      <div class="summary__item">
        <div class="summary__line">
          <span class="summary__label">Last break</span>
          <span class="summary__value">{formatLastLabel()}</span>
        </div>
      </div>

      <div class="summary__item summary__item--highlight">
        <div class="summary__line">
          <span class="summary__label">Latest nudge</span>
          <span class="summary__value summary__value--small">
            {lastNudge ? formatClock(lastNudge) : "Not yet"}
          </span>
        </div>
        {#if lastNudge}
          <span class="summary__meta">{formatRelative(lastNudge)}</span>
        {/if}
      </div>
    </div>

    <div class="actions">
      <div class="actions__buttons">
        <button
          type="button"
          class="button button--ghost actions__send"
          onclick={triggerPreview}
          disabled={pending || isLoading}
        >
          Send test reminder
        </button>
        <button
          type="button"
          class="button button--primary actions__pause"
          onclick={togglePause}
          disabled={pending || isLoading}
        >
          {status?.paused ? "Resume reminders" : "Pause reminders"}
        </button>

        <div class="actions__snooze">
          {#if isSnoozedActive()}
            <button
              type="button"
              class="button button--warning button--compact"
              onclick={clearSnooze}
              disabled={pending || isLoading}
            >
              Stop snooze
            </button>
          {:else}
            <button
              type="button"
              class="button button--ghost button--compact"
              onclick={() => snooze(5)}
              disabled={pending || isLoading}
            >
              Snooze 5m
            </button>
            <button
              type="button"
              class="button button--ghost button--compact"
              onclick={() => snooze(15)}
              disabled={pending || isLoading}
            >
              15m
            </button>
          {/if}
        </div>
      </div>
    </div>
  </section>

  <section class="settings-grid">
    <section class="card">
      <div class="card__title">
        <h2>Reminder cadence</h2>
        <span class="card__title-help">Choose how often TouchGrass nudges you.</span>
      </div>
      <div class="interval-group">
        {#each intervalPresets as option}
          <button
            type="button"
            class="interval"
            class:interval--active={preferences?.intervalMinutes === option}
            onclick={() => handleIntervalSelect(option)}
            disabled={pending || isLoading}
          >
            {option} min
          </button>
        {/each}
        <div class="interval interval--custom">
          <label for="custom-interval">Custom</label>
          <input
            id="custom-interval"
            type="number"
            min="5"
            max="240"
            placeholder="min"
            value={customInterval ?? ""}
            onchange={(event) =>
              handleCustomIntervalChange(parseInt(event.currentTarget.value, 10))}
            disabled={pending || isLoading}
          />
        </div>
      </div>
    </section>

    <section class="card">
      <div class="preferences">
        <label class="toggle with-help" data-help="Skip reminders when you have been idle for ~2 minutes.">
          <span class="toggle__label">Activity detection</span>
          <input
            type="checkbox"
            checked={preferences?.activityDetection}
            onchange={(event) => toggleActivityDetection(event.currentTarget.checked)}
            disabled={pending || isLoading}
          />
          <span
            class="toggle__visual"
            data-active={preferences?.activityDetection}
          ></span>
        </label>

        <label class="toggle with-help" data-help="Play a soft tone alongside notifications.">
          <span class="toggle__label">Chime when reminding</span>
          <input
            type="checkbox"
            checked={preferences?.soundEnabled}
            onchange={(event) => toggleSound(event.currentTarget.checked)}
            disabled={pending || isLoading}
          />
          <span class="toggle__visual" data-active={preferences?.soundEnabled}></span>
        </label>

        <div
          class="metric-row with-help"
          data-help="When you’re idle at least this long, the timer restarts once you return."
        >
          <span class="metric-row__label">Break resets after</span>
          <div class="metric-row__input">
            <input
              type="number"
              min="1"
              max="30"
              value={preferences?.idleThresholdMinutes ?? 2}
              onchange={(event) => setIdleThreshold(parseInt(event.currentTarget.value, 10))}
              disabled={pending || isLoading}
            />
            <span>min</span>
          </div>
        </div>

        <label
          class="toggle with-help"
          data-help="Start TouchGrass quietly whenever you sign in."
        >
          <span class="toggle__label">Launch on login</span>
          <input
            type="checkbox"
            checked={preferences?.autostartEnabled}
            onchange={(event) => toggleAutostart(event.currentTarget.checked)}
            disabled={pending || isLoading}
          />
          <span
            class="toggle__visual"
            data-active={preferences?.autostartEnabled}
          ></span>
        </label>
      </div>
    </section>
  </section>

  <footer class="footer">
    <p>TouchGrass runs offline and lives in your tray when you need it.</p>
  </footer>

  {#if toastMessage}
    <div class="toast">{toastMessage}</div>
  {/if}
</main>

<style>
.app {
  display: flex;
  flex-direction: column;
  gap: 1.75rem;
  padding: 2.75rem clamp(1.25rem, 5vw, 3rem) 3rem;
  max-width: 760px;
  margin: 0 auto;
}

.top {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: flex-start;
  gap: 1.25rem;
}

.top__brand {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
  max-width: 420px;
}

.top__badge {
  align-self: flex-start;
  padding: 0.25rem 0.75rem;
  border-radius: 999px;
  background: rgba(45, 134, 89, 0.18);
  color: var(--tg-color-accent);
  font-size: 0.75rem;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.top__brand h1 {
  margin: 0;
  font-size: clamp(1.8rem, 4vw, 2.2rem);
  line-height: 1.1;
}

.top__brand p {
  margin: 0;
  color: var(--tg-color-text-soft);
  font-size: 0.95rem;
}

.top__actions {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 0.65rem;
}

.theme-switch {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  color: var(--tg-color-text-soft);
  font-size: 0.85rem;
  cursor: pointer;
  user-select: none;
}

.theme-switch--disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.theme-switch--disabled .theme-switch__caption {
  pointer-events: none;
}

.theme-switch input {
  position: absolute;
  opacity: 0;
  pointer-events: none;
}

.theme-switch__track {
  position: relative;
  width: 40px;
  height: 22px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.08);
  transition: background 0.2s ease;
}

.theme-switch__thumb {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: rgba(0, 0, 0, 0.7);
  transition: transform 0.2s ease, background 0.2s ease;
}

.theme-switch__track[data-active="true"] {
  background: var(--tg-color-primary);
}

.theme-switch__track[data-active="true"] .theme-switch__thumb {
  transform: translateX(18px);
  background: rgba(0, 0, 0, 0.85);
}

.theme-switch__caption {
  font-weight: 600;
}

.top-panel {
  display: grid;
  gap: 1.25rem;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: start;
  background: var(--tg-color-surface);
  border: 1px solid var(--tg-color-border);
  border-radius: var(--tg-radius-md);
  padding: 1.4rem;
  box-shadow: var(--tg-shadow-subtle);
}

.summary {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.summary__item {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  padding: 0.65rem 0.8rem;
  border-radius: var(--tg-radius-sm);
  background: rgba(255, 255, 255, 0.02);
  border: 1px solid rgba(255, 255, 255, 0.04);
}

.summary__item--highlight {
  border: 1px solid rgba(45, 134, 89, 0.35);
  background: rgba(45, 134, 89, 0.12);
}

.summary__line {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 1rem;
  flex-wrap: wrap;
}

.summary__label {
  font-size: 0.9rem;
  font-weight: 600;
  color: var(--tg-color-text-soft);
  letter-spacing: 0.01em;
}

.summary__label::after {
  content: ":";
  margin-left: 0.35rem;
  color: inherit;
}

.summary__value {
  font-size: 0.95rem;
  font-weight: 600;
  color: var(--tg-color-text);
  text-align: right;
  white-space: nowrap;
  margin-left: auto;
}

.summary__value--small {
  font-size: 0.9rem;
}

.summary__meta {
  font-size: 0.78rem;
  color: var(--tg-color-text-soft);
  margin-top: 0.2rem;
}

.actions {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  align-items: flex-end;
}

.actions__buttons {
  display: flex;
  flex-direction: column;
  gap: 0.65rem;
  align-items: flex-end;
}

.actions__send {
  align-self: flex-end;
  padding-inline: 1.15rem;
}

.actions__pause {
  min-width: 180px;
}

.actions__snooze {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 0.5rem;
}

.card {
  background: var(--tg-color-surface);
  border: 1px solid var(--tg-color-border);
  border-radius: var(--tg-radius-md);
  padding: 1.25rem;
  display: flex;
  flex-direction: column;
  gap: 0.9rem;
  box-shadow: var(--tg-shadow-subtle);
}

.settings-grid {
  display: grid;
  gap: 1rem;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
}

.card__title {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 0.5rem;
  position: relative;
}

.card__title h2 {
  margin: 0;
  font-size: 1rem;
  font-weight: 600;
  white-space: nowrap;
  flex-shrink: 0;
}

.card__title-help {
  font-size: 0.78rem;
  color: var(--tg-color-text-soft);
  opacity: 0;
  transform: translateY(-2px);
  transition: opacity 0.15s ease;
  pointer-events: none;
}

.card__title:hover .card__title-help,
.card__title:focus-within .card__title-help {
  opacity: 1;
}

.interval-group {
  display: flex;
  flex-wrap: wrap;
  gap: 0.6rem;
}

.interval {
  border-radius: var(--tg-radius-md);
  border: 1px solid rgba(168, 213, 186, 0.25);
  padding: 0.6rem 1.05rem;
  background: rgba(255, 255, 255, 0.03);
  color: var(--tg-color-text);
  font-weight: 600;
  font-size: 0.9rem;
  cursor: pointer;
  transition: transform 0.15s ease, box-shadow 0.15s ease, border 0.15s ease;
}

.interval:hover {
  transform: translateY(-1px);
  box-shadow: var(--tg-shadow-soft);
}

.interval--active {
  border-color: rgba(45, 134, 89, 0.65);
  background: rgba(45, 134, 89, 0.25);
}

.interval--custom {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  padding-right: 0.75rem;
}

.interval--custom label {
  font-size: 0.8rem;
  color: var(--tg-color-text-soft);
  letter-spacing: 0.02em;
}

.interval--custom input {
  width: 60px;
  border-radius: var(--tg-radius-md);
  border: 1px solid rgba(45, 134, 89, 0.35);
  padding: 0.5rem 0.55rem;
  background: rgba(255, 255, 255, 0.04);
  color: inherit;
}

.preferences {
  display: flex;
  flex-direction: column;
  gap: 0.9rem;
}

.with-help {
  position: relative;
}

.with-help:hover::after,
.with-help:focus-within::after {
  content: attr(data-help);
  position: absolute;
  bottom: calc(100% + 0.5rem);
  left: 0;
  padding: 0.45rem 0.65rem;
  border-radius: var(--tg-radius-sm);
  background: rgba(14, 18, 16, 0.95);
  border: 1px solid rgba(168, 213, 186, 0.25);
  color: var(--tg-color-text-soft);
  font-size: 0.78rem;
  line-height: 1.3;
  box-shadow: var(--tg-shadow-soft);
  max-width: 250px;
  z-index: 5;
  pointer-events: none;
}

.with-help:hover::before,
.with-help:focus-within::before {
  content: "";
  position: absolute;
  bottom: calc(100% + 0.2rem);
  left: 1rem;
  width: 8px;
  height: 8px;
  transform: rotate(45deg);
  background: rgba(14, 18, 16, 0.95);
  border-left: 1px solid rgba(168, 213, 186, 0.25);
  border-top: 1px solid rgba(168, 213, 186, 0.25);
  z-index: 5;
  pointer-events: none;
}

.toggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding: 0.9rem 1.1rem;
  border-radius: var(--tg-radius-md);
  border: 1px solid rgba(168, 213, 186, 0.28);
  background: rgba(20, 24, 22, 0.88);
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.03);
  cursor: pointer;
}

.toggle:focus-within {
  outline: 2px solid rgba(45, 134, 89, 0.5);
  outline-offset: 2px;
}

.toggle__label {
  font-size: 0.95rem;
  font-weight: 600;
  letter-spacing: 0.01em;
}

.toggle input {
  position: absolute;
  opacity: 0;
  pointer-events: none;
}

.toggle__visual {
  position: relative;
  flex-shrink: 0;
  width: 48px;
  height: 28px;
  border-radius: 999px;
  background: rgba(60, 70, 64, 0.55);
  transition: background 0.2s ease;
}

.toggle__visual::after {
  content: "";
  position: absolute;
  top: 3px;
  left: 3px;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  background: rgba(0, 0, 0, 0.7);
  transition: transform 0.2s ease, background 0.2s ease;
}

.toggle__visual[data-active="true"] {
  background: linear-gradient(135deg, rgba(45, 134, 89, 0.9), rgba(53, 162, 105, 0.95));
}

.toggle__visual[data-active="true"]::after {
  transform: translateX(20px);
  background: rgba(3, 12, 6, 0.9);
}

.metric-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  padding: 0.85rem 1.1rem;
  border-radius: var(--tg-radius-md);
  border: 1px solid rgba(168, 213, 186, 0.28);
  background: rgba(20, 24, 22, 0.88);
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.03);
}

.metric-row__label {
  font-size: 0.95rem;
  font-weight: 600;
  letter-spacing: 0.01em;
}

.metric-row__input {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(45, 134, 89, 0.35);
  border-radius: var(--tg-radius-md);
  padding: 0.4rem 0.55rem;
}

.metric-row__input input {
  width: 52px;
  border: none;
  background: transparent;
  color: inherit;
  font-weight: 600;
  text-align: right;
}

.metric-row__input input:focus {
  outline: none;
}

.metric-row__input span {
  font-size: 0.8rem;
  color: var(--tg-color-text-soft);
}

.button {
  border-radius: var(--tg-radius-md);
  border: 1px solid rgba(168, 213, 186, 0.25);
  padding: 0.75rem 1.2rem;
  background: rgba(255, 255, 255, 0.03);
  color: inherit;
  font-weight: 600;
  cursor: pointer;
  font-size: 0.95rem;
  transition: transform 0.15s ease, box-shadow 0.15s ease, background 0.15s ease;
}

.button--compact {
  padding: 0.55rem 0.9rem;
  font-size: 0.9rem;
}

.button:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: var(--tg-shadow-soft);
}

.button:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}

.button--primary {
  background: linear-gradient(135deg, var(--tg-color-primary), var(--tg-color-primary-strong));
  color: #ffffff;
  border: none;
}

.button--ghost {
  background: rgba(255, 255, 255, 0.02);
  border: 1px solid rgba(168, 213, 186, 0.25);
  color: var(--tg-color-accent);
}

.button--warning {
  background: rgba(200, 78, 78, 0.12);
  border-color: rgba(200, 78, 78, 0.3);
  color: #f5bcbc;
}

.button--warning:hover:not(:disabled) {
  box-shadow: 0 8px 22px rgba(200, 78, 78, 0.22);
}

.button--active {
  background: rgba(45, 134, 89, 0.32);
  border-color: rgba(45, 134, 89, 0.55);
}

.footer {
  text-align: center;
  color: var(--tg-color-text-soft);
  font-size: 0.85rem;
  border-top: 1px solid var(--tg-color-border);
  padding-top: 0.75rem;
}

.toast {
  position: fixed;
  bottom: 1.75rem;
  right: 1.75rem;
  padding: 0.85rem 1.3rem;
  border-radius: var(--tg-radius-md);
  background: rgba(45, 134, 89, 0.9);
  color: #ffffff;
  font-weight: 600;
  box-shadow: var(--tg-shadow-soft);
  animation: fade-in-out 2.2s ease forwards;
}

@keyframes fade-in-out {
  0% {
    opacity: 0;
    transform: translateY(20px);
  }
  15% {
    opacity: 1;
    transform: translateY(0);
  }
  85% {
    opacity: 1;
    transform: translateY(0);
  }
  100% {
    opacity: 0;
    transform: translateY(20px);
  }
}

@media (max-width: 640px) {
  .app {
    padding: 2.25rem 1.1rem 3rem;
    gap: 1.5rem;
  }

  .with-help::after,
  .with-help::before {
    display: none;
  }

  .top__actions {
    width: 100%;
    align-items: flex-start;
  }

  .top-panel {
    grid-template-columns: 1fr;
    gap: 1rem;
  }

  .summary__value {
    white-space: normal;
  }

  .actions,
  .actions__buttons {
    align-items: stretch;
  }

  .actions__send {
    align-self: stretch;
  }

  .actions__snooze {
    justify-content: flex-start;
  }

  .toast {
    left: 50%;
    right: auto;
    transform: translateX(-50%);
  }
}
</style>
