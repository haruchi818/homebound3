<script lang="ts">
  let { value = 0, label = "", ...rest } = $props();
  const clamped = $derived(() => Math.max(0, Math.min(100, value)));
</script>

<div class="progress" {...rest}>
  <div class="track">
    <div class="fill" style={`width: ${clamped}%`}></div>
  </div>
  {#if label}
    <span class="label">{label}</span>
  {/if}
</div>

<style>
  .progress {
    display: grid;
    gap: 8px;
  }

  .track {
    height: 8px;
    background: var(--md-sys-color-surface-variant);
    border-radius: 999px;
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--md-sys-color-primary);
    border-radius: 999px;
  }

  .label {
    font-size: var(--md-sys-typescale-label-medium);
    color: var(--md-sys-color-on-surface-variant);
  }
</style>
