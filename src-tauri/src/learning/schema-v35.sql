-- Per-batch revert journal. A revert never deletes user-edited, reused, or conflicting
-- targets and never removes raw archives; created notes go to the knowledge trash.
CREATE TABLE IF NOT EXISTS learning_import_reverts (
  batch_id TEXT PRIMARY KEY REFERENCES learning_import_batches(id) ON DELETE CASCADE,
  original_request_sha256 TEXT NOT NULL CHECK(length(original_request_sha256)=64),
  reverted_at TEXT NOT NULL,
  report_json TEXT NOT NULL CHECK(length(CAST(report_json AS BLOB))<=65536)
);
CREATE TABLE IF NOT EXISTS learning_import_item_reverts (
  item_id TEXT PRIMARY KEY REFERENCES learning_import_items(id) ON DELETE CASCADE,
  batch_id TEXT NOT NULL REFERENCES learning_import_batches(id) ON DELETE CASCADE,
  outcome TEXT NOT NULL CHECK(outcome IN ('reverted','kept_modified','kept_reused','kept_conflict','kept_missing')),
  note_id TEXT CHECK(note_id IS NULL OR length(note_id)=36),
  note_revision INTEGER,
  reverted_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS learning_import_item_reverts_batch ON learning_import_item_reverts(batch_id);