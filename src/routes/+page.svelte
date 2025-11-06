<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { check, type Update } from "@tauri-apps/plugin-updater";

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
  let toastTimeout: ReturnType<typeof setTimeout> | null = null;
  let updateChecking = $state(false);
  let updateInstalling = $state(false);
  let updateAvailable = $state(false);
  let updateInfo = $state<{ version: string; notes?: string | null } | null>(null);
  let pendingUpdate: Update | null = null;

  const LIGHT_MODE_WARNING_KEY = "touchgrass.lightModeWarningAcknowledged";
  const LIGHT_MODE_CONFIRM_MESSAGES = [
    "Light mode? Bold choice, sunflower.",
    "Turning the sun on indoors, are we?",
    "Sure, let’s blind the owls. Light mode engaged.",
    "Dark mode is hurt but it understands.",
    "Light mode it is. Sunglasses optional.",
    "Engaging photon cannon… ☀️",
    "Welcome to the bright side.",
    "Brightness: 100%. Subtlety: 0%.",
    "Alert: vampires have left the chat.",
    "Okay, lighthouse. Light mode on.",
    "SPF 50 applied. Proceed.",
    "Hope your retinas stretched first.",
    "Your screen, your rules.",
    "Plants are thrilled. 🌱",
    "Sunrise achieved.",
  ] as const;
  const SNOOZE_MESSAGES = [
    (minutes: number) => `Snoozed ${minutes}m. Don't ghost me.`,
    (_minutes: number) => "Okay, future-you will handle it.",
  ] as const;
  let lightModeWarningAcknowledged = $state(false);

  let audioContext: AudioContext | null = null;
  let unlistenStatus: UnlistenFn | null = null;
  let unlistenReminder: UnlistenFn | null = null;
  let realtimeUpdateInterval: ReturnType<typeof setInterval> | null = null;
  let realtimeTick = $state(0); // Used to trigger reactive updates for time displays

  onMount(async () => {
    // Update every 10 seconds to refresh relative times
    realtimeUpdateInterval = setInterval(() => {
      realtimeTick++;
    }, 10000);
    if (typeof window !== "undefined") {
      const stored = window.localStorage.getItem(LIGHT_MODE_WARNING_KEY);
      lightModeWarningAcknowledged = stored === "true";
    }
    await loadInitial();
    if (preferences?.theme === "light" && !lightModeWarningAcknowledged) {
      markLightModeWarningAcknowledged();
    }
    await setupListeners();
    await checkForUpdates();
  });

  onDestroy(() => {
    unlistenStatus?.();
    unlistenReminder?.();
    if (toastTimeout) {
      clearTimeout(toastTimeout);
    }
    if (realtimeUpdateInterval) {
      clearInterval(realtimeUpdateInterval);
    }
    if (pendingUpdate) {
      pendingUpdate.close().catch((error) => {
        console.warn("TouchGrass: failed to dispose update handle", error);
      });
      pendingUpdate = null;
    }
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

  function markLightModeWarningAcknowledged() {
    lightModeWarningAcknowledged = true;
    if (typeof window !== "undefined") {
      window.localStorage.setItem(LIGHT_MODE_WARNING_KEY, "true");
    }
  }

  function randomLightModeMessage(): string {
    const index = Math.floor(Math.random() * LIGHT_MODE_CONFIRM_MESSAGES.length);
    return LIGHT_MODE_CONFIRM_MESSAGES[index];
  }

  function randomSnoozeMessage(minutes: number): string {
    const index = Math.floor(Math.random() * SNOOZE_MESSAGES.length);
    return SNOOZE_MESSAGES[index](minutes);
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

    // Build a layered chime with three partials and a soft echo tail.
    const outputGain = ctx.createGain();
    outputGain.gain.setValueAtTime(0.0001, now);
    outputGain.gain.linearRampToValueAtTime(0.55, now + 0.05);
    outputGain.gain.exponentialRampToValueAtTime(0.0001, now + 2.2);

    const dryGain = ctx.createGain();
    dryGain.gain.setValueAtTime(0.85, now);
    outputGain.connect(dryGain).connect(ctx.destination);

    const wetGain = ctx.createGain();
    wetGain.gain.setValueAtTime(0.32, now);

    const delay = ctx.createDelay(1.5);
    delay.delayTime.setValueAtTime(0.24, now);

    const delayFilter = ctx.createBiquadFilter();
    delayFilter.type = "lowpass";
    delayFilter.frequency.setValueAtTime(3200, now);

    const delayFeedback = ctx.createGain();
    delayFeedback.gain.setValueAtTime(0.28, now);

    outputGain.connect(delay);
    delay.connect(delayFilter);
    delayFilter.connect(delayFeedback);
    delayFeedback.connect(delay);
    delay.connect(wetGain).connect(ctx.destination);

    type ToneSpec = {
      frequency: number;
      startOffset: number;
      duration: number;
      peakGain: number;
      type?: OscillatorType;
    };

    const tones: ToneSpec[] = [
      { frequency: 659.25, startOffset: 0, duration: 1.8, peakGain: 0.22, type: "triangle" },
      { frequency: 880, startOffset: 0.05, duration: 1.6, peakGain: 0.18, type: "sine" },
      { frequency: 1174.66, startOffset: 0.12, duration: 1.2, peakGain: 0.12, type: "sine" },
    ];

    const releaseDuration =
      Math.max(...tones.map((tone) => tone.startOffset + tone.duration)) + 0.8;

    tones.forEach((tone) => {
      const startTime = now + tone.startOffset;
      const stopTime = startTime + tone.duration + 0.25;

      const osc = ctx.createOscillator();
      osc.type = tone.type ?? "sine";
      osc.frequency.setValueAtTime(tone.frequency * 0.985, startTime);
      osc.frequency.exponentialRampToValueAtTime(
        tone.frequency,
        startTime + 0.35,
      );

      const vibrato = ctx.createOscillator();
      vibrato.type = "sine";
      vibrato.frequency.setValueAtTime(4.5, startTime);
      const vibratoDepth = ctx.createGain();
      vibratoDepth.gain.setValueAtTime(10, startTime); // cents
      vibrato.connect(vibratoDepth).connect(osc.detune);

      const toneGain = ctx.createGain();
      toneGain.gain.setValueAtTime(0.0001, startTime);
      toneGain.gain.exponentialRampToValueAtTime(
        tone.peakGain,
        startTime + 0.06,
      );
      toneGain.gain.exponentialRampToValueAtTime(
        0.0001,
        startTime + tone.duration,
      );

      osc.connect(toneGain).connect(outputGain);

      osc.start(startTime);
      vibrato.start(startTime);
      osc.stop(stopTime);
      vibrato.stop(stopTime);
      osc.onended = () => {
        osc.disconnect();
        toneGain.disconnect();
      };
      vibrato.onended = () => {
        vibrato.disconnect();
        vibratoDepth.disconnect();
      };
    });

    if (typeof window !== "undefined") {
      window.setTimeout(() => {
        outputGain.disconnect();
        dryGain.disconnect();
        wetGain.disconnect();
        delay.disconnect();
        delayFilter.disconnect();
        delayFeedback.disconnect();
      }, releaseDuration * 1000);
    }
  }

  function handleReminder(payload: ReminderEvent) {
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
    const clamped = Math.min(Math.max(value, 2), 240);
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

  async function handleThemeToggle(event: Event) {
    if (!preferences) return;
    const input = event.currentTarget as HTMLInputElement | null;
    if (!input) return;
    const revertChecked = preferences.theme !== "light";
    const nextTheme = input.checked ? "dark" : "light";

    if (nextTheme === "light") {
      const confirmed = window.confirm(randomLightModeMessage());
      if (!confirmed) {
        input.checked = revertChecked;
        return;
      }
      if (!lightModeWarningAcknowledged) {
        markLightModeWarningAcknowledged();
      }
    }

    input.checked = nextTheme !== "light";
    await setTheme(nextTheme);
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
      showToast(
        next
          ? "Noted. I’ll try again when you’re less dramatic."
          : "Reminders resumed. Chair missed you (a little).",
      );
    } catch (error) {
      console.error("TouchGrass: failed to toggle pause", error);
      showToast("Could not pause reminders");
    }
  }

  async function snooze(minutes: number) {
    try {
      await invoke<void>("snooze_for_minutes", { minutes });
      showToast(randomSnoozeMessage(minutes));
    } catch (error) {
      console.error("TouchGrass: failed to snooze", error);
      showToast("Could not snooze reminders");
    }
  }

  async function clearSnooze() {
    try {
      await invoke<void>("clear_snooze");
      showToast("Snooze cancelled. Chair missed you (a little).");
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

  async function checkForUpdates(manual = false) {
    if (updateChecking) return;
    updateChecking = true;
    try {
      if (typeof window === "undefined" || !(window as { __TAURI__?: unknown }).__TAURI__) {
        updateChecking = false;
        return;
      }
      const update = await check();
      if (!update) {
        if (manual) {
          showToast("You're already on the latest version");
        }
        if (pendingUpdate) {
          await pendingUpdate.close();
          pendingUpdate = null;
        }
        updateAvailable = false;
        updateInfo = null;
        return;
      }

      if (pendingUpdate) {
        await pendingUpdate.close().catch((error) => {
          console.warn("TouchGrass: failed to close stale update handle", error);
        });
      }

      pendingUpdate = update;
      updateAvailable = true;
      updateInfo = { version: update.version, notes: update.body ?? null };

      if (!manual) {
        showToast(`Update ${update.version} is ready to install`);
      }
    } catch (error) {
      console.error("TouchGrass: update check failed", error);
      if (manual) {
        showToast("Could not check for updates");
      }
    } finally {
      updateChecking = false;
    }
  }

  async function installPendingUpdate() {
    if (!pendingUpdate || updateInstalling) return;
    updateInstalling = true;
    try {
      await pendingUpdate.downloadAndInstall();
      showToast("Update installed. Restarting…");
      await relaunch();
    } catch (error) {
      console.error("TouchGrass: failed to install update", error);
      showToast("Update could not be installed");
    } finally {
      updateInstalling = false;
      if (pendingUpdate) {
        pendingUpdate.close().catch((err) => {
          console.warn("TouchGrass: failed to close update handle after install", err);
        });
        pendingUpdate = null;
      }
    }
  }

  function showToast(message: string) {
    toastMessage = message;
    if (toastTimeout) {
      clearTimeout(toastTimeout);
    }
    toastTimeout = setTimeout(() => {
      toastMessage = null;
      toastTimeout = null;
    }, 2200);
  }

  function formatNextLabel(): string {
    // Depend on realtimeTick for live updates
    void realtimeTick;

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
    // Depend on realtimeTick for live updates
    void realtimeTick;

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
          onchange={handleThemeToggle}
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

      <div class="top__update">
        {#if updateAvailable && updateInfo}
          <div class="update-banner">
            <div class="update-banner__info">
              <span class="update-banner__label">Update ready</span>
              <strong>v{updateInfo.version}</strong>
            </div>
            <button
              type="button"
              class="button button--primary button--compact"
              onclick={installPendingUpdate}
              disabled={updateInstalling}
            >
              {updateInstalling ? "Installing…" : "Install now"}
            </button>
          </div>
        {:else}
          <button
            type="button"
            class="button button--ghost button--compact top__update-check"
            onclick={() => checkForUpdates(true)}
            disabled={updateChecking}
          >
            {updateChecking ? "Checking…" : "Check for updates"}
          </button>
        {/if}
      </div>
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
            min="2"
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

        <div
          class="metric-row with-help"
          data-help="When you’re idle at least this long, the timer restarts once you return. Activity detection must be on."
          data-disabled={!preferences?.activityDetection}
        >
          <span class="metric-row__label">Break resets after</span>
          <div class="metric-row__input">
            <input
              type="number"
              min="1"
              max="30"
              value={preferences?.idleThresholdMinutes ?? 2}
              onchange={(event) => setIdleThreshold(parseInt(event.currentTarget.value, 10))}
              disabled={pending || isLoading || !preferences?.activityDetection}
            />
            <span>min</span>
          </div>
        </div>

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

.top__update {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 0.5rem;
}

.top__update-check {
  align-self: flex-end;
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
  background: var(--tg-color-switch-track);
  transition: background 0.2s ease;
}

.theme-switch__thumb {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--tg-color-switch-thumb);
  transition: transform 0.2s ease, background 0.2s ease;
}

.theme-switch__track[data-active="true"] {
  background: var(--tg-color-primary);
}

.theme-switch__track[data-active="true"] .theme-switch__thumb {
  transform: translateX(18px);
  background: var(--tg-color-switch-thumb-active);
}

.theme-switch__caption {
  font-weight: 600;
}

.update-banner {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.6rem 0.75rem;
  border-radius: var(--tg-radius-md);
  border: 1px solid var(--tg-color-interactive-border);
  background: var(--tg-color-surface-muted);
}

.update-banner__info {
  display: flex;
  flex-direction: column;
  gap: 0.05rem;
  text-align: left;
}

.update-banner__label {
  font-size: 0.72rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--tg-color-text-soft);
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
  background: var(--tg-color-surface-muted);
  border: 1px solid var(--tg-color-border-soft);
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
  border: 1px solid var(--tg-color-interactive-border);
  padding: 0.6rem 1.05rem;
  background: var(--tg-color-ghost-bg);
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
  background: var(--tg-color-ghost-bg);
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
  background: var(--tg-color-tooltip-bg);
  border: 1px solid var(--tg-color-tooltip-border);
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
  background: var(--tg-color-tooltip-bg);
  border-left: 1px solid var(--tg-color-tooltip-border);
  border-top: 1px solid var(--tg-color-tooltip-border);
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
  border: 1px solid var(--tg-color-border-strong);
  background: var(--tg-color-surface-elevated);
  box-shadow: inset 0 0 0 1px var(--tg-color-toggle-inner-shadow);
  cursor: pointer;
}

.toggle:focus-within {
  outline: 2px solid var(--tg-color-outline-focus);
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
  background: var(--tg-color-toggle-track);
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
  background: var(--tg-color-toggle-thumb);
  transition: transform 0.2s ease, background 0.2s ease;
}

.toggle__visual[data-active="true"] {
  background: var(--tg-color-toggle-active);
}

.toggle__visual[data-active="true"]::after {
  transform: translateX(20px);
  background: var(--tg-color-toggle-thumb-active);
}

.metric-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  padding: 0.85rem 1.1rem;
  border-radius: var(--tg-radius-md);
  border: 1px solid var(--tg-color-border-strong);
  background: var(--tg-color-surface-elevated);
  box-shadow: inset 0 0 0 1px var(--tg-color-toggle-inner-shadow);
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
  background: var(--tg-color-ghost-bg);
  border: 1px solid var(--tg-color-interactive-border);
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

.metric-row[data-disabled="true"] .metric-row__label,
.metric-row[data-disabled="true"] .metric-row__input,
.metric-row[data-disabled="true"] .metric-row__input span {
  opacity: 0.6;
}

.button {
  border-radius: var(--tg-radius-md);
  border: 1px solid var(--tg-color-interactive-border);
  padding: 0.75rem 1.2rem;
  background: var(--tg-color-ghost-bg);
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
  background: var(--tg-color-ghost-bg);
  border: 1px solid var(--tg-color-interactive-border);
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

  .top__update {
    width: 100%;
    align-items: stretch;
  }

  .update-banner {
    justify-content: space-between;
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
