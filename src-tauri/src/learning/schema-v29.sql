-- Snapshots and receipts survive deletion of a source or destination entity.
CREATE TABLE IF NOT EXISTS learning_excerpts (
 id TEXT PRIMARY KEY,
 library_id TEXT NOT NULL, root_hash TEXT NOT NULL CHECK(length(root_hash)=64),
 course_id TEXT NOT NULL, media_id TEXT NOT NULL, source_file_id TEXT NOT NULL,
 payload_json TEXT NOT NULL CHECK(length(CAST(payload_json AS BLOB))<=16777216),
 payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256)=64),
 status TEXT NOT NULL CHECK(status IN ('ready','cancelled','committed')),
 created_at TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_learning_excerpt_ready
 ON learning_excerpts(payload_sha256) WHERE status='ready';
CREATE INDEX IF NOT EXISTS idx_learning_excerpt_course ON learning_excerpts(course_id,created_at DESC,id);
CREATE TABLE IF NOT EXISTS learning_excerpt_operations (
 operation_id TEXT PRIMARY KEY,
 excerpt_id TEXT NOT NULL UNIQUE REFERENCES learning_excerpts(id),
 request_json TEXT NOT NULL CHECK(length(CAST(request_json AS BLOB))<=65536),
 request_sha256 TEXT NOT NULL CHECK(length(request_sha256)=64),
 status TEXT NOT NULL CHECK(status IN ('pending','committed','cancelled')),
 manifest_relative TEXT,
 receipt_json TEXT CHECK(length(CAST(receipt_json AS BLOB))<=65536),
 created_at TEXT NOT NULL,
 committed_at TEXT,
 CHECK((status='committed' AND receipt_json IS NOT NULL) OR
       (status!='committed' AND receipt_json IS NULL))
);
CREATE TABLE IF NOT EXISTS learning_course_notes (
 course_id TEXT NOT NULL REFERENCES learning_courses(id) ON DELETE CASCADE,
 note_id TEXT NOT NULL REFERENCES knowledge_notes(id) ON DELETE CASCADE,
 PRIMARY KEY(course_id,note_id)
);
CREATE INDEX IF NOT EXISTS idx_learning_course_notes_note ON learning_course_notes(note_id);
CREATE TABLE IF NOT EXISTS learning_excerpt_board_items (
 item_id TEXT PRIMARY KEY REFERENCES reference_board_items(id) ON DELETE CASCADE,
 excerpt_id TEXT NOT NULL UNIQUE REFERENCES learning_excerpts(id)
);
