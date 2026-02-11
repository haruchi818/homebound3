<script lang="ts">
  let { name = "", src = "", size = 40, ...rest } = $props();

  const initials = (value: string) => {
    if (!value) return "U";
    return value
      .split(" ")
      .map((part) => part[0])
      .slice(0, 2)
      .join("")
      .toUpperCase();
  };
</script>

<div
  class="avatar"
  style={`--size: ${size}px`}
  role="img"
  aria-label={name || "User avatar"}
  {...rest}
>
  {#if src}
    <img src={src} alt={name || "User avatar"} />
  {:else}
    <span>{initials(name)}</span>
  {/if}
</div>

<style>
  .avatar {
    width: var(--size);
    height: var(--size);
    border-radius: 50%;
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
    display: grid;
    place-items: center;
    font-weight: 700;
    overflow: hidden;
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
