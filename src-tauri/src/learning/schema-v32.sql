-- Provenance does not authorize source access and survives target deletion.
CREATE TABLE learning_origin_map (
 source_profile_id TEXT NOT NULL REFERENCES learning_source_profiles(id) ON DELETE RESTRICT,
 source_key_json TEXT NOT NULL CHECK(length(CAST(source_key_json AS BLOB))<=65536),
 target_kind TEXT NOT NULL,
 target_id TEXT NOT NULL CHECK(length(target_id)=36),
 source_sha256 TEXT NOT NULL CHECK(length(source_sha256)=64),
 target_revision INTEGER NOT NULL CHECK(target_revision>=0),
 target_sha256 TEXT NOT NULL CHECK(length(target_sha256)=64),
 PRIMARY KEY(source_profile_id,source_key_json)
);
CREATE UNIQUE INDEX learning_origin_one_note ON learning_origin_map(target_id) WHERE target_kind='note';
