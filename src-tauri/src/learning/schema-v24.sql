CREATE TABLE IF NOT EXISTS learning_courses (
 id TEXT PRIMARY KEY,
 media_entry_id TEXT NOT NULL UNIQUE REFERENCES media_entries(id) ON DELETE CASCADE,
 title_override TEXT NOT NULL DEFAULT '',
 position_ms INTEGER NOT NULL DEFAULT 0 CHECK(position_ms>=0),
 duration_ms INTEGER CHECK(duration_ms IS NULL OR duration_ms>=0),
 rate REAL NOT NULL DEFAULT 1 CHECK(rate BETWEEN 0.25 AND 4),
 completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN (0,1)),
 progress_revision INTEGER NOT NULL DEFAULT 0 CHECK(progress_revision>=0),
 created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS learning_courses_updated ON learning_courses(updated_at DESC,id);
CREATE TABLE IF NOT EXISTS learning_series (
 id TEXT PRIMARY KEY, name TEXT NOT NULL CHECK(length(trim(name))>0), description TEXT NOT NULL DEFAULT '',
 revision INTEGER NOT NULL DEFAULT 0 CHECK(revision>=0), created_at TEXT NOT NULL,updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS learning_series_courses (
 series_id TEXT NOT NULL REFERENCES learning_series(id) ON DELETE CASCADE,
 course_id TEXT NOT NULL REFERENCES learning_courses(id) ON DELETE CASCADE,
 position INTEGER NOT NULL CHECK(position>=0), PRIMARY KEY(series_id,course_id), UNIQUE(series_id,position)
);
