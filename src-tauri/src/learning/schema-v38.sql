-- Machine-local source identity only. This does not authorize reading the source.
CREATE TABLE IF NOT EXISTS learning_source_sqlite_bindings (
  source_profile_id TEXT PRIMARY KEY REFERENCES learning_source_profiles(id) ON DELETE RESTRICT,
  file_identity TEXT NOT NULL UNIQUE CHECK(length(file_identity) BETWEEN 1 AND 128)
);
