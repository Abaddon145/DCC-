CREATE TABLE IF NOT EXISTS ai_sessions (
 id TEXT PRIMARY KEY,
 name TEXT NOT NULL,
 kind TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'draft',
 request_json TEXT NOT NULL,
 context_json TEXT NOT NULL,
 output_json TEXT,
 message TEXT NOT NULL DEFAULT '',
 created_at TEXT NOT NULL,
 updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ai_sessions_updated ON ai_sessions(updated_at DESC);
CREATE TABLE IF NOT EXISTS ai_last_undo (
 slot INTEGER PRIMARY KEY CHECK(slot=1),
 session_id TEXT NOT NULL,
 library_id TEXT NOT NULL,
 before_json TEXT NOT NULL,
 after_json TEXT NOT NULL,
 created_json TEXT NOT NULL,
 created_at TEXT NOT NULL
);
