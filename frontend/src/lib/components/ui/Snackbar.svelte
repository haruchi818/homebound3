<script lang="ts">
  import { createEventDispatcher } from "svelte";

  let { open = false, message = "", tone = "neutral", ...rest } = $props();
  const dispatch = createEventDispatcher();

  function close() {
    dispatch("close");
  }
</script>

{#if open}
  <div class={`snackbar ${tone}`} {...rest}>
    <span>{message}</span>
    <button type="button" onclick={close}>Dismiss</button>
  </div>
{/if}

<style>
  .snackbar {
    position: fixed;
    bottom: 24px;
    right: 24px;
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
    padding: 12px 16px;
    border-radius: 16px;
    display: flex;
    align-items: center;
    gap: 16px;
    box-shadow: var(--md-sys-elevation-2);
    z-index: 50;
  }

  .snackbar.success {
    border-left: 4px solid var(--md-sys-color-success);
  }

  .snackbar.error {
    border-left: 4px solid #c53c30;
  }

  button {
    border: none;
    background: transparent;
    color: inherit;
    cursor: pointer;
    font-weight: 600;
  }
</style>
