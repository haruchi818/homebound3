<script lang="ts">
  let { label = "", tone = "neutral", ...rest } = $props();
</script>

<span class={`badge ${tone}`} {...rest}>{label}</span>

<style>
  .badge {
    padding: 4px 10px;
    border-radius: 999px;
    font-size: var(--md-sys-typescale-label-small);
    font-weight: 600;
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
  }

  .badge.live {
    background: rgba(46, 125, 50, 0.18);
    color: var(--md-sys-color-success);
  }

  .badge.processing {
    background: rgba(239, 108, 0, 0.16);
    color: var(--md-sys-color-warning);
  }

  .badge.ready {
    background: rgba(45, 91, 117, 0.16);
    color: var(--md-sys-color-primary);
  }

  .badge.failed {
    background: rgba(197, 60, 48, 0.16);
    color: #c53c30;
  }
</style>
