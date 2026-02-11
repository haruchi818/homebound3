<script lang="ts">
  import { theme, toggleTheme } from "$lib/stores/theme";

  const isDark = $derived($theme === "dark");
</script>

<button
  class:dark={isDark}
  class="theme-toggle"
  aria-pressed={isDark}
  aria-label={isDark ? "Switch to light mode" : "Switch to dark mode"}
  title={isDark ? "Switch to light mode" : "Switch to dark mode"}
  type="button"
  onclick={toggleTheme}
>
  <span class="track"></span>
  <span class="thumb"></span>
</button>

<style>
  .theme-toggle {
    position: relative;
    width: 54px;
    height: 30px;
    border-radius: 999px;
    border: none;
    background: var(--md-sys-color-surface-container-high);
    cursor: pointer;
    padding: 0;
  }

  .theme-toggle .track {
    position: absolute;
    inset: 4px;
    border-radius: 999px;
    background: var(--md-sys-color-outline);
    opacity: 0.35;
  }

  .theme-toggle .thumb {
    position: absolute;
    top: 4px;
    left: 4px;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--md-sys-color-on-primary);
    transition: transform 0.2s ease;
    box-shadow: 0 4px 10px rgba(0, 0, 0, 0.2);
  }

  .theme-toggle.dark {
    background: #1f1f1f;
  }

  .theme-toggle.dark .thumb {
    transform: translateX(24px);
    background: var(--md-sys-color-primary);
  }
</style>
