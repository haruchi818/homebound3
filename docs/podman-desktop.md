# Podman Desktop (Windows) Deployment

## Prereqs
- Podman Desktop installed and a Podman machine running
- Docker compatibility enabled in Podman Desktop

## Compose setup
This repo includes a compose file at the root: compose.yml.

### Bring up the stack
From the repo root:
- docker-compose up -d --build

### Run migrations
Migrations are not run automatically in the container. Run them locally:
- cd backend
- sqlx migrate run

### Stop the stack
- docker-compose down

## Notes
- Ports exposed: 5173 (frontend), 8080 (backend), 5432 (Postgres)
- Media storage is a named volume (media_data)
- Update SESSION_KEY before production use
- Update FRONTEND_URL if you change ports or domain
