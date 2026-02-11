<script lang="ts">
  let { padded = true, elevated = false, ...rest } = $props();
</script>

<div class:pad={padded} class:elevated {...rest}>
  <slot />
</div>

<style>
  div {
    background: var(--md-sys-color-surface-container);
    border-radius: 22px;
  }

  .pad {
    padding: 18px;
  }

  .elevated {
    box-shadow: var(--md-sys-elevation-2);
  }
</style>
