<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { refreshSession, signOut, user } from "$lib/stores/auth";
  import { apiUrl, wsUrl } from "$lib/api";
  import ThemeToggle from "$lib/components/ui/ThemeToggle.svelte";
  import Avatar from "$lib/components/ui/Avatar.svelte";


  type PresenceUser = {
    id: string;
    displayName: string;
    avatarUrl?: string;
    status: "online" | "offline" | "idle";
  };

  type ChatMessage = {
    id: string;
    from: string;
    to: string;
    text: string;
    timestamp: string;
  };

  const currentUser = $derived($user);

  let chatOpen = $state(false);
  let showSettings = $state(false);
  let showMenu = $state(false);
  let activeChatId = $state("");
  let onlineCount = $state(0);
  let messageText = $state("");
  let socket: WebSocket | null = null;

  const appIcons = [
    { id: "watchtogether", label: "WatchTogether" },
    { id: "ebook-library", label: "Ebook Library" }
  ];

  let people = $state<PresenceUser[]>([]);
  let messagesByUser = $state<Record<string, ChatMessage[]>>({});

  onMount(async () => {
    const session = await refreshSession();
    if (!session) {
      goto("/");
      return;
    }

    await loadUsers();
    connectSocket();
  });

  onDestroy(() => {
    socket?.close();
  });

  async function handleLogout() {
    await signOut();
    goto("/");
  }

  function initialsFromName(name: string) {
    return name
      .split(" ")
      .map((part) => part[0])
      .slice(0, 2)
      .join("")
      .toUpperCase();
  }

  function formatTime(timestamp: string) {
    if (!timestamp) return "";
    const date = new Date(timestamp);
    if (Number.isNaN(date.getTime())) return "";
    return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }

  async function loadUsers() {
    const response = await fetch(apiUrl("/api/users"), {
      credentials: "include",
    });

    if (!response.ok) return;
    const data = (await response.json()) as PresenceUser[];
    people = data;
    onlineCount = data.filter((person) => person.status === "online").length;
  }

  async function loadHistory(userId: string) {
    const response = await fetch(apiUrl(`/api/messages/${userId}`), {
      credentials: "include",
    });

    if (!response.ok) return;
    const history = (await response.json()) as ChatMessage[];
    messagesByUser = { ...messagesByUser, [userId]: history };
  }

  function connectSocket() {
    socket = new WebSocket(wsUrl("/api/ws"));

    socket.onmessage = (event) => {
      const payload = JSON.parse(event.data) as { type: string; data: any };

      if (payload.type === "Presence") {
        const data = payload.data as { users: PresenceUser[]; onlineCount: number };
        people = data.users;
        onlineCount = data.onlineCount;
      }

      if (payload.type === "Message") {
        const message = payload.data as ChatMessage;
        const current = $user?.id;
        const otherId = message.from === current ? message.to : message.from;
        const existing = messagesByUser[otherId] ?? [];
        messagesByUser = { ...messagesByUser, [otherId]: [...existing, message] };
      }
    };
  }

  async function openChat(person: PresenceUser) {
    activeChatId = person.id;
    chatOpen = true;
    await loadHistory(person.id);
  }

  function closeChatView() {
    activeChatId = "";
  }

  function toggleChatPanel() {
    chatOpen = !chatOpen;
    if (!chatOpen) {
      activeChatId = "";
    }
  }

  function openApp(appId: string) {
    if (appId === "watchtogether") {
      goto("/streams");
      return;
    }
  }

  function sendMessage() {
    const trimmed = messageText.trim();
    if (!trimmed || !activeChatId) return;

    const payload = {
      type: "SendMessage",
      data: { to: activeChatId, text: trimmed },
    };

    if (socket && socket.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify(payload));
    }

    messageText = "";
  }

  function activeUser() {
    return people.find((person) => person.id === activeChatId);
  }

  function activeMessages() {
    return activeChatId ? messagesByUser[activeChatId] ?? [] : [];
  }
</script>

<div class="dashboard">
  <header class="top-bar surface">
    <div>
      <p class="eyebrow">Dashboard</p>
      <h2>Workspace</h2>
    </div>
    <div class="top-actions">
      <button class="icon-btn" type="button">+ Add app</button>
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
    <section class="desktop surface">
      <div class="desktop-head">
        <h3 class="section-title">Desktop apps</h3>
      </div>
      <div class="icon-grid">
        {#each appIcons as app}
          <button class="icon" type="button" onclick={() => openApp(app.id)}>
            <span class="icon-mark">{app.label.slice(0, 1)}</span>
            <span>{app.label}</span>
          </button>
        {/each}
      </div>
    </section>

    {#if chatOpen}
      <aside class="chat-panel surface">
        <div class="chat-header">
          <div>
            <h3 class="section-title">Chat</h3>
            <p class="online-count">{onlineCount} online</p>
          </div>
          {#if activeChatId}
            <button class="icon-btn" type="button" onclick={closeChatView}>
              Back to list
            </button>
          {/if}
        </div>
        <div class="chat-body">
          <div class="user-list">
            {#each activeChatId ? people.filter((person) => person.id === activeChatId) : people as person}
              <div class="user-row">
                <button
                  class:active={person.id === activeChatId}
                  onclick={() => openChat(person)}
                  type="button"
                >
                  <span class="avatar-badge">
                    {#if person.avatarUrl}
                      <img src={person.avatarUrl} alt={person.displayName} />
                    {:else}
                      {initialsFromName(person.displayName)}
                    {/if}
                  </span>
                  <span class="user-meta">
                    <span class="user-name">{person.displayName}</span>
                    <span class="user-status {person.status}">{person.status}</span>
                  </span>
                </button>

                {#if activeChatId === person.id}
                  <div class="chat-inline">
                    <div class="chat-title">
                      <span class="chat-name">{person.displayName}</span>
                      <button class="icon-btn" type="button" onclick={closeChatView}>
                        <span class="material-symbols-rounded">undo</span>
                      </button>
                    </div>
                    <div class="chat-messages">
                      {#each activeMessages() as message}
                        <div class:me={message.from === $user?.id} class="message">
                          <div class="bubble">
                            <p>{message.text}</p>
                            <div class="meta">
                              <span>{formatTime(message.timestamp)}</span>
                            </div>
                          </div>
                        </div>
                      {/each}
                    </div>
                    <div class="chat-input">
                      <input
                        type="text"
                        placeholder="Type a message"
                        bind:value={messageText}
                        onkeydown={(event) => event.key === "Enter" && sendMessage()}
                      />
                      <button class="action-btn" type="button" onclick={sendMessage}>Send</button>
                    </div>
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        </div>
      </aside>
    {/if}
  </main>

  <footer class="bottom-bar surface">
    <button class="icon-btn" type="button" onclick={() => (showSettings = !showSettings)}>
      Settings
    </button>
    <div class="bottom-actions">
      <button class="action-btn" type="button" onclick={toggleChatPanel}>
        {chatOpen ? "Close Chat" : "Chat"}
      </button>
      <ThemeToggle />
    </div>
  </footer>

  {#if showSettings}
    <div class="settings-panel surface">
      <h3 class="section-title">Settings</h3>
      <div class="settings-grid">
        <div>
          <p class="settings-title">Presence</p>
          <p class="settings-body">Show active status and sync presence updates.</p>
        </div>
        <button class="icon-btn" type="button">Enabled</button>
        <div>
          <p class="settings-title">Notifications</p>
          <p class="settings-body">Use subtle tones and minimal banners.</p>
        </div>
        <button class="icon-btn" type="button">Standard</button>
        <div>
          <p class="settings-title">Theme</p>
          <p class="settings-body">Surface containers and calm gradients.</p>
        </div>
        <button class="icon-btn" type="button">Material You</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .dashboard {
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

  .desktop {
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  .desktop-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
  }

  .icon-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 18px;
  }

  .icon {
    background: var(--md-sys-color-surface-container-high);
    border: none;
    border-radius: 20px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    font-weight: 600;
    cursor: pointer;
    width: min(150px, 30vw);
    aspect-ratio: 1 / 1;
    text-align: center;
  }

  .icon-mark {
    height: 36px;
    width: 36px;
    border-radius: 12px;
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
    display: grid;
    place-items: center;
    font-weight: 700;
  }

  .chat-panel {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    height: 100%;
    min-height: 0;
    max-height: 100%;
    overflow: hidden;
  }

  .chat-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .online-count {
    margin: 4px 0 0 0;
    font-size: 0.85rem;
    color: var(--md-sys-color-on-surface-variant);
  }

  .chat-body {
    display: flex;
    flex-direction: column;
    gap: 12px;
    flex: 1;
    min-height: 0;
  }

  .user-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
    flex: 1;
    overflow: hidden;
  }

  .user-list button {
    display: flex;
    align-items: center;
    gap: 12px;
    border: none;
    background: var(--md-sys-color-surface-container-high);
    padding: 10px 12px;
    border-radius: 14px;
    text-align: left;
    cursor: pointer;
    width: 100%;
    margin: 0;
  }

  .user-list button.active {
    outline: 2px solid var(--md-sys-color-primary);
  }

  .avatar-badge {
    height: 36px;
    width: 36px;
    border-radius: 12px;
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
    display: grid;
    place-items: center;
    font-weight: 700;
    overflow: hidden;
  }

  .avatar-badge img {
    height: 100%;
    width: 100%;
    object-fit: cover;
  }

  .user-meta {
    display: grid;
    gap: 2px;
  }

  .user-name {
    font-weight: 600;
  }

  .user-status {
    font-size: 0.75rem;
    color: var(--md-sys-color-on-surface-variant);
    text-transform: capitalize;
  }

  .user-status.online {
    color: var(--md-sys-color-success);
  }

  .user-row {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
  }

  .chat-inline {
    background: var(--md-sys-color-surface-container-high);
    border-radius: 20px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow: hidden;
    min-height: 0;
    flex: 1;
  }

  .chat-title {
    display: grid;
    grid-template-columns: 80% 20%;
    align-items: center;
    font-weight: 600;
    text-align: left;
  }

  .chat-name {
    text-align: left;
  }

  .chat-title .icon-btn {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .chat-messages {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-right: 6px;
    min-height: 0;
  }

  .chat-input {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 10px;
  }

  .chat-input .action-btn {
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }

  .chat-input input {
    border-radius: 12px;
    border: 1px solid var(--md-sys-color-outline);
    padding: 10px 12px;
    font-size: 0.95rem;
    background: var(--md-sys-color-surface);
    color: var(--md-sys-color-on-surface);
  }


  .message {
    display: flex;
  }

  .message.me {
    justify-content: flex-end;
  }

  .bubble {
    background: var(--md-sys-color-surface);
    padding: 10px 14px;
    border-radius: 16px;
    max-width: 70%;
    display: grid;
    gap: 6px;
  }

  .bubble p {
    margin: 0;
  }

  .meta {
    display: flex;
    gap: 12px;
    font-size: 0.75rem;
    color: var(--md-sys-color-on-surface-variant);
  }

  .bottom-bar {
    gap: 16px;
  }

  .bottom-actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .settings-panel {
    position: fixed;
    right: 3vw;
    bottom: 14vh;
    padding: 20px;
    width: min(420px, 90vw);
    display: grid;
    gap: 16px;
  }

  .settings-grid {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 16px 12px;
    align-items: center;
  }

  .settings-title {
    margin: 0;
    font-weight: 600;
  }

  .settings-body {
    margin: 4px 0 0 0;
    color: var(--md-sys-color-on-surface-variant);
  }

  @media (max-width: 900px) {
    main.chat-open {
      grid-template-columns: 1fr;
    }

    .chat-panel {
      position: fixed;
      right: 3vw;
      left: 3vw;
      top: 18vh;
      bottom: 14vh;
      z-index: 10;
    }

    .chat-body {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 600px) {
    .dashboard {
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

    .icon-grid {
      justify-content: center;
    }
  }
</style>
