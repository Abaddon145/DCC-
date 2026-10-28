-- Identity comparison only; this grants no access to external files.
CREATE TABLE learning_source_vault_bindings (
 source_profile_id TEXT NOT NULL REFERENCES learning_source_profiles(id) ON DELETE RESTRICT,
 vault_id TEXT NOT NULL CHECK(length(CAST(vault_id AS BLOB)) BETWEEN 1 AND 4096),
 root_identity TEXT NOT NULL CHECK(length(root_identity) BETWEEN 1 AND 128),
 PRIMARY KEY(source_profile_id,vault_id)
);
