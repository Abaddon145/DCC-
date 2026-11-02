-- Provenance identities do not authorize reading an external source.
CREATE TABLE learning_source_profiles (
 id TEXT PRIMARY KEY CHECK(length(id)=36),
 application TEXT NOT NULL CHECK(application='yingliu'),
 access_epoch TEXT NOT NULL CHECK(length(access_epoch)=36),
 created_at TEXT NOT NULL CHECK(length(created_at)>0)
);
