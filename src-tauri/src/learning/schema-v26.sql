CREATE TABLE IF NOT EXISTS learning_analysis_previews (
 id TEXT PRIMARY KEY,
 course_id TEXT NOT NULL REFERENCES learning_courses(id) ON DELETE CASCADE,
 fingerprint TEXT NOT NULL CHECK(length(fingerprint)=64),
 input_json TEXT NOT NULL,
 created_at INTEGER NOT NULL,expires_at INTEGER NOT NULL CHECK(expires_at>created_at),
 consumed_at INTEGER
);
CREATE INDEX IF NOT EXISTS learning_analysis_preview_course ON learning_analysis_previews(course_id,created_at DESC);
CREATE TABLE IF NOT EXISTS learning_analyses (
 id TEXT PRIMARY KEY,
 course_id TEXT NOT NULL REFERENCES learning_courses(id) ON DELETE CASCADE,
 preview_id TEXT NOT NULL UNIQUE,
 source_fingerprint TEXT NOT NULL CHECK(length(source_fingerprint)=64),
 model TEXT NOT NULL,template_id TEXT NOT NULL,template_version INTEGER NOT NULL,
 prompt_version TEXT NOT NULL,format_version INTEGER NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('queued','running','partial','completed','failed','cancelled')),
 result_json TEXT,error TEXT NOT NULL DEFAULT '',created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS learning_analysis_course ON learning_analyses(course_id,created_at DESC);
CREATE INDEX IF NOT EXISTS learning_analysis_fingerprint ON learning_analyses(course_id,source_fingerprint);
CREATE TABLE IF NOT EXISTS learning_analysis_stages (
 analysis_id TEXT NOT NULL REFERENCES learning_analyses(id) ON DELETE CASCADE,
 stage_key TEXT NOT NULL,start_ms INTEGER NOT NULL CHECK(start_ms>=0),
 end_ms INTEGER NOT NULL CHECK(end_ms>start_ms),input_hash TEXT NOT NULL CHECK(length(input_hash)=64),
 status TEXT NOT NULL CHECK(status IN ('pending','completed','failed')),
 result_json TEXT,tokens_used INTEGER CHECK(tokens_used IS NULL OR tokens_used>=0),
 error TEXT NOT NULL DEFAULT '',updated_at INTEGER NOT NULL,
 PRIMARY KEY(analysis_id,stage_key)
);
