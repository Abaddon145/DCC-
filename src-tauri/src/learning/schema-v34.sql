-- Durable import journal and runtime staging metadata. Does not grant source access.
CREATE TABLE IF NOT EXISTS learning_import_batches (
  id TEXT PRIMARY KEY CHECK(length(id)=36),
  library_id TEXT NOT NULL CHECK(length(library_id)=36),
  root_hash TEXT NOT NULL CHECK(length(root_hash)=64),
  source_profile_id TEXT NOT NULL REFERENCES learning_source_profiles(id) ON DELETE RESTRICT,
  source_epoch TEXT NOT NULL CHECK(length(source_epoch)=36),
  request_sha256 TEXT NOT NULL CHECK(length(request_sha256)=64),
  status TEXT NOT NULL CHECK(status IN ('pending','committed','cancelled')),
  created_at TEXT NOT NULL,
  finished_at TEXT,
  receipt_json TEXT CHECK(receipt_json IS NULL OR length(CAST(receipt_json AS BLOB))<=65536),
  CHECK(
    (status='pending' AND finished_at IS NULL AND receipt_json IS NULL) OR
    (status IN ('committed','cancelled') AND finished_at IS NOT NULL)
  ),
  CHECK(
    (status='committed' AND receipt_json IS NOT NULL) OR
    (status!='committed')
  )
);
CREATE UNIQUE INDEX IF NOT EXISTS learning_import_batch_request
  ON learning_import_batches(library_id, request_sha256)
  WHERE status IN ('pending','committed');
CREATE INDEX IF NOT EXISTS learning_import_batches_library
  ON learning_import_batches(library_id, created_at DESC, id);

CREATE TABLE IF NOT EXISTS learning_import_items (
  id TEXT PRIMARY KEY CHECK(length(id)=36),
  batch_id TEXT NOT NULL REFERENCES learning_import_batches(id) ON DELETE CASCADE,
  source_key_json TEXT NOT NULL CHECK(length(CAST(source_key_json AS BLOB))<=65536),
  source_sha256 TEXT NOT NULL CHECK(length(source_sha256)=64),
  target_kind TEXT NOT NULL CHECK(target_kind='note'),
  expected_target_id TEXT CHECK(expected_target_id IS NULL OR length(expected_target_id)=36),
  expected_target_revision INTEGER CHECK(expected_target_revision IS NULL OR expected_target_revision>=0),
  expected_target_sha256 TEXT CHECK(expected_target_sha256 IS NULL OR length(expected_target_sha256)=64),
  title TEXT NOT NULL CHECK(length(title)>0 AND length(title)<=512),
  document_json TEXT NOT NULL CHECK(length(CAST(document_json AS BLOB))<=16777216),
  stage_relative TEXT CHECK(stage_relative IS NULL OR (length(stage_relative)>0 AND length(stage_relative)<=1024)),
  status TEXT NOT NULL CHECK(status IN ('pending','staged','committed','cancelled','conflict')),
  conflict_code TEXT,
  target_id TEXT CHECK(target_id IS NULL OR length(target_id)=36),
  receipt_json TEXT CHECK(receipt_json IS NULL OR length(CAST(receipt_json AS BLOB))<=65536),
  created_at TEXT NOT NULL,
  UNIQUE(batch_id, source_key_json),
  CHECK(
    (expected_target_id IS NULL AND expected_target_revision IS NULL AND expected_target_sha256 IS NULL) OR
    (expected_target_id IS NOT NULL AND expected_target_revision IS NOT NULL AND expected_target_sha256 IS NOT NULL)
  ),
  CHECK(
    (status='staged' AND stage_relative IS NOT NULL) OR
    (status!='staged') OR
    (stage_relative IS NOT NULL)
  ),
  CHECK(
    (status='conflict' AND conflict_code IS NOT NULL) OR
    (status!='conflict')
  )
);
CREATE INDEX IF NOT EXISTS learning_import_items_batch ON learning_import_items(batch_id, created_at, id);
