-- Course-batch decision journal. A staged payload is not authorization to read a source database.
CREATE TABLE IF NOT EXISTS learning_course_import_stage (
  batch_id TEXT PRIMARY KEY REFERENCES learning_import_batches(id) ON DELETE CASCADE,
  source_fingerprint TEXT NOT NULL CHECK(length(source_fingerprint)=64),
  target_fingerprint TEXT NOT NULL CHECK(length(target_fingerprint)=64),
  payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256)=64),
  payload_json TEXT NOT NULL CHECK(length(CAST(payload_json AS BLOB)) BETWEEN 2 AND 1048576),
  staged_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS learning_course_import_entries (
  id TEXT PRIMARY KEY CHECK(length(id)=36),
  batch_id TEXT NOT NULL REFERENCES learning_import_batches(id) ON DELETE CASCADE,
  entry_kind TEXT NOT NULL CHECK(entry_kind IN ('series','course')),
  source_id TEXT NOT NULL CHECK(length(source_id) BETWEEN 1 AND 1024),
  status TEXT NOT NULL CHECK(status IN ('committed','conflict')),
  conflict_code TEXT,
  target_id TEXT CHECK(target_id IS NULL OR length(target_id)=36),
  receipt_json TEXT NOT NULL CHECK(length(CAST(receipt_json AS BLOB)) BETWEEN 2 AND 65536),
  created_at TEXT NOT NULL,
  UNIQUE(batch_id, entry_kind, source_id),
  CHECK(
    (status='conflict' AND conflict_code IS NOT NULL AND length(conflict_code) BETWEEN 1 AND 64) OR
    (status='committed' AND conflict_code IS NULL)
  )
);
CREATE INDEX IF NOT EXISTS learning_course_import_entries_batch
  ON learning_course_import_entries(batch_id, entry_kind, source_id);
