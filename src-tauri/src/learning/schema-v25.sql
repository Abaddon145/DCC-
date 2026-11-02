CREATE TABLE IF NOT EXISTS learning_subtitles (
 id TEXT PRIMARY KEY,
 course_id TEXT NOT NULL REFERENCES learning_courses(id) ON DELETE CASCADE,
 name TEXT NOT NULL,format TEXT NOT NULL CHECK(format IN ('srt','vtt')),
 relative_path TEXT NOT NULL UNIQUE,sha256 TEXT NOT NULL,
 cue_count INTEGER NOT NULL CHECK(cue_count>0),created_at TEXT NOT NULL,
 UNIQUE(course_id,sha256)
);
CREATE TABLE IF NOT EXISTS learning_subtitle_cues (
 id TEXT PRIMARY KEY,
 subtitle_id TEXT NOT NULL REFERENCES learning_subtitles(id) ON DELETE CASCADE,
 ordinal INTEGER NOT NULL CHECK(ordinal>=0),
 start_ms INTEGER NOT NULL CHECK(start_ms>=0),end_ms INTEGER NOT NULL CHECK(end_ms>start_ms),
 text TEXT NOT NULL CHECK(length(text)>0),UNIQUE(subtitle_id,ordinal)
);
