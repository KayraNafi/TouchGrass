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
    return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }

  $effect(() => {
    if (preferences) {
      document.documentElement.dataset.theme = preferences.theme;
    }
  });
</script>

<main class="app">
  <header class="heading">
    <span class="heading__pill">TouchGrass</span>
    <h1>Break reminders that fight burnout.</h1>
    <p class="heading__sub">Tiny, offline nudges so you actually go touch grass.</p>
  </header>

  <section class="status-grid">
    <article class="status-card">
      <div class="status-card__header">
        <h2>Next reminder</h2>
      </div>
      <p class="status-card__value">{formatNextLabel()}</p>
      {#if status?.idleSeconds !== null}
        <span class="status-chip">{formatIdleLabel()}</span>
      {/if}
    </article>

    <article class="status-card">
      <div class="status-card__header">
        <h2>Last break</h2>
      </div>
      <p class="status-card__value">{formatLastLabel()}</p>
      <button
        type="button"
        class="button button--ghost"
        onclick={triggerPreview}
        disabled={pending || isLoading}
      >
        Send test reminder
      </button>
    </article>

    {#if lastReminderMessage}
      <article class="status-card status-card--highlight">
        <div class="status-card__header">
          <h2>Latest nudge</h2>
          {#if lastReminderAt}
            <time>{formatClock(lastReminderAt)}</time>
          {/if}
        </div>
        <p class="status-card__value status-card__value--smaller">
          {lastReminderMessage}
        </p>
      </article>
    {/if}
  </section>

  <section class="panel">
    <div class="panel__header">
      <h2>Reminder cadence</h2>
      <p>Select how often TouchGrass nudges you.</p>
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

  <section class="panel">
    <div class="panel__header">
      <h2>Preferences</h2>
      <p>Tune how TouchGrass behaves on your machine.</p>
    </div>
    <div class="toggle-list">
      <label class="toggle">
        <span class="toggle__copy">
          <strong>Activity detection</strong>
          <small>Skip reminders when you have been idle for ~2 minutes.</small>
        </span>
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

      <label class="toggle">
        <span class="toggle__copy">
          <strong>Chime when reminding</strong>
          <small>Play a soft tone alongside notifications.</small>
        </span>
        <input
          type="checkbox"
          checked={preferences?.soundEnabled}
          onchange={(event) => toggleSound(event.currentTarget.checked)}
          disabled={pending || isLoading}
        />
        <span class="toggle__visual" data-active={preferences?.soundEnabled}></span>
      </label>

      <div class="metric-row">
        <span class="metric-row__copy">
          <strong>Break resets after</strong>
          <small>When you’re idle this many minutes, the timer restarts once you return.</small>
        </span>
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

      <label class="toggle">
        <span class="toggle__copy">
          <strong>Launch on login</strong>
          <small>Start TouchGrass quietly whenever you sign in.</small>
        </span>
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

      <div class="theme-toggle">
        <span class="theme-toggle__copy">
          <strong>Theme</strong>
          <small>Dark is default. Light is here if you must.</small>
        </span>
        <div class="theme-toggle__buttons">
          <button
            type="button"
            class="button"
            class:button--active={preferences?.theme === "dark"}
            onclick={() => setTheme("dark")}
            disabled={pending || isLoading}
          >
            Dark
          </button>
          <button
            type="button"
            class="button"
            class:button--active={preferences?.theme === "light"}
            onclick={() => setTheme("light")}
            disabled={pending || isLoading}
          >
            Light
          </button>
        </div>
      </div>
    </div>
  </section>

  <section class="panel">
    <div class="panel__header">
      <h2>Quick controls</h2>
      <p>Need a breather right now? Use these.</p>
    </div>
    <div class="quick-row">
      <button
        type="button"
        class="button button--primary"
        onclick={togglePause}
        disabled={pending || isLoading}
      >
        {status?.paused ? "Resume reminders" : "Pause reminders"}
      </button>
      <button
        type="button"
        class="button"
        onclick={() => snooze(5)}
        disabled={pending || isLoading}
      >
        Snooze 5 min
      </button>
      <button
        type="button"
        class="button"
        onclick={() => snooze(15)}
        disabled={pending || isLoading}
      >
        Snooze 15 min
      </button>
    </div>
  </section>

  <footer class="footer">
    <p>“Go touch grass.” Tiny, offline nudges that help you step away.</p>
  </footer>

  {#if toastMessage}
    <div class="toast">{toastMessage}</div>
  {/if}
</main>

<style>
.app {
  display: flex;
  flex-direction: column;
  gap: 2.5rem;
  padding: 3.5rem clamp(1.5rem, 5vw, 4rem) 4rem;
  max-width: 960px;
  margin: 0 auto;
}

.heading {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.heading__pill {
  align-self: flex-start;
  padding: 0.35rem 1rem;
  border-radius: 999px;
  background: rgba(45, 134, 89, 0.2);
  color: var(--tg-color-accent);
  font-size: 0.85rem;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.heading h1 {
  margin: 0;
  font-size: clamp(2.4rem, 4vw, 3rem);
  line-height: 1.1;
}

.heading__sub {
  margin: 0;
  font-size: 1.05rem;
  color: var(--tg-color-text-soft);
}

.status-grid {
  display: grid;
  gap: 1.5rem;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
}

.status-card {
  background: var(--tg-color-surface);
  border: 1px solid var(--tg-color-border);
  border-radius: var(--tg-radius-lg);
  padding: 1.75rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
  box-shadow: var(--tg-shadow-subtle);
}

.status-card--highlight {
  border-color: rgba(45, 134, 89, 0.45);
  box-shadow: 0 18px 45px rgba(45, 134, 89, 0.25);
  background: rgba(45, 134, 89, 0.12);
}

.status-card__header {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 0.5rem;
}

.status-card__header h2 {
  margin: 0;
  font-size: 1.1rem;
  letter-spacing: 0.01em;
}

.status-card__header time {
  font-size: 0.85rem;
  color: var(--tg-color-text-soft);
}

.status-card__value {
  margin: 0;
  font-size: 1.4rem;
  font-weight: 600;
}

.status-card__value--smaller {
  font-size: 1.15rem;
  font-weight: 500;
}

.status-chip {
  align-self: flex-start;
  padding: 0.25rem 0.75rem;
  border-radius: 999px;
  background: rgba(168, 213, 186, 0.18);
  color: var(--tg-color-accent);
  font-size: 0.85rem;
}

.panel {
  background: var(--tg-color-surface);
  border: 1px solid var(--tg-color-border);
  border-radius: var(--tg-radius-lg);
  padding: 2rem;
  display: flex;
  flex-direction: column;
  gap: 1.75rem;
  box-shadow: var(--tg-shadow-subtle);
}

.panel__header h2 {
  margin: 0;
  font-size: 1.2rem;
}

.panel__header p {
  margin: 0.35rem 0 0;
  color: var(--tg-color-text-soft);
  font-size: 0.95rem;
}

.interval-group {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
}

.interval {
  border-radius: var(--tg-radius-md);
  border: 1px solid rgba(168, 213, 186, 0.25);
  padding: 0.85rem 1.2rem;
  background: rgba(255, 255, 255, 0.03);
  color: var(--tg-color-text);
  font-weight: 600;
  cursor: pointer;
  transition: transform 0.15s ease, box-shadow 0.15s ease, border 0.15s ease;
}

.interval:hover {
  transform: translateY(-1px);
  box-shadow: var(--tg-shadow-soft);
}

.interval--active {
  border-color: rgba(45, 134, 89, 0.65);
  background: rgba(45, 134, 89, 0.28);
}

.interval--custom {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  padding-right: 1rem;
}

.interval--custom label {
  font-size: 0.9rem;
  color: var(--tg-color-text-soft);
}

.interval--custom input {
  width: 70px;
  border-radius: var(--tg-radius-md);
  border: 1px solid rgba(45, 134, 89, 0.35);
  padding: 0.65rem 0.75rem;
  background: rgba(255, 255, 255, 0.04);
  color: inherit;
}

.toggle-list {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.toggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding: 1rem 1.25rem;
  border-radius: var(--tg-radius-md);
  border: 1px solid rgba(168, 213, 186, 0.25);
  background: rgba(255, 255, 255, 0.02);
}

.toggle__copy {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.toggle__copy strong {
  font-size: 1rem;
}

.toggle__copy small {
  font-size: 0.85rem;
  color: var(--tg-color-text-soft);
}

.toggle input {
  position: absolute;
  opacity: 0;
  pointer-events: none;
}

.toggle__visual {
  position: relative;
  width: 46px;
  height: 26px;
  border-radius: 999px;
  background: rgba(168, 213, 186, 0.2);
  transition: background 0.2s ease;
}

.toggle__visual::after {
  content: "";
  position: absolute;
  top: 3px;
  left: 3px;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--tg-color-text);
  transition: transform 0.2s ease, background 0.2s ease;
}

.toggle__visual[data-active="true"] {
  background: rgba(45, 134, 89, 0.7);
}

.toggle__visual[data-active="true"]::after {
  transform: translateX(20px);
  background: var(--tg-color-bg);
}

.metric-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding: 1rem 1.25rem;
  border-radius: var(--tg-radius-md);
  border: 1px solid rgba(168, 213, 186, 0.25);
  background: rgba(255, 255, 255, 0.02);
}

.metric-row__copy {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.metric-row__copy strong {
  font-size: 1rem;
}

.metric-row__copy small {
  font-size: 0.85rem;
  color: var(--tg-color-text-soft);
}

.metric-row__input {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(45, 134, 89, 0.35);
  border-radius: var(--tg-radius-md);
  padding: 0.45rem 0.65rem;
}

.metric-row__input input {
  width: 60px;
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
  font-size: 0.85rem;
  color: var(--tg-color-text-soft);
}

.theme-toggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding: 1rem 1.25rem;
  border-radius: var(--tg-radius-md);
  border: 1px solid rgba(168, 213, 186, 0.25);
  background: rgba(255, 255, 255, 0.02);
}

.theme-toggle__copy {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.theme-toggle__copy strong {
  font-size: 1rem;
}

.theme-toggle__copy small {
  color: var(--tg-color-text-soft);
  font-size: 0.85rem;
}

.theme-toggle__buttons {
  display: flex;
  gap: 0.5rem;
}

.quick-row {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
}

.button {
  border-radius: var(--tg-radius-md);
  border: 1px solid rgba(168, 213, 186, 0.28);
  padding: 0.85rem 1.4rem;
  background: rgba(255, 255, 255, 0.03);
  color: inherit;
  font-weight: 600;
  cursor: pointer;
  transition: transform 0.15s ease, box-shadow 0.15s ease, background 0.15s ease;
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
  background: linear-gradient(135deg, var(--tg-color-primary), #35a269);
  color: #ffffff;
  border: none;
}

.button--ghost {
  background: none;
  border: 1px solid rgba(168, 213, 186, 0.25);
  color: var(--tg-color-accent);
}

.button--active {
  background: rgba(45, 134, 89, 0.32);
  border-color: rgba(45, 134, 89, 0.55);
}

.footer {
  text-align: center;
  color: var(--tg-color-text-soft);
  font-size: 0.9rem;
  border-top: 1px solid var(--tg-color-border);
  padding-top: 1rem;
}

.toast {
  position: fixed;
  bottom: 2rem;
  right: 2rem;
  padding: 0.9rem 1.4rem;
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
    padding: 2.8rem 1.25rem 3.5rem;
    gap: 2rem;
  }

  .panel {
    padding: 1.5rem;
  }

  .toast {
    left: 50%;
    right: auto;
    transform: translateX(-50%);
  }
}
</style>
