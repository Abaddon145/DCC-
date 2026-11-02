-- Keep old revert records while allowing bounded large course revert reports.
CREATE TABLE learning_import_reverts_v39 (
  batch_id TEXT PRIMARY KEY REFERENCES learning_import_batches(id) ON DELETE CASCADE,
  original_request_sha256 TEXT NOT NULL CHECK(length(original_request_sha256)=64),
  reverted_at TEXT NOT NULL,
  report_json TEXT NOT NULL CHECK(length(CAST(report_json AS BLOB))<=2097152)
);
INSERT INTO learning_import_reverts_v39 SELECT batch_id,original_request_sha256,reverted_at,report_json FROM learning_import_reverts;
DROP TABLE learning_import_reverts;
ALTER TABLE learning_import_reverts_v39 RENAME TO learning_import_reverts;
CREATE INDEX IF NOT EXISTS learning_course_import_entries_target
  ON learning_course_import_entries(entry_kind,target_id,status);
