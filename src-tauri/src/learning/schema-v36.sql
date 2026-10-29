ALTER TABLE learning_import_batches ADD COLUMN batch_kind TEXT NOT NULL DEFAULT 'notes' CHECK(batch_kind IN ('notes','courses'));
