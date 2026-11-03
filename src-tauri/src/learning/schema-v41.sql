CREATE TABLE IF NOT EXISTS learning_analysis_templates (
 id TEXT PRIMARY KEY,
 name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 80),
 focus TEXT NOT NULL CHECK(length(CAST(focus AS BLOB)) BETWEEN 1 AND 4000),
 version INTEGER NOT NULL CHECK(version > 0),
 updated_at TEXT NOT NULL
);
