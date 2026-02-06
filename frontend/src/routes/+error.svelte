<script lang="ts">
  export let error: Error | null = null;
  export let status: number = 500;
</script>

<div class="page">
  <div class="error-shell surface">
    <p class="eyebrow">Something went wrong</p>
    <h1>{status}</h1>
    <p class="message">{error?.message ?? "Unexpected error"}</p>
    <a class="action-btn" href="/">Back to login</a>
  </div>
</div>

<style>
  .error-shell {
    margin: auto;
    padding: 36px;
    width: min(520px, 90vw);
    display: grid;
    gap: 12px;
    text-align: center;
  }

  .eyebrow {
    text-transform: uppercase;
    letter-spacing: 0.3em;
    font-size: 0.7rem;
    color: var(--md-sys-color-secondary);
    margin: 0;
  }

  h1 {
    font-size: 3rem;
    margin: 0;
  }

  .message {
    margin: 0;
    color: var(--md-sys-color-on-surface-variant);
  }

  a.action-btn {
    text-decoration: none;
    justify-self: center;
  }
</style>
