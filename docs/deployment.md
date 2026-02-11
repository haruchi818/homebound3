# Deployment Guide

## Backend
### Environment variables
- BIND_ADDR
- FRONTEND_URL
- SESSION_KEY (>= 64 bytes)
- SESSION_SECURE=true in production
- DATABASE_URL
- MEDIA_ORIGINALS_PATH
- MEDIA_HLS_PATH
- MEDIA_THUMBNAILS_PATH
- MEDIA_SUBTITLES_PATH
- MEDIA_UPLOAD_TMP_PATH
- MAX_UPLOAD_GB (or MAX_UPLOAD_BYTES)
- ALLOWED_VIDEO_FORMATS
- FFMPEG_PATH (optional)

### System requirements
- FFmpeg installed (ffmpeg + ffprobe)
- PostgreSQL running and reachable
- Large disk volume for MEDIA_* paths

### Suggested systemd service (Linux)
- Run backend with a dedicated user
- Ensure MEDIA_* paths are writable
- Configure log rotation for HLS/transcode logs

## Frontend
### Build
- npm install
- npm run build
- Serve build output from adapter (default auto)

### Environment
- VITE_API_BASE pointing to backend

## Reverse proxy (Caddy outline)
- Proxy /api and /ws to backend
- Serve frontend assets
- Enable gzip and caching for HLS segments

## SSL
- Terminate TLS at proxy
- Use wss for WebSocket connections

## Media storage
- Mount large volume at /var/media
- Periodic cleanup of old HLS camera segments
- Optional backup strategy for originals

## Monitoring
- Track CPU usage during FFmpeg transcodes
- Alert on disk usage
- Log HLS/transcode failures
