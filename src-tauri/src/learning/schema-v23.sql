CREATE TABLE IF NOT EXISTS media_external_files (
  file_id TEXT PRIMARY KEY REFERENCES media_files(id) ON DELETE CASCADE,
  canonical_path TEXT NOT NULL,
  size_bytes INTEGER NOT NULL CHECK(size_bytes>0),
  mtime_ms INTEGER NOT NULL,
  sha256 TEXT NOT NULL CHECK(length(sha256)=64),
  file_identity TEXT NOT NULL,
  volume_id TEXT NOT NULL DEFAULT '',
  revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>0),
  last_checked_at TEXT NOT NULL,
  access_epoch TEXT NOT NULL
);
CREATE TRIGGER IF NOT EXISTS media_external_video_only BEFORE INSERT ON media_external_files
WHEN NOT EXISTS(SELECT 1 FROM media_files f JOIN media_entries e ON e.id=f.entry_id
  WHERE f.id=NEW.file_id AND f.role='main' AND e.kind='video' AND f.rel_path='external-references/'||f.id)
BEGIN SELECT RAISE(ABORT,'external references require a registered main video'); END;
