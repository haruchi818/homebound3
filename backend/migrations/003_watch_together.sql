DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'transcoding_status') THEN
    CREATE TYPE transcoding_status AS ENUM ('pending', 'processing', 'ready', 'failed');
  END IF;
END $$;

DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'stream_status') THEN
    CREATE TYPE stream_status AS ENUM ('starting', 'live', 'ended');
  END IF;
END $$;

DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'stream_type') THEN
    CREATE TYPE stream_type AS ENUM ('movie', 'camera');
  END IF;
END $$;

ALTER TABLE movies
  ADD COLUMN IF NOT EXISTS hls_path TEXT,
  ADD COLUMN IF NOT EXISTS transcoding_status transcoding_status NOT NULL DEFAULT 'pending',
  ADD COLUMN IF NOT EXISTS duration_seconds INTEGER,
  ADD COLUMN IF NOT EXISTS file_size_bytes BIGINT,
  ADD COLUMN IF NOT EXISTS upload_date TIMESTAMPTZ NOT NULL DEFAULT now(),
  ADD COLUMN IF NOT EXISTS uploader_id UUID;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_constraint
    WHERE conname = 'movies_uploader_id_fkey'
  ) THEN
    ALTER TABLE movies
      ADD CONSTRAINT movies_uploader_id_fkey
      FOREIGN KEY (uploader_id) REFERENCES users(id);
  END IF;
END $$;

ALTER TABLE streams
  ALTER COLUMN user_id TYPE UUID USING user_id::uuid,
  ALTER COLUMN status TYPE stream_status USING status::stream_status;

ALTER TABLE streams
  ALTER COLUMN status SET DEFAULT 'starting',
  ADD COLUMN IF NOT EXISTS ended_at TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS "current_timestamp" DOUBLE PRECISION NOT NULL DEFAULT 0,
  ADD COLUMN IF NOT EXISTS is_playing BOOLEAN NOT NULL DEFAULT false,
  ADD COLUMN IF NOT EXISTS stream_type stream_type NOT NULL DEFAULT 'movie',
  ADD COLUMN IF NOT EXISTS viewer_count INTEGER NOT NULL DEFAULT 0;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_constraint
    WHERE conname = 'streams_user_id_fkey'
  ) THEN
    ALTER TABLE streams
      ADD CONSTRAINT streams_user_id_fkey
      FOREIGN KEY (user_id) REFERENCES users(id);
  END IF;
END $$;

CREATE TABLE IF NOT EXISTS stream_viewers (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  stream_id TEXT NOT NULL,
  user_id UUID NOT NULL,
  joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  left_at TIMESTAMPTZ,
  is_active BOOLEAN NOT NULL DEFAULT true,
  CONSTRAINT stream_viewers_stream_id_fkey
    FOREIGN KEY (stream_id) REFERENCES streams(stream_id),
  CONSTRAINT stream_viewers_user_id_fkey
    FOREIGN KEY (user_id) REFERENCES users(id)
);
