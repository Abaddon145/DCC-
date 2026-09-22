CREATE TABLE IF NOT EXISTS schema_meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS categories (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL COLLATE NOCASE,
  parent_id TEXT REFERENCES categories(id) ON DELETE RESTRICT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  deleted_at TEXT,
  delete_batch_id TEXT,
  UNIQUE(parent_id, name)
);

CREATE TABLE IF NOT EXISTS assets (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  category_id TEXT REFERENCES categories(id) ON DELETE SET NULL,
  dcc_tools_json TEXT NOT NULL DEFAULT '[]',
  versions_json TEXT NOT NULL DEFAULT '[]',
  formats_json TEXT NOT NULL DEFAULT '[]',
  size_bytes INTEGER,
  author TEXT NOT NULL DEFAULT '',
  source_url TEXT NOT NULL DEFAULT '',
  fab_listing_id TEXT NOT NULL DEFAULT '',
  license TEXT NOT NULL DEFAULT '',
  share_url TEXT NOT NULL,
  normalized_share_url TEXT NOT NULL DEFAULT '',
  link_check_status TEXT NOT NULL DEFAULT 'unknown' CHECK(link_check_status IN ('unknown','valid','invalid','error')),
  link_checked_at TEXT,
  link_check_message TEXT NOT NULL DEFAULT '',
  extraction_code TEXT NOT NULL DEFAULT '',
  favorite INTEGER NOT NULL DEFAULT 0,
  rating INTEGER NOT NULL DEFAULT 0 CHECK(rating BETWEEN 0 AND 5),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  last_viewed_at TEXT,
  deleted_at TEXT,
  delete_batch_id TEXT
);

CREATE INDEX IF NOT EXISTS idx_assets_category ON assets(category_id);
CREATE INDEX IF NOT EXISTS idx_assets_updated ON assets(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_assets_favorite ON assets(favorite, updated_at DESC);
CREATE TABLE IF NOT EXISTS deletion_batches (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL CHECK(kind IN ('assets','category','media','mediaFolder')),
  label TEXT NOT NULL,
  asset_count INTEGER NOT NULL DEFAULT 0,
  category_count INTEGER NOT NULL DEFAULT 0,
  media_count INTEGER NOT NULL DEFAULT 0,
  media_folder_count INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tags (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  normalized_name TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS asset_tags (
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY(asset_id, tag_id)
);

CREATE TABLE IF NOT EXISTS images (
  id TEXT PRIMARY KEY,
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  original_name TEXT NOT NULL,
  original_rel_path TEXT NOT NULL,
  thumbnail_rel_path TEXT NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0,
  is_cover INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_images_asset ON images(asset_id, sort_order);

CREATE TABLE IF NOT EXISTS asset_media (
  id TEXT PRIMARY KEY,
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK(kind IN ('video','audio','model')),
  original_name TEXT NOT NULL,
  original_rel_path TEXT NOT NULL,
  proxy_rel_path TEXT,
  thumbnail_rel_path TEXT,
  mime_type TEXT NOT NULL,
  file_size INTEGER NOT NULL DEFAULT 0,
  duration_ms INTEGER,
  pixel_width INTEGER,
  pixel_height INTEGER,
  sort_order INTEGER NOT NULL DEFAULT 0,
  is_cover INTEGER NOT NULL DEFAULT 0,
  processing_status TEXT NOT NULL DEFAULT 'pending' CHECK(processing_status IN ('pending','processing','ready','error')),
  processing_message TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_asset_media_asset ON asset_media(asset_id, sort_order);
CREATE VIRTUAL TABLE IF NOT EXISTS asset_search USING fts5(asset_id UNINDEXED, text, tokenize='trigram');

CREATE TABLE IF NOT EXISTS asset_localizations (
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  locale TEXT NOT NULL CHECK(locale IN ('zh-CN','en')),
  name TEXT NOT NULL DEFAULT '',
  description TEXT NOT NULL DEFAULT '',
  license TEXT NOT NULL DEFAULT '',
  PRIMARY KEY(asset_id, locale)
);

CREATE TABLE IF NOT EXISTS localized_tags (
  id TEXT PRIMARY KEY,
  locale TEXT NOT NULL CHECK(locale IN ('zh-CN','en')),
  name TEXT NOT NULL,
  normalized_name TEXT NOT NULL,
  UNIQUE(locale, normalized_name)
);

CREATE TABLE IF NOT EXISTS asset_localized_tags (
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  tag_id TEXT NOT NULL REFERENCES localized_tags(id) ON DELETE CASCADE,
  PRIMARY KEY(asset_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_asset_localizations_locale ON asset_localizations(locale, asset_id);
CREATE INDEX IF NOT EXISTS idx_localized_tags_locale ON localized_tags(locale, normalized_name);

CREATE TABLE IF NOT EXISTS reference_boards (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  background TEXT NOT NULL DEFAULT '#15191f',
  view_x REAL NOT NULL DEFAULT 0,
  view_y REAL NOT NULL DEFAULT 0,
  view_scale REAL NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  last_opened_at TEXT
);

CREATE TABLE IF NOT EXISTS reference_board_items (
  id TEXT PRIMARY KEY,
  board_id TEXT NOT NULL REFERENCES reference_boards(id) ON DELETE CASCADE,
  original_name TEXT NOT NULL,
  original_rel_path TEXT NOT NULL,
  thumbnail_rel_path TEXT NOT NULL,
  pixel_width INTEGER NOT NULL,
  pixel_height INTEGER NOT NULL,
  x REAL NOT NULL,
  y REAL NOT NULL,
  width REAL NOT NULL,
  height REAL NOT NULL,
  rotation REAL NOT NULL DEFAULT 0,
  z_index INTEGER NOT NULL DEFAULT 0,
  source_asset_id TEXT,
  source_image_id TEXT,
  deleted_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reference_board_items_board ON reference_board_items(board_id, z_index);

CREATE TABLE IF NOT EXISTS smart_collections (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL COLLATE NOCASE UNIQUE,
  icon TEXT NOT NULL DEFAULT 'sparkles',
  color TEXT NOT NULL DEFAULT '#D99A42',
  rule_json TEXT NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_smart_collections_sort ON smart_collections(sort_order, name);

CREATE TABLE IF NOT EXISTS library_preferences (
  id INTEGER PRIMARY KEY CHECK(id=1),
  settings_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  project_type TEXT NOT NULL CHECK(project_type IN ('still','scene','animation')),
  status TEXT NOT NULL CHECK(status IN ('planning','active','paused','completed','archived')),
  target_tools_json TEXT NOT NULL DEFAULT '[]',
  versions_json TEXT NOT NULL DEFAULT '[]',
  resolution_width INTEGER,
  resolution_height INTEGER,
  frame_rate REAL,
  cover_asset_id TEXT REFERENCES assets(id) ON DELETE SET NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  last_opened_at TEXT,
  archived_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_projects_status ON projects(status, updated_at DESC);

CREATE TABLE IF NOT EXISTS project_units (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  parent_id TEXT REFERENCES project_units(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK(kind IN ('scene','shot')),
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  start_frame INTEGER,
  end_frame INTEGER,
  resolution_width INTEGER,
  resolution_height INTEGER,
  frame_rate REAL,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_project_units_parent ON project_units(project_id, parent_id, sort_order);

CREATE TABLE IF NOT EXISTS project_tasks (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  unit_id TEXT REFERENCES project_units(id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL CHECK(status IN ('todo','in_progress','review','done')),
  priority TEXT NOT NULL CHECK(priority IN ('low','normal','high','urgent')),
  due_date TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_project_tasks_column ON project_tasks(project_id, status, sort_order);

CREATE TABLE IF NOT EXISTS project_assets (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  status TEXT NOT NULL DEFAULT 'candidate' CHECK(status IN ('candidate','selected','used','rejected')),
  purpose TEXT NOT NULL DEFAULT '',
  note TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY(project_id, asset_id)
);

CREATE INDEX IF NOT EXISTS idx_project_assets_status ON project_assets(project_id, status, updated_at DESC);

CREATE TABLE IF NOT EXISTS project_asset_units (
  project_id TEXT NOT NULL,
  asset_id TEXT NOT NULL,
  unit_id TEXT NOT NULL REFERENCES project_units(id) ON DELETE CASCADE,
  PRIMARY KEY(project_id, asset_id, unit_id),
  FOREIGN KEY(project_id, asset_id) REFERENCES project_assets(project_id, asset_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS project_task_assets (
  task_id TEXT NOT NULL REFERENCES project_tasks(id) ON DELETE CASCADE,
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  PRIMARY KEY(task_id, asset_id)
);

CREATE TABLE IF NOT EXISTS project_reference_boards (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  board_id TEXT NOT NULL REFERENCES reference_boards(id) ON DELETE CASCADE,
  is_main INTEGER NOT NULL DEFAULT 0,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  PRIMARY KEY(project_id, board_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_project_main_board ON project_reference_boards(project_id) WHERE is_main=1;

CREATE TABLE IF NOT EXISTS project_paths (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK(kind IN ('root','project_file','output','custom')),
  label TEXT NOT NULL,
  path TEXT NOT NULL,
  path_type TEXT NOT NULL CHECK(path_type IN ('file','directory')),
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_project_paths_sort ON project_paths(project_id, sort_order);

CREATE TABLE IF NOT EXISTS media_folders (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL CHECK(kind IN ('image','model','audio','video')),
  parent_id TEXT REFERENCES media_folders(id) ON DELETE RESTRICT,
  name TEXT NOT NULL COLLATE NOCASE,
  sort_order INTEGER NOT NULL DEFAULT 0,
  deleted_at TEXT,
  delete_batch_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(kind,parent_id,name)
);
CREATE INDEX IF NOT EXISTS idx_media_folders_tree ON media_folders(kind,parent_id,sort_order,name);

CREATE TABLE IF NOT EXISTS media_entries (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL CHECK(kind IN ('image','model','audio','video')),
  folder_id TEXT REFERENCES media_folders(id) ON DELETE SET NULL,
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  author TEXT NOT NULL DEFAULT '',
  source_url TEXT NOT NULL DEFAULT '',
  license TEXT NOT NULL DEFAULT '',
  favorite INTEGER NOT NULL DEFAULT 0,
  processing_status TEXT NOT NULL DEFAULT 'ready',
  processing_message TEXT NOT NULL DEFAULT '',
  primary_file_id TEXT NOT NULL,
  deleted_at TEXT,
  delete_batch_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_media_entries_kind ON media_entries(kind,deleted_at,updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_media_entries_folder ON media_entries(folder_id,deleted_at,updated_at DESC);

CREATE TABLE IF NOT EXISTS media_files (
  id TEXT PRIMARY KEY,
  entry_id TEXT NOT NULL REFERENCES media_entries(id) ON DELETE CASCADE,
  role TEXT NOT NULL CHECK(role IN ('main','dependency','proxy','thumbnail','waveform')),
  logical_path TEXT NOT NULL,
  original_name TEXT NOT NULL,
  rel_path TEXT NOT NULL UNIQUE,
  mime_type TEXT NOT NULL DEFAULT 'application/octet-stream',
  file_size INTEGER NOT NULL DEFAULT 0,
  checksum TEXT NOT NULL DEFAULT '',
  width INTEGER,
  height INTEGER,
  duration_ms INTEGER,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  UNIQUE(entry_id,logical_path)
);
CREATE INDEX IF NOT EXISTS idx_media_files_entry ON media_files(entry_id,role,sort_order);
CREATE UNIQUE INDEX IF NOT EXISTS idx_media_files_main_checksum ON media_files(checksum) WHERE role='main' AND checksum<>'';

CREATE TABLE IF NOT EXISTS media_entry_tags (
  entry_id TEXT NOT NULL REFERENCES media_entries(id) ON DELETE CASCADE,
  tag_id TEXT NOT NULL REFERENCES localized_tags(id) ON DELETE CASCADE,
  PRIMARY KEY(entry_id,tag_id)
);
CREATE TABLE IF NOT EXISTS media_entry_assets (
  entry_id TEXT NOT NULL REFERENCES media_entries(id) ON DELETE CASCADE,
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  PRIMARY KEY(entry_id,asset_id)
);
CREATE TABLE IF NOT EXISTS project_media_entries (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  entry_id TEXT NOT NULL REFERENCES media_entries(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL,
  PRIMARY KEY(project_id,entry_id)
);

CREATE VIRTUAL TABLE IF NOT EXISTS media_search USING fts5(entry_id UNINDEXED,text,tokenize='trigram');
