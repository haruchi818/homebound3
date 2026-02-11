# Project Context

## Stack
- Frontend: Svelte with hls.js for video playback
- Backend: Rust with Actix-web
- Database: PostgreSQL
- Video Processing: FFmpeg for HLS transcoding
- Real-time: WebSocket for synchronization and chat

## UI Style
- Material Design 3 (Material You)
- Prefer surfaceContainer backgrounds
- Avoid card-heavy layouts
- Favor spacing over borders
- Expressive but minimal

---

## Screens and Behavior

### Home / Login
- Home page is a login screen
- Authentication is Google-only via OAuth2 from Google Cloud

### Dashboard (After Login)
- Layout uses the full window
- **Top bar: 15% of window height**
  - Right side shows the user avatar
  - Clicking the avatar opens a menu with a Logout button
- **Main section: 75% of window height**
  - Desktop-style application launcher with clickable icons
  - Applications include: Watch Together, etc.
- **Bottom bar: 10% of window height**
  - Contains Settings and Chat buttons
  - Clicking Chat opens a right-side panel listing all logged-in users
  - Clicking a user opens a chat view (Facebook-style messaging)

---

## Watch Together Application

### Streams Overview (`/streams`)
- Accessible by clicking "Watch Together" icon on dashboard
- **Top bar: 15% of window height**
  - User avatar (same as dashboard)
  - "Start Stream" button (redirects to `/stream/admin`)
- **Main section: 75% of window height**
  - Grid/list of live stream buttons
  - Each button shows:
    - Stream host name
    - Current viewers count
    - Stream status (live/starting)
    - Current movie playing (if any)
  - Click to join stream
- **Bottom bar: 10% of window height**
  - Chat button (same behavior as dashboard)
  - Dark mode toggle

### Stream Admin (`/stream/admin`)
- Accessible via "Start Stream" button from `/streams`
- **Stream ID generation:**
  - Based on user's email
  - Remove `@` and replace `.` with `-`
  - Example: `haruchi@gmail.com` → `haruchi-gmail-com`
- **Top bar: 15% of window height**
  - User avatar (same as dashboard)
  - "Join Stream" button (redirects to stream room as viewer)
  - Display stream ID prominently
  - Stream status indicator (starting/live/ended)
- **Main section: 75% of window height**
  - Split into two columns:
    - **Left column (35%):**
      - **Top 50%:** List of movies from server's public directory
        - Shows movie title, duration, and format
        - Indicates HLS conversion status (ready/processing/pending)
      - **Upload button** below the list
        - Accepts common video formats (mp4, mkv, avi, mov, webm)
        - Shows upload progress
        - Triggers automatic HLS transcoding on completion
      - **"Share Camera" button** to start browser camera stream
        - Uses WebRTC for camera capture
        - Transcodes camera feed to HLS in real-time
    - **Right column (65%):**
      - **Movie preview area:**
        - If movie selected: show video thumbnail, title, and short description
        - Data from database: `id`, `filename`, `movie_title`, `description`, `movie_image` (blob), `subtitle_filename`, `hls_path`, `transcoding_status`
        - If no database entry exists: show "Edit" button to add metadata
        - Shows HLS availability status
        - "Play to Stream" button to start streaming selected movie
      - **Camera preview area:**
        - If "Share Camera" active: show live camera feed preview
        - Camera settings (resolution, bitrate)
        - "Stream Camera" button to broadcast camera to viewers
- **Bottom bar: 10% of window height**
  - Chat button
  - Dark mode toggle

### Stream Room (`/stream/:streamId`)
- Accessible via "Join Stream" button from `/stream/admin` or from `/streams`
- **Top bar: 15% of window height**
  - Stream title and host name
  - Viewer count
  - User avatar
  - Leave stream button
- **Main section: 75% of window height**
  - **Video player: 70% width**
    - HLS video player using hls.js
    - Playback synchronized via WebSocket
    - Controls disabled for viewers (host controls playback)
    - Subtitle support if available
    - Quality selector (auto/720p/480p/360p based on HLS manifest)
  - **Right sidebar: 30% width**
    - Real-time chat for stream participants
    - Viewer list with online status
    - Reactions/emojis
- **Bottom bar: 10% of window height**
  - Playback info (current time, buffering status)
  - Connection quality indicator
  - Dark mode toggle

---

## Video Streaming Architecture (Server-Side HLS)

### Upload and Transcoding Flow
1. **User uploads video** via `/stream/admin`
2. **Backend receives file** (Actix multipart upload)
3. **Store original file** in configured directory (`/var/media/originals/`)
4. **Save metadata to database** (filename, size, upload timestamp)
5. **Queue transcoding job** (background task)
6. **FFmpeg transcodes to HLS:**
```bash
   ffmpeg -i input.mp4 \
     -codec:v libx264 -codec:a aac \
     -hls_time 10 \
     -hls_playlist_type vod \
     -hls_segment_filename /var/media/hls/movie-id/segment%03d.ts \
     -start_number 0 \
     /var/media/hls/movie-id/playlist.m3u8
```
7. **Generate multiple quality variants:**
   - 1080p (if source supports)
   - 720p
   - 480p
   - 360p
8. **Create master playlist** linking all variants
9. **Update database** with `hls_path` and `transcoding_status = 'ready'`
10. **Extract thumbnail** for movie preview:
```bash
    ffmpeg -i input.mp4 -ss 00:00:05 -vframes 1 thumbnail.jpg
```

### Streaming Flow
1. **Host selects movie** in `/stream/admin`
2. **Host clicks "Play to Stream"**
3. **Backend creates stream session** in database
4. **WebSocket broadcasts** stream start to all connected clients
5. **Backend serves HLS manifest** at `/hls/:streamId/playlist.m3u8`
6. **Viewers' browsers:**
   - Load hls.js library
   - Fetch master playlist
   - Select appropriate quality variant
   - Start downloading segments
7. **Host controls playback:**
   - Play/pause/seek events sent via WebSocket
   - Backend broadcasts to all viewers
   - Viewers sync their playback to host's timestamp
8. **Synchronization mechanism:**
   - Host sends heartbeat with current timestamp every 2 seconds
   - Viewers adjust playback if drift exceeds 2 seconds
   - Buffer ahead to handle network jitter

### Camera Streaming Flow
1. **Host clicks "Share Camera"**
2. **Browser requests camera permission** (WebRTC MediaStream)
3. **Frontend captures camera stream** via MediaRecorder API
4. **Send chunks to backend** via WebSocket or chunked upload
5. **Backend pipes to FFmpeg** for real-time HLS transcoding:
```bash
   ffmpeg -f webm -i pipe:0 \
     -codec:v libx264 -preset ultrafast -tune zerolatency \
     -codec:a aac \
     -hls_time 2 \
     -hls_list_size 5 \
     -hls_flags delete_segments \
     /var/media/hls/camera-streamId/live.m3u8
```
6. **Viewers receive live HLS stream** with ~6-10 second latency
7. **Stream ends** when host clicks stop or leaves

---

## Database Schema

### Movies Table
| Column | Type | Description |
|--------|------|-------------|
| `id` | UUID | Primary key |
| `filename` | VARCHAR | Original video file name |
| `movie_title` | VARCHAR | Display title |
| `description` | TEXT | Short description |
| `movie_image` | BLOB | Thumbnail/poster image |
| `subtitle_filename` | VARCHAR | Subtitle file name (optional) |
| `hls_path` | VARCHAR | Path to HLS master playlist |
| `transcoding_status` | ENUM | pending/processing/ready/failed |
| `duration_seconds` | INTEGER | Video duration |
| `file_size_bytes` | BIGINT | Original file size |
| `upload_date` | TIMESTAMP | Upload timestamp |
| `uploader_id` | UUID | Foreign key to users table |

### Streams Table
| Column | Type | Description |
|--------|------|-------------|
| `stream_id` | VARCHAR | Generated from user email (PK) |
| `user_id` | UUID | Stream creator (FK) |
| `status` | ENUM | starting/live/ended |
| `created_at` | TIMESTAMP | Stream start time |
| `ended_at` | TIMESTAMP | Stream end time (nullable) |
| `current_movie_id` | UUID | Currently playing movie (nullable, FK) |
| `current_timestamp` | FLOAT | Current playback position in seconds |
| `is_playing` | BOOLEAN | Play/pause state |
| `stream_type` | ENUM | movie/camera |
| `viewer_count` | INTEGER | Current viewers (updated via WebSocket) |

### Stream_Viewers Table (for tracking)
| Column | Type | Description |
|--------|------|-------------|
| `id` | UUID | Primary key |
| `stream_id` | VARCHAR | Foreign key to streams |
| `user_id` | UUID | Viewer user ID |
| `joined_at` | TIMESTAMP | When viewer joined |
| `left_at` | TIMESTAMP | When viewer left (nullable) |
| `is_active` | BOOLEAN | Currently watching |

---

## WebSocket Protocol

### Connection
- **Endpoint:** `ws://backend/ws/stream/:streamId`
- **Authentication:** JWT token in query param or header
- **Heartbeat:** Ping/pong every 30 seconds

### Message Types (JSON)

#### From Host to Server:
```json
{
  "type": "playback_control",
  "action": "play" | "pause" | "seek",
  "timestamp": 123.45
}

{
  "type": "movie_start",
  "movie_id": "uuid"
}

{
  "type": "stream_end"
}
```

#### From Server to Viewers:
```json
{
  "type": "playback_sync",
  "action": "play" | "pause" | "seek",
  "timestamp": 123.45,
  "host_id": "uuid"
}

{
  "type": "viewer_update",
  "count": 5,
  "viewers": [{"id": "uuid", "name": "User1"}, ...]
}

{
  "type": "stream_ended",
  "reason": "host_ended" | "host_disconnected"
}
```

#### Chat Messages:
```json
{
  "type": "chat_message",
  "user_id": "uuid",
  "username": "John",
  "message": "Hello!",
  "timestamp": "2025-02-08T12:00:00Z"
}
```

---

## File Storage Structure
```
/var/media/
├── originals/
│   ├── {movie-uuid}.mp4
│   ├── {movie-uuid}.mkv
│   └── ...
├── hls/
│   ├── {movie-uuid}/
│   │   ├── playlist.m3u8 (master)
│   │   ├── 1080p/
│   │   │   ├── playlist.m3u8
│   │   │   ├── segment000.ts
│   │   │   ├── segment001.ts
│   │   │   └── ...
│   │   ├── 720p/
│   │   ├── 480p/
│   │   └── 360p/
│   └── camera-{stream-id}/
│       ├── live.m3u8
│       ├── segment000.ts (auto-deleted after playback)
│       └── ...
├── thumbnails/
│   ├── {movie-uuid}.jpg
│   └── ...
└── subtitles/
    ├── {movie-uuid}.srt
    ├── {movie-uuid}.vtt
    └── ...
```

---

## Navigation and Routing
- Unauthenticated users are redirected to Home / Login screen
- Authenticated users land on Dashboard
- Dashboard applications route to their respective views:
  - Watch Together → `/streams`
  - Start Stream → `/stream/admin`
  - Join Stream → `/stream/:streamId`
- Logout returns the user to Home / Login screen
- Leaving a stream returns to `/streams` overview

## Authentication and Session
- Google OAuth2 is the only login method
- Store minimal user profile data for avatar and display name
- Sessions should expire cleanly and require re-authentication
- WebSocket connections authenticated via session token

## Presence and Realtime
- Chat sidebar shows currently logged-in users
- Presence updates in near real-time
- Offline users disappear from the list without reload
- Stream rooms track active viewers via WebSocket connections
- Viewer count updates automatically when users join/leave

## Chat Behavior
- Opening chat does not navigate away from current view
- Chat is a right-side panel that reduces main content width (except in stream room where it's integrated)
- Conversation list and active chat visible within panel
- Messages show sender, timestamp, and delivery status
- Chat available on Dashboard, `/streams`, `/stream/admin`, and `/stream/:streamId`
- Stream room chat is scoped to that stream only

## Settings
- Settings opens from the bottom bar
- Avoid heavy cards; use spacing-driven sections
- Includes:
  - Dark mode preference
  - Video quality preference (auto/high/medium/low)
  - Notification settings for stream starts
  - Storage usage and cleanup options

## Responsive Behavior
- Layout proportions (15/75/10) scale with window height
- Chat panel adapts to narrow screens by overlaying instead of shrinking
- Stream admin columns (35/65) stack on mobile/tablet
- Stream room video player goes fullscreen on mobile with chat as overlay

## Accessibility
- All interactive elements are keyboard reachable
- Color contrast meets Material Design 3 accessibility guidance
- Video players include captions support (from subtitle files)
- Screen reader announcements for stream status changes
- Keyboard shortcuts for playback control (space=play/pause, arrows=seek)

## Non-Functional Requirements
- Fast first render on login
- Smooth transitions when opening menus and chat
- Avoid visual clutter; prioritize whitespace and hierarchy
- Video streaming optimized for low latency (6-10 second delay acceptable)
- Efficient handling of video uploads (chunked upload with progress)
- HLS transcoding should be backgrounded and not block UI
- Maximum simultaneous streams: 10 (configurable)
- Maximum upload size: 10GB per video (configurable)
- Automatic cleanup of old HLS segments for ended streams
- CDN-ready HLS serving (optional nginx/cloudflare integration)

## Backend API Endpoints (Rust/Actix)

### Movies
- `POST /api/movies/upload` - Upload video file (multipart)
- `GET /api/movies` - List all movies with transcoding status
- `GET /api/movies/:id` - Get movie details
- `PUT /api/movies/:id` - Update movie metadata
- `DELETE /api/movies/:id` - Delete movie and HLS files
- `GET /hls/:movie-id/playlist.m3u8` - Serve HLS master playlist
- `GET /hls/:movie-id/:quality/segment*.ts` - Serve HLS segments

### Streams
- `POST /api/streams/create` - Create new stream (returns stream_id)
- `GET /api/streams` - List all active streams
- `GET /api/streams/:stream-id` - Get stream details
- `POST /api/streams/:stream-id/start` - Start movie playback
- `POST /api/streams/:stream-id/end` - End stream
- `WS /ws/stream/:stream-id` - WebSocket for sync and chat

### Camera
- `POST /api/streams/:stream-id/camera/start` - Initialize camera stream
- `WS /ws/camera/:stream-id` - WebSocket for camera chunk upload
- `GET /hls/camera-:stream-id/live.m3u8` - Serve live camera HLS

## Frontend Libraries (Svelte)

- **hls.js** - HLS playback in browser
- **socket.io-client** or native WebSocket - Real-time communication
- **Material Components for Svelte** - UI components
- **date-fns** - Timestamp formatting
- **file-upload** library - Chunked upload with progress

## Security Considerations

- Validate video file types on upload (magic bytes, not just extension)
- Limit upload rate per user (prevent DoS)
- Stream access control (only authenticated users can join)
- HLS segment URLs should include short-lived tokens
- Sanitize user input in chat messages (XSS prevention)
- Rate limit WebSocket messages
- Implement CORS properly for HLS serving
- Clean up abandoned streams after host disconnect timeout (5 minutes)