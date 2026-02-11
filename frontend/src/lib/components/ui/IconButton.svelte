<script lang="ts">
  let { label = "", type = "button", variant = "surface", ...rest } = $props();
</script>

<button class={`icon-button ${variant}`} type={type} aria-label={label} {...rest}>
  <slot />
</button>

<style>
  .icon-button {
    border: none;
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
    padding: 10px 12px;
    border-radius: 14px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  .icon-button.primary {
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }

  .icon-button.ghost {
    background: transparent;
    color: var(--md-sys-color-on-surface);
  }
</style>
