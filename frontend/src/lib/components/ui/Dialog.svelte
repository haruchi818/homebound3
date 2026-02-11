<script lang="ts">
  import { createEventDispatcher } from "svelte";

  let { open = false, title = "", ...rest } = $props();
  const dispatch = createEventDispatcher();

  function close() {
    dispatch("close");
  }
</script>

{#if open}
  <button class="backdrop" type="button" aria-label="Close dialog" onclick={close}></button>
  <section class="dialog" role="dialog" aria-modal="true" {...rest}>
    <header>
      <h3>{title}</h3>
      <button class="icon" type="button" onclick={close}>×</button>
    </header>
    <div class="content">
      <slot />
    </div>
    <footer>
      <slot name="actions" />
    </footer>
  </section>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.3);
    z-index: 30;
  }

  .dialog {
    position: fixed;
    inset: 0;
    margin: auto;
    width: min(520px, 90vw);
    background: var(--md-sys-color-surface-container);
    border-radius: 20px;
    box-shadow: var(--md-sys-elevation-3);
    padding: 18px;
    z-index: 40;
    display: grid;
    gap: 16px;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  h3 {
    margin: 0;
    font-size: var(--md-sys-typescale-headline-small);
  }

  .icon {
    border: none;
    background: transparent;
    font-size: 1.2rem;
    cursor: pointer;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 12px;
  }
</style>
