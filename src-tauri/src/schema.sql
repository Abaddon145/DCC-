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
  license TEXT NOT NULL DEFAULT '',
  share_url TEXT NOT NULL,
  normalized_share_url TEXT NOT NULL DEFAULT '',
  link_check_status TEXT NOT NULL DEFAULT 'unknown' CHECK(link_check_status IN ('unknown','valid','invalid','error')),
  link_checked_at TEXT,
  link_check_message TEXT NOT NULL DEFAULT '',
  extraction_code TEXT NOT NULL DEFAULT '',
  favorite INTEGER NOT NULL DEFAULT 0,
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
  kind TEXT NOT NULL CHECK(kind IN ('assets','category')),
  label TEXT NOT NULL,
  asset_count INTEGER NOT NULL DEFAULT 0,
  category_count INTEGER NOT NULL DEFAULT 0,
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
