-- migrate:up
-- Alpha schema rationalization:
-- `tags`, `task_tags`, and `images` are no longer part of the runtime model.
-- This migration removes those redundant tables.
PRAGMA foreign_keys=OFF;

DROP TABLE IF EXISTS task_tags;
DROP TABLE IF EXISTS images;
DROP TABLE IF EXISTS tags;

PRAGMA foreign_keys=ON;

-- migrate:down
-- Recreate removed tables for rollback/testing compatibility.
PRAGMA foreign_keys=OFF;

CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    color TEXT,
    created_at DATETIME NOT NULL
);

CREATE TABLE IF NOT EXISTS task_tags (
    task_id TEXT NOT NULL REFERENCES tasks(id),
    tag_id TEXT NOT NULL REFERENCES tags(id),
    PRIMARY KEY (task_id, tag_id)
);

CREATE TABLE IF NOT EXISTS images (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    uri TEXT NOT NULL,
    caption TEXT,
    created_at DATETIME NOT NULL
);

CREATE INDEX IF NOT EXISTS ix_images_task_id ON images (task_id);

PRAGMA foreign_keys=ON;
