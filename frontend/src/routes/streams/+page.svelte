<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { refreshSession, signOut, user } from "$lib/stores/auth";
  import ThemeToggle from "$lib/components/ui/ThemeToggle.svelte";
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
  import { fetchJson } from "$lib/api";
  import Avatar from "$lib/components/ui/Avatar.svelte";
  import Badge from "$lib/components/ui/Badge.svelte";
  import Card from "$lib/components/ui/Card.svelte";
  import Snackbar from "$lib/components/ui/Snackbar.svelte";

  const currentUser = $derived($user);

  let chatOpen = $state(false);
  let showMenu = $state(false);

  type StreamRow = {
    streamId: string;
    userId: string;
    status: string;
    createdAt: string;
    endedAt?: string | null;
    currentMovieId?: string | null;
    currentTimestamp: number;
    isPlaying: boolean;
    streamType: string;
    viewerCount: number;
  };

  let streams = $state<StreamRow[]>([]);
  let refreshTimer: ReturnType<typeof setInterval> | null = null;
  let snackbarOpen = $state(false);
  let snackbarMessage = $state("");
  let snackbarTone = $state<"neutral" | "success" | "error">("neutral");

  onMount(async () => {
    const session = await refreshSession();
    if (!session) {
      goto("/");
      return;
    }

    await loadStreams();
    refreshTimer = setInterval(loadStreams, 10000);
  });

  onDestroy(() => {
    if (refreshTimer) clearInterval(refreshTimer);
  });

  async function handleLogout() {
    await signOut();
    goto("/");
  }

  function openStream(streamId: string) {
    goto(`/stream/${streamId}`);
  }

  async function loadStreams() {
    try {
      streams = await fetchJson<StreamRow[]>("/api/streams");
    } catch (error) {
      snackbarMessage = "Unable to load live streams.";
      snackbarTone = "error";
      snackbarOpen = true;
    }
  }
</script>

<div class="streams-page">
  <header class="top-bar surface">
    <div>
      <p class="eyebrow">Watch Together</p>
      <h2>Live streams</h2>
    </div>
    <div class="top-actions">
      <button class="action-btn" type="button" onclick={() => goto("/stream/admin")}
        >Start Stream</button
      >
      <div class="avatar-area">
        <button class="avatar-btn" onclick={() => (showMenu = !showMenu)} type="button">
          <Avatar
            name={currentUser?.displayName ?? ""}
            src={currentUser?.avatarUrl ?? ""}
            size={44}
          />
        </button>
        {#if showMenu}
          <div class="menu surface">
            <p class="menu-title">Signed in</p>
            <p class="menu-name">{currentUser?.displayName}</p>
            <button class="action-btn secondary" onclick={handleLogout} type="button">
              Logout
            </button>
          </div>
        {/if}
      </div>
    </div>
  </header>

  <main class:chat-open={chatOpen}>
    <Card class="streams" elevated>
      <div class="streams-head">
        <h3 class="section-title">Join a room</h3>
        <p class="hint">Select a live stream to jump in.</p>
      </div>
      <div class="streams-grid">
        {#if streams.length === 0}
          <div class="empty-state">
            <p>No live streams yet. Start one from the top bar.</p>
          </div>
        {:else}
          {#each streams as stream}
          <button class="stream-card" type="button" onclick={() => openStream(stream.streamId)}>
            <span class="stream-title">{stream.streamId}</span>
            <span class="stream-meta">{stream.viewerCount} watching</span>
            <Badge label={stream.streamType} tone={stream.streamType === "camera" ? "live" : "ready"} />
          </button>
          {/each}
        {/if}
      </div>
    </Card>

    <ChatPanel open={chatOpen} />
  </main>

  <footer class="bottom-bar surface">
    <div class="bottom-actions">
      <button class="action-btn" type="button" onclick={() => (chatOpen = !chatOpen)}>
        {chatOpen ? "Close Chat" : "Chat"}
      </button>
      <ThemeToggle />
    </div>
  </footer>
</div>

<Snackbar
  open={snackbarOpen}
  message={snackbarMessage}
  tone={snackbarTone}
  on:close={() => (snackbarOpen = false)}
/>

<style>
  .streams-page {
    min-height: 100vh;
    height: 100vh;
    height: 100dvh;
    display: grid;
    grid-template-rows: 15vh minmax(0, 1fr) 10vh;
    padding: 2vh 3vw 3vh;
    gap: 2vh;
    overflow: hidden;
  }

  .top-bar,
  .bottom-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 24px;
  }

  .top-bar h2 {
    margin: 0;
    font-size: 1.8rem;
  }

  .eyebrow {
    text-transform: uppercase;
    letter-spacing: 0.3em;
    font-size: 0.75rem;
    color: var(--md-sys-color-secondary);
    margin: 0 0 6px 0;
  }

  .top-actions {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .avatar-area {
    position: relative;
  }

  .avatar-btn {
    border: none;
    background: transparent;
    padding: 0;
    cursor: pointer;
  }

  .menu {
    position: absolute;
    right: 0;
    top: 56px;
    padding: 16px;
    min-width: 200px;
    display: grid;
    gap: 10px;
  }

  .menu-title {
    margin: 0;
    color: var(--md-sys-color-secondary);
    font-size: 0.75rem;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .menu-name {
    margin: 0;
    font-weight: 600;
  }

  main {
    display: grid;
    grid-template-columns: 1fr;
    gap: 2vh;
    min-height: 0;
    overflow: hidden;
  }

  main.chat-open {
    grid-template-columns: minmax(0, 1fr) minmax(280px, 360px);
  }

  .streams {
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  .streams-head {
    display: grid;
    gap: 6px;
  }

  .hint {
    margin: 0;
    color: var(--md-sys-color-on-surface-variant);
  }

  .streams-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 16px;
  }

  .stream-card {
    border: none;
    background: var(--md-sys-color-surface-container-high);
    border-radius: 20px;
    padding: 18px;
    display: grid;
    gap: 10px;
    text-align: left;
    cursor: pointer;
    min-height: 140px;
  }

  .stream-title {
    font-size: 1.1rem;
    font-weight: 600;
  }

  .stream-meta {
    color: var(--md-sys-color-on-surface-variant);
    font-size: 0.9rem;
  }

  .stream-tag {
    align-self: start;
    width: fit-content;
    padding: 6px 10px;
    border-radius: 999px;
    background: var(--md-sys-color-surface);
    font-size: 0.75rem;
  }

  .bottom-actions {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    justify-content: space-between;
  }

  @media (max-width: 900px) {
    main.chat-open {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 600px) {
    .streams-page {
      grid-template-rows: auto minmax(0, 1fr) auto;
      padding: 2vh 4vw 3vh;
    }

    .top-bar,
    .bottom-bar {
      flex-wrap: wrap;
      gap: 12px;
      padding: 12px 16px;
    }

    .top-actions {
      width: 100%;
      justify-content: space-between;
    }
  }
</style>
