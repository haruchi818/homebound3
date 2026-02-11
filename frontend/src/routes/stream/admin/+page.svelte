<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { refreshSession, signOut, user } from "$lib/stores/auth";
  import ThemeToggle from "$lib/components/ui/ThemeToggle.svelte";
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
  import { apiUrl, fetchJson, wsUrl } from "$lib/api";
  import Avatar from "$lib/components/ui/Avatar.svelte";
  import IconButton from "$lib/components/ui/IconButton.svelte";
  import Badge from "$lib/components/ui/Badge.svelte";
  import ProgressBar from "$lib/components/ui/ProgressBar.svelte";
  import Card from "$lib/components/ui/Card.svelte";
  import Dialog from "$lib/components/ui/Dialog.svelte";
  import Snackbar from "$lib/components/ui/Snackbar.svelte";

  type MovieRow = {
    id: string;
    filename: string;
    movieTitle?: string | null;
    description?: string | null;
    subtitleFilename?: string | null;
    hlsPath?: string | null;
    transcodingStatus: string;
    durationSeconds?: number | null;
    fileSizeBytes?: number | null;
    uploadDate: string;
  };

  type StreamCreateResponse = {
    streamId: string;
    streamUrl: string;
  };

  type UploadResponse = {
    movieId: string;
    uploadId: string;
    filename: string;
    chunkIndex: number;
    totalChunks: number;
    bytesReceived: number;
    status: string;
  };

  type CameraStartResponse = {
    wsUrl: string;
    hlsUrl: string;
  };

  const currentUser = $derived($user);

  let chatOpen = $state(false);
  let showMenu = $state(false);
  let selectedMovieId = $state("");
  let movies = $state<MovieRow[]>([]);
  let streamId = $state("");
  let cameraActive = $state(false);
  let cameraStream = $state<MediaStream | null>(null);
  let cameraVideo = $state<HTMLVideoElement | null>(null);
  let cameraError = $state("");
  let uploadProgress = $state(0);
  let uploadStatus = $state("");
  let fileInput: HTMLInputElement | null = null;
  let cameraSocket: WebSocket | null = null;
  let cameraRecorder: MediaRecorder | null = null;
  let editOpen = $state(false);
  let editTitle = $state("");
  let editDescription = $state("");
  let editThumbnail = $state<File | null>(null);
  let snackbarOpen = $state(false);
  let snackbarMessage = $state("");
  let snackbarTone = $state<"neutral" | "success" | "error">("neutral");

  const selectedMovie = $derived(movies.find((movie) => movie.id === selectedMovieId));
  const selectedMovieTitle = $derived(
    selectedMovie?.movieTitle ?? selectedMovie?.filename ?? ""
  );
  const selectedMovieInitial = $derived(selectedMovieTitle.slice(0, 1));

  onMount(async () => {
    const session = await refreshSession();
    if (!session) {
      goto("/");
      return;
    }

    await createStream();
    await loadMovies();
  });

  onDestroy(() => {
    stopCamera();
  });

  async function handleLogout() {
    await signOut();
    goto("/");
  }

  function showSnackbar(message: string, tone: "neutral" | "success" | "error" = "neutral") {
    snackbarMessage = message;
    snackbarTone = tone;
    snackbarOpen = true;
  }

  function selectMovie(movieId: string) {
    selectedMovieId = movieId;
  }

  async function createStream() {
    try {
      const response = await fetch(apiUrl("/api/streams/create"), {
        method: "POST",
        credentials: "include",
      });

      if (!response.ok && response.status !== 409) {
        return;
      }

      const payload = (await response.json()) as StreamCreateResponse;
      streamId = payload.streamId;
    } catch (error) {
      // Ignore for now; streamId will remain empty.
    }
  }

  async function loadMovies() {
    try {
      movies = await fetchJson<MovieRow[]>("/api/movies");
    } catch (error) {
      movies = [];
    }
  }

  async function startStreamPlayback() {
    if (!streamId || !selectedMovieId) return;
    try {
      const response = await fetch(apiUrl(`/api/streams/${streamId}/start`), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ movieId: selectedMovieId }),
        credentials: "include",
      });
      if (!response.ok) {
        const text = await response.text();
        showSnackbar(text || "Failed to start stream.", "error");
        return;
      }
      showSnackbar("Stream started.", "success");
    } catch (error) {
      showSnackbar("Failed to start stream.", "error");
    }
  }

  function formatDuration(seconds?: number | null) {
    if (!seconds || seconds <= 0) return "--";
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  }

  function openUploadPicker() {
    fileInput?.click();
  }

  async function handleFileSelected(event: Event) {
    const input = event.target as HTMLInputElement;
    if (!input.files?.length) return;
    const file = input.files[0];
    input.value = "";

    uploadProgress = 0;
    uploadStatus = "Uploading...";
    const uploadId = crypto.randomUUID();
    const chunkSize = 5 * 1024 * 1024;
    const totalChunks = Math.ceil(file.size / chunkSize);

    let lastResponse: UploadResponse | null = null;
    for (let index = 0; index < totalChunks; index += 1) {
      const start = index * chunkSize;
      const end = Math.min(start + chunkSize, file.size);
      const chunk = file.slice(start, end);

      const form = new FormData();
      form.append("file", chunk, file.name);

      let response: Response | null = null;
      let lastError = "";
      for (let attempt = 0; attempt < 3; attempt += 1) {
        try {
          response = await fetch(apiUrl("/api/movies/upload"), {
            method: "POST",
            headers: {
              "x-upload-id": uploadId,
              "x-chunk-index": index.toString(),
              "x-total-chunks": totalChunks.toString(),
            },
            body: form,
            credentials: "include",
          });
          if (!response.ok) {
            lastError = await response.text();
          }
        } catch (error) {
          response = null;
          lastError = "Network error";
        }

        if (response?.ok) {
          break;
        }

        uploadStatus = `Retrying chunk ${index + 1}...`;
        await new Promise((resolve) => setTimeout(resolve, 500 * (attempt + 1)));
      }

      if (!response?.ok) {
        uploadStatus = "Upload failed";
        showSnackbar(
          lastError ? `Upload failed: ${lastError}` : "Upload failed. Please try again.",
          "error"
        );
        return;
      }

      lastResponse = (await response.json()) as UploadResponse;
      uploadProgress = Math.round(((index + 1) / totalChunks) * 100);
    }

    if (lastResponse?.movieId) {
      uploadStatus = "Transcoding...";
      await pollMovie(lastResponse.movieId);
    }
    await loadMovies();
    uploadStatus = "";
  }

  function openEditDialog() {
    if (!selectedMovie) return;
    editTitle = selectedMovie.movieTitle ?? "";
    editDescription = selectedMovie.description ?? "";
    editThumbnail = null;
    editOpen = true;
  }

  async function saveMetadata() {
    if (!selectedMovie) return;
    const form = new FormData();
    if (editTitle.trim()) form.append("movie_title", editTitle.trim());
    if (editDescription.trim()) form.append("description", editDescription.trim());
    if (editThumbnail) form.append("thumbnail", editThumbnail);

    try {
      const response = await fetch(apiUrl(`/api/movies/${selectedMovie.id}`), {
        method: "PUT",
        body: form,
        credentials: "include",
      });
      if (!response.ok) {
        showSnackbar("Failed to update movie metadata.", "error");
        return;
      }
      await loadMovies();
      showSnackbar("Movie updated.", "success");
      editOpen = false;
    } catch (error) {
      showSnackbar("Failed to update movie metadata.", "error");
    }
  }

  async function pollMovie(movieId: string) {
    for (let attempts = 0; attempts < 30; attempts += 1) {
      try {
        const movie = await fetchJson<MovieRow>(`/api/movies/${movieId}`);
        if (movie.transcodingStatus === "ready" || movie.transcodingStatus === "failed") {
          return;
        }
      } catch (error) {
        return;
      }

      await new Promise((resolve) => setTimeout(resolve, 5000));
    }
  }

  async function toggleCamera() {
    if (cameraActive) {
      stopCamera();
      return;
    }

    cameraError = "";
    if (!navigator.mediaDevices?.getUserMedia) {
      cameraError = "Camera access is not supported in this browser.";
      return;
    }

    try {
      cameraStream = await navigator.mediaDevices.getUserMedia({ video: true, audio: true });
      cameraActive = true;
    } catch (error) {
      cameraError = "Camera access was blocked.";
      cameraActive = false;
      return;
    }

    if (!streamId) return;
    const startResponse = await fetchJson<CameraStartResponse>(
      `/api/streams/${streamId}/camera/start`,
      { method: "POST" }
    );

    cameraSocket = new WebSocket(wsUrl(startResponse.wsUrl));
    cameraRecorder = new MediaRecorder(cameraStream, { mimeType: "video/webm;codecs=vp8,opus" });
    cameraRecorder.ondataavailable = (event) => {
      if (event.data.size === 0 || !cameraSocket) return;
      if (cameraSocket.readyState === WebSocket.OPEN) {
        cameraSocket.send(event.data);
      }
    };
    cameraRecorder.start(1000);
  }

  function stopCamera() {
    cameraRecorder?.stop();
    cameraRecorder = null;
    cameraSocket?.close();
    cameraSocket = null;
    cameraStream?.getTracks().forEach((track) => track.stop());
    cameraStream = null;
    cameraActive = false;
  }

  $effect(() => {
    if (cameraVideo && cameraStream) {
      cameraVideo.srcObject = cameraStream;
    }
  });
</script>

<div class="admin-page">
  <header class="top-bar surface">
    <div>
      <p class="eyebrow">Watch Together</p>
      <h2>Stream admin</h2>
      <p class="stream-id">Stream ID: {streamId || "creating..."}</p>
    </div>
    <div class="top-actions">
      <button
        class="action-btn"
        type="button"
        disabled={!streamId}
        onclick={() => goto(`/stream/${streamId}`)}
      >
        Join Stream
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
    <section class="admin surface">
      <div class="admin-grid">
        <div class="admin-left">
          <div class="list-block">
            <h3 class="section-title">Movie library</h3>
            <div class="movie-list">
              {#if movies.length === 0}
                <p class="preview-empty">No movies yet. Upload a video to start.</p>
              {:else}
                {#each movies as movie}
                <button
                  class:selected={movie.id === selectedMovieId}
                  class="movie-item"
                  type="button"
                  onclick={() => selectMovie(movie.id)}
                >
                  <span class="movie-title">{movie.movieTitle ?? movie.filename}</span>
                  <span class="movie-file">
                    {movie.filename} · {formatDuration(movie.durationSeconds)}
                  </span>
                  <Badge label={movie.transcodingStatus} tone={movie.transcodingStatus} />
                </button>
                {/each}
              {/if}
            </div>
          </div>
          <div class="list-actions">
            <IconButton
              label="Play to stream"
              type="button"
              onclick={startStreamPlayback}
              disabled={!selectedMovieId}
            >
              Play to Stream
            </IconButton>
            <input
              class="visually-hidden"
              type="file"
              accept="video/*"
              bind:this={fileInput}
              onchange={handleFileSelected}
            />
            <IconButton label="Upload movie" type="button" onclick={openUploadPicker}>
              Upload movie
            </IconButton>
            <IconButton label="Share camera" type="button" onclick={toggleCamera}>
              {cameraActive ? "Stop Camera" : "Share Camera"}
            </IconButton>
          </div>
          {#if uploadStatus}
            <ProgressBar value={uploadProgress} label={`${uploadStatus} ${uploadProgress}%`} />
          {/if}
        </div>

        <div class="admin-right">
          <Card class="preview-card" elevated>
            <h3 class="section-title">Movie preview</h3>
            {#if selectedMovie}
              <div class="preview-grid">
                <div class="preview-art">
                  {selectedMovieInitial}
                </div>
                <div class="preview-info">
                  <p class="preview-title">
                    {selectedMovieTitle}
                  </p>
                  <p class="preview-desc">{selectedMovie.description ?? "No description."}</p>
                  <div class="preview-meta">
                    <span>File: {selectedMovie.filename}</span>
                    <span>Status: {selectedMovie.transcodingStatus}</span>
                  </div>
                  <button class="action-btn secondary" type="button" onclick={openEditDialog}>
                    Edit metadata
                  </button>
                </div>
              </div>
            {:else}
              <div class="preview-empty">
                <p>Select a movie to preview details.</p>
              </div>
            {/if}
          </Card>

          <Card class="preview-card" elevated>
            <h3 class="section-title">Camera preview</h3>
            {#if cameraActive && cameraStream}
              <div class="camera-frame">
                <video
                  class="camera-feed"
                  bind:this={cameraVideo}
                  autoplay
                  muted
                  playsinline
                ></video>
              </div>
            {:else}
              <div class="preview-empty">
                <p>Camera is not sharing yet.</p>
                {#if cameraError}
                  <p class="error-text">{cameraError}</p>
                {/if}
              </div>
            {/if}
          </Card>
        </div>
      </div>
    </section>

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

<Dialog open={editOpen} title="Edit movie" on:close={() => (editOpen = false)}>
  <div class="edit-form">
    <label>
      Title
      <input type="text" bind:value={editTitle} placeholder="Movie title" />
    </label>
    <label>
      Description
      <textarea rows={4} bind:value={editDescription} placeholder="Short description"></textarea>
    </label>
    <label class="edit-thumb">
      Thumbnail
      <input type="file" accept="image/*" onchange={(event) => {
        const input = event.target as HTMLInputElement;
        editThumbnail = input.files?.[0] ?? null;
      }} />
    </label>
  </div>
  <div slot="actions">
    <button class="action-btn secondary" type="button" onclick={() => (editOpen = false)}>
      Cancel
    </button>
    <button class="action-btn" type="button" onclick={saveMetadata}>
      Save
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
  .admin-page {
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

  .admin {
    padding: 24px;
    height: 100%;
    min-height: 0;
  }

  .admin-grid {
    display: grid;
    grid-template-columns: 35% 65%;
    gap: 24px;
    height: 100%;
    min-height: 0;
  }

  .admin-left {
    display: grid;
    grid-template-rows: 1fr auto;
    gap: 16px;
    min-height: 0;
  }

  .list-block {
    display: grid;
    gap: 12px;
    min-height: 0;
  }

  .movie-list {
    display: grid;
    gap: 10px;
    overflow-y: auto;
    padding-right: 6px;
  }

  .movie-item {
    border: none;
    background: var(--md-sys-color-surface-container-high);
    border-radius: 16px;
    padding: 12px 14px;
    display: grid;
    gap: 4px;
    text-align: left;
    cursor: pointer;
  }

  .movie-item.selected {
    outline: 2px solid var(--md-sys-color-primary);
  }

  .movie-title {
    font-weight: 600;
  }

  .movie-file {
    font-size: 0.85rem;
    color: var(--md-sys-color-on-surface-variant);
  }

  .list-actions {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
  }

  .admin-right {
    display: grid;
    grid-template-rows: minmax(0, 1fr) minmax(0, 1fr);
    gap: 16px;
    min-height: 0;
  }

  .preview-card {
    background: var(--md-sys-color-surface-container-high);
    border-radius: 22px;
    padding: 18px;
    display: grid;
    gap: 14px;
    min-height: 0;
  }

  .preview-grid {
    display: grid;
    grid-template-columns: 120px 1fr;
    gap: 16px;
    align-items: center;
  }

  .preview-art {
    height: 120px;
    border-radius: 20px;
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
    display: grid;
    place-items: center;
    font-size: 2rem;
    font-weight: 700;
  }

  .preview-title {
    margin: 0 0 6px 0;
    font-weight: 600;
  }

  .preview-desc {
    margin: 0 0 12px 0;
    color: var(--md-sys-color-on-surface-variant);
  }

  .preview-meta {
    display: grid;
    gap: 4px;
    font-size: 0.85rem;
    color: var(--md-sys-color-on-surface-variant);
    margin-bottom: 12px;
  }

  .preview-empty {
    display: grid;
    gap: 10px;
    color: var(--md-sys-color-on-surface-variant);
  }

  .edit-form {
    display: grid;
    gap: 12px;
  }

  .edit-form label {
    display: grid;
    gap: 6px;
    font-size: var(--md-sys-typescale-label-medium);
  }

  .edit-form input,
  .edit-form textarea {
    border-radius: 12px;
    border: 1px solid var(--md-sys-color-outline);
    padding: 10px 12px;
    background: transparent;
    color: var(--md-sys-color-on-surface);
  }

  .edit-thumb input {
    border: none;
    padding: 0;
  }

  .visually-hidden {
    position: absolute;
    opacity: 0;
    pointer-events: none;
    height: 0;
    width: 0;
  }

  .camera-frame {
    width: 100%;
    aspect-ratio: 16 / 9;
    border-radius: 18px;
    background: #000;
    overflow: hidden;
    display: grid;
    place-items: center;
  }

  .camera-feed {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .error-text {
    color: var(--md-sys-color-warning);
  }

  .bottom-actions {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    justify-content: space-between;
  }

  @media (max-width: 980px) {
    .admin-grid {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 900px) {
    main.chat-open {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 600px) {
    .admin-page {
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

    .preview-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
