<script lang="ts">
  let { variant = "primary", type = "button", ...rest } = $props();
</script>

<button
  class:secondary={variant === "secondary"}
  class:dark={variant === "dark"}
  type={type}
  {...rest}
>
  <slot />
</button>

<style>
  button {
    border: none;
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
    padding: 12px 18px;
    border-radius: 16px;
    font-weight: 600;
    cursor: pointer;
  }

  button.secondary {
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
  }

  button.dark {
    background: #1f1f1f;
    color: #f4f1ec;
  }
</style>
