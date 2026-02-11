# Testing Guide

## Prereqs
- Backend running on http://127.0.0.1:8080
- Frontend running on http://127.0.0.1:5173
- DATABASE_URL configured
- FFmpeg available (FFMPEG_PATH or in PATH)
- Media directories writable

## Phase 2: Upload + Transcoding
### Upload flow
1. Login to the frontend.
2. Go to /stream/admin.
3. Upload a video (mp4/mkv/avi/mov/webm).
4. Confirm progress updates and eventual status change.

### Backend checks
- File written to MEDIA_ORIGINALS_PATH.
- DB row inserted in movies with transcoding_status = pending.
- HLS output appears under MEDIA_HLS_PATH/{movie_id}/.
- Thumbnail created in MEDIA_THUMBNAILS_PATH/{movie_id}.jpg.
- transcode.log created in the HLS folder.

### Edge cases
- Upload a large file near MAX_UPLOAD_GB.
- Upload a file with unsupported container.
- Interrupt upload mid-way and confirm retry behavior.

## Phase 3: Stream REST + WebSocket
### Stream creation
1. In /stream/admin, confirm streamId is created.
2. Verify POST /api/streams/create returns streamId.
3. Verify streams table has status starting.

### Start playback
1. Select a movie and click Play to Stream.
2. Verify POST /api/streams/{stream_id}/start returns 200.
3. Verify streams.status = live, current_movie_id set, is_playing = true.

### Viewer sync
1. Open /stream/{stream_id} in two browsers.
2. Confirm viewerCount updates.
3. Play/pause/seek from host; verify viewer sync.

## Phase 4: Camera Streaming
1. In /stream/admin, click Share Camera.
2. Verify POST /api/streams/{stream_id}/camera/start returns ws_url + hls_url.
3. Confirm /hls/camera-{stream_id}/live.m3u8 loads.
4. Join /stream/{stream_id} and confirm playback.

## Frontend UI Checks
- Streams list loads and refreshes every 10s.
- Chat panel opens in dashboard/streams/room.
- Theme toggle persists across reloads.
- Snackbars and dialogs appear when expected.

## Troubleshooting
- If HLS fails, check transcode.log for FFmpeg errors.
- If WebSocket disconnects, verify backend is running and CORS allows frontend origin.
- If uploads fail, verify MEDIA_UPLOAD_TMP_PATH and MAX_UPLOAD_* settings.
