<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import { refreshSession, signOut, user } from "$lib/stores/auth";
  import ThemeToggle from "$lib/components/ui/ThemeToggle.svelte";
  import { apiUrl, fetchJson, wsUrl } from "$lib/api";
  import Hls from "hls.js";
  import Avatar from "$lib/components/ui/Avatar.svelte";
  import Card from "$lib/components/ui/Card.svelte";
  import Dialog from "$lib/components/ui/Dialog.svelte";
  import Snackbar from "$lib/components/ui/Snackbar.svelte";

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

  type MovieRow = {
    id: string;
    hlsPath?: string | null;
    movieTitle?: string | null;
    description?: string | null;
  };

  type Viewer = {
    id: string;
    displayName: string;
  };

  type StreamEvent =
    | { type: "playback_sync"; action: string; timestamp: number; hostId: string }
    | { type: "movie_start"; movieId: string }
    | { type: "stream_ended"; reason: string }
    | { type: "chat_message"; userId: string; username: string; message: string; timestamp: string }
    | { type: "viewer_update"; count: number; viewers: Viewer[] };

  const currentUser = $derived($user);

  let chatOpen = $state(false);
  let showMenu = $state(false);
  let stream: StreamRow | null = null;
  let viewers = $state<Viewer[]>([]);
  let viewerCount = $state(0);
  let chatMessages = $state<StreamEvent[]>([]);
  let chatText = $state("");
  let isHost = $state(false);
  let videoEl: HTMLVideoElement | null = null;
  let hls: Hls | null = null;
  let ws: WebSocket | null = null;
  let currentMovie = $state<MovieRow | null>(null);
  let leaveOpen = $state(false);
  let snackbarOpen = $state(false);
  let snackbarMessage = $state("");
  let snackbarTone = $state<"neutral" | "success" | "error">("neutral");
  let reconnecting = $state(false);
  let reconnectAttempt = $state(0);
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  const streamId = $derived($page.params.streamId);

  onMount(async () => {
    const session = await refreshSession();
    if (!session) {
      goto("/");
      return;
    }

    const loaded = await loadStream();
    if (loaded) {
      connectStreamSocket();
    }
  });

  onDestroy(() => {
    ws?.close();
    hls?.destroy();
    if (reconnectTimer) clearTimeout(reconnectTimer);
  });

  async function handleLogout() {
    await signOut();
    goto("/");
  }

  async function loadStream() {
    try {
      stream = await fetchJson<StreamRow>(`/api/streams/${streamId}`);
      viewerCount = stream.viewerCount;
      isHost = currentUser?.id === stream.userId;
      if (stream.currentMovieId) {
        await loadMovie(stream.currentMovieId);
      }
      return true;
    } catch (error) {
      snackbarMessage = "Unable to load stream details.";
      snackbarTone = "error";
      snackbarOpen = true;
      return false;
    }
  }

  async function loadMovie(movieId: string) {
    currentMovie = await fetchJson<MovieRow>(`/api/movies/${movieId}`);
    if (!currentMovie?.hlsPath) return;
    initHlsPlayer(currentMovie.hlsPath);
  }

  function initHlsPlayer(path: string) {
    if (!videoEl) return;
    const source = path.startsWith("http") ? path : apiUrl(path);
    if (Hls.isSupported()) {
      hls?.destroy();
      hls = new Hls({ enableWorker: true, lowLatencyMode: false, backBufferLength: 90 });
      hls.loadSource(source);
      hls.attachMedia(videoEl);
    } else {
      videoEl.src = source;
    }
  }

  function connectStreamSocket() {
    reconnecting = false;
    ws = new WebSocket(wsUrl(`/ws/stream/${streamId}`));

    ws.onopen = () => {
      reconnectAttempt = 0;
      reconnecting = false;
    };

    ws.onclose = () => {
      if (!reconnecting) {
        startReconnect();
      }
    };

    ws.onmessage = (event) => {
      const message = JSON.parse(event.data) as StreamEvent;

      if (message.type === "viewer_update") {
        viewers = message.viewers;
        viewerCount = message.count;
      }

      if (message.type === "chat_message") {
        chatMessages = [...chatMessages, message];
      }

      if (message.type === "movie_start") {
        loadMovie(message.movieId);
      }

      if (message.type === "playback_sync" && videoEl) {
        const drift = Math.abs(videoEl.currentTime - message.timestamp);
        if (drift > 2) {
          videoEl.currentTime = message.timestamp;
        }

        if (message.action === "play") {
          videoEl.play();
        } else if (message.action === "pause") {
          videoEl.pause();
        } else if (message.action === "seek") {
          videoEl.currentTime = message.timestamp;
        }
      }

      if (message.type === "stream_ended") {
        goto("/streams");
      }
    };
  }

  function startReconnect() {
    reconnecting = true;
    reconnectAttempt += 1;
    const delay = Math.min(8000, 1000 * 2 ** (reconnectAttempt - 1));
    snackbarMessage = `Reconnecting... (${reconnectAttempt})`;
    snackbarTone = "neutral";
    snackbarOpen = true;

    if (reconnectTimer) clearTimeout(reconnectTimer);
    reconnectTimer = setTimeout(() => {
      connectStreamSocket();
    }, delay);
  }

  function sendWs(payload: object) {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(payload));
    }
  }

  function sendPlayback(action: "play" | "pause" | "seek") {
    if (!videoEl || !isHost) return;
    sendWs({ type: "playback_control", action, timestamp: videoEl.currentTime });
  }

  function sendChat() {
    const trimmed = chatText.trim();
    if (!trimmed) return;
    sendWs({ type: "chat_message", message: trimmed });
    chatText = "";
  }
</script>

<div class="room-page">
  <header class="top-bar surface">
    <div>
      <p class="eyebrow">Watch Together</p>
      <h2>Stream room</h2>
      <p class="stream-id">Room: {streamId}</p>
    </div>
    <div class="top-actions">
      <button class="icon-btn" type="button" onclick={() => (leaveOpen = true)}>
        Back to streams
      </button>
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
    <section class="room surface">
      <div class="room-hero">
        <div>
          <h3 class="section-title">Synchronized playback</h3>
          <p class="room-copy">
            This is the shared viewing space. Playback, chat, and presence will sync in real
            time.
          </p>
        </div>
        <button class="action-btn" type="button" onclick={() => sendPlayback("play")} disabled={!isHost}>
          Start playback
        </button>
      </div>
      {#if reconnecting}
        <div class="reconnect-banner">
          <span>Reconnecting to stream...</span>
          <span>Attempt {reconnectAttempt}</span>
        </div>
      {/if}
      <Card class="player-shell" elevated>
        <div class="player-screen">
          <video bind:this={videoEl} controls playsinline></video>
        </div>
        <div class="player-meta">
          <div>
            <p class="player-title">
              {currentMovie?.movieTitle ?? "Awaiting stream selection"}
            </p>
            <p class="player-sub">
              {currentMovie?.description ?? "Stream host will choose the movie."}
            </p>
          </div>
          <button class="icon-btn" type="button" onclick={() => sendPlayback("seek")}>Request sync</button>
        </div>
        {#if isHost}
          <div class="player-controls">
            <button class="icon-btn" type="button" onclick={() => sendPlayback("play")}>Play</button>
            <button class="icon-btn" type="button" onclick={() => sendPlayback("pause")}>Pause</button>
          </div>
        {/if}
      </Card>
    </section>

    <aside class:open={chatOpen} class="stream-chat surface">
      <div class="chat-head">
        <div>
          <h3 class="section-title">Chat</h3>
          <p class="chat-meta">{viewerCount} viewers</p>
        </div>
      </div>
      <div class="chat-body">
        <div class="chat-messages">
          {#each chatMessages as message}
            {#if message.type === "chat_message"}
              <div class:me={message.userId === currentUser?.id} class="chat-message">
                <div class="chat-bubble">
                  <p>{message.message}</p>
                  <span>{message.username}</span>
                </div>
              </div>
            {/if}
          {/each}
        </div>
      </div>
      <div class="chat-input">
        <input
          type="text"
          placeholder="Type a message"
          bind:value={chatText}
          onkeydown={(event) => event.key === "Enter" && sendChat()}
        />
        <button class="action-btn" type="button" onclick={sendChat}>Send</button>
      </div>
    </aside>
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

<Dialog open={leaveOpen} title="Leave stream?" on:close={() => (leaveOpen = false)}>
  <p>Are you sure you want to leave this stream?</p>
  <div slot="actions">
    <button class="action-btn secondary" type="button" onclick={() => (leaveOpen = false)}>
      Stay
    </button>
    <button class="action-btn" type="button" onclick={() => goto("/streams")}>
      Leave
    </button>
  </div>
</Dialog>

<Snackbar
  open={snackbarOpen}
  message={snackbarMessage}
  tone={snackbarTone}
  on:close={() => (snackbarOpen = false)}
/>

<style>
  .room-page {
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

  .stream-id {
    margin: 6px 0 0 0;
    color: var(--md-sys-color-on-surface-variant);
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

  .room {
    padding: 24px;
    display: grid;
    gap: 24px;
  }

  .room-hero {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }

  .reconnect-banner {
    background: var(--md-sys-color-surface-container-high);
    border: 1px dashed var(--md-sys-color-outline);
    border-radius: 16px;
    padding: 10px 14px;
    display: flex;
    justify-content: space-between;
    color: var(--md-sys-color-on-surface-variant);
  }

  .room-copy {
    margin: 6px 0 0 0;
    color: var(--md-sys-color-on-surface-variant);
  }

  .player-shell {
    background: var(--md-sys-color-surface-container-high);
    border-radius: 24px;
    padding: 18px;
    display: grid;
    gap: 16px;
    height: 100%;
  }

  .player-screen {
    background: #000;
    border-radius: 18px;
    overflow: hidden;
    aspect-ratio: 16 / 9;
  }

  .player-screen video {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .player-controls {
    display: flex;
    gap: 12px;
  }

  .stream-chat {
    padding: 18px;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    gap: 12px;
    opacity: 0;
    pointer-events: none;
    transform: translateX(8px);
    transition: 0.2s ease;
  }

  .stream-chat.open {
    opacity: 1;
    pointer-events: auto;
    transform: translateX(0);
  }

  .chat-meta {
    margin: 4px 0 0 0;
    color: var(--md-sys-color-on-surface-variant);
  }

  .chat-body {
    overflow: hidden;
  }

  .chat-messages {
    display: grid;
    gap: 10px;
    overflow-y: auto;
    max-height: 100%;
    padding-right: 6px;
  }

  .chat-message {
    display: flex;
  }

  .chat-message.me {
    justify-content: flex-end;
  }

  .chat-bubble {
    background: var(--md-sys-color-surface-container-high);
    padding: 10px 12px;
    border-radius: 16px;
    max-width: 240px;
    display: grid;
    gap: 4px;
  }

  .chat-bubble span {
    font-size: 0.75rem;
    color: var(--md-sys-color-on-surface-variant);
  }

  .chat-input {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 10px;
  }

  .chat-input input {
    border-radius: 12px;
    border: 1px solid var(--md-sys-color-outline);
    padding: 10px 12px;
    background: transparent;
    color: var(--md-sys-color-on-surface);
  }

  .player-screen {
    background: #0a0a0a;
    color: #f4f1ec;
    border-radius: 20px;
    display: grid;
    place-items: center;
    font-size: 1.05rem;
    height: min(55vh, 480px);
  }

  .player-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .player-title {
    margin: 0;
    font-weight: 600;
  }

  .player-sub {
    margin: 6px 0 0 0;
    color: var(--md-sys-color-on-surface-variant);
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

    .room-hero {
      flex-direction: column;
      align-items: flex-start;
    }
  }

  @media (max-width: 600px) {
    .room-page {
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
