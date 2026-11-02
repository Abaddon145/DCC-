CREATE TABLE IF NOT EXISTS learning_note_previews (
 id TEXT PRIMARY KEY,
 archive_id TEXT NOT NULL REFERENCES learning_note_archives(id),
 identity_key TEXT NOT NULL CHECK(length(identity_key)=64),
 payload_json TEXT NOT NULL CHECK(length(CAST(payload_json AS BLOB))<=16777216),
 payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256)=64),
 status TEXT NOT NULL CHECK(status IN ('ready','committed','cancelled')),
 receipt_json TEXT,
 created_at TEXT NOT NULL,
 committed_at TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_learning_note_preview_identity
 ON learning_note_previews(identity_key) WHERE status!='cancelled';
