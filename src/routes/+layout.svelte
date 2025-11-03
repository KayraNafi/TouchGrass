<script lang="ts">
  import { onMount } from "svelte";
  import "../app.css";

  const defaultTheme = "dark";

  onMount(() => {
    const root = document.documentElement;
    if (!root.dataset.theme) {
      root.dataset.theme = defaultTheme;
    }
  });
</script>

<svelte:head>
  <title>TouchGrass</title>
  <meta name="color-scheme" content="dark light" />
  <meta
    name="description"
    content="TouchGrass nudges you to step away with playful, customizable break reminders."
  />
</svelte:head>

<div class="app-shell">
  <slot />
</div>
