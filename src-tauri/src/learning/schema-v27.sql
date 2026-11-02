CREATE TABLE IF NOT EXISTS learning_note_archives (
 id TEXT PRIMARY KEY,
 source_profile_id TEXT NOT NULL,
 source_key TEXT NOT NULL CHECK(length(source_key)>0),
 version_key TEXT NOT NULL CHECK(length(version_key)>0),
 sha256 TEXT NOT NULL CHECK(length(sha256)=64),
 relative_path TEXT NOT NULL UNIQUE,
 size INTEGER NOT NULL CHECK(size>=0 AND size<=5242880),
 created_at TEXT NOT NULL,
 UNIQUE(source_profile_id,source_key,version_key,sha256)
);
CREATE TABLE IF NOT EXISTS learning_note_archive_items (
 id TEXT PRIMARY KEY,
 archive_id TEXT NOT NULL REFERENCES learning_note_archives(id),
 start_byte INTEGER NOT NULL CHECK(start_byte>=0),
 end_byte INTEGER NOT NULL CHECK(end_byte>start_byte AND end_byte<=5242880),
 sha256 TEXT NOT NULL CHECK(length(sha256)=64),
 label TEXT NOT NULL,
 UNIQUE(archive_id,start_byte,end_byte)
);
