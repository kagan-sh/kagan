-- Kagan Database Schema
-- SQLite with WAL mode for concurrent read/write

-- Enable WAL mode for better concurrency
PRAGMA journal_mode=WAL;

-- Tickets table - core entity for Kanban board
CREATE TABLE IF NOT EXISTS tickets (
    id TEXT PRIMARY KEY,           -- UUID v4
    title TEXT NOT NULL,
    description TEXT,
    status TEXT DEFAULT 'BACKLOG' CHECK(status IN ('BACKLOG', 'IN_PROGRESS', 'REVIEW', 'DONE')),
    priority INTEGER DEFAULT 1 CHECK(priority IN (0, 1, 2)),  -- 0=low, 1=medium, 2=high
    parent_id TEXT REFERENCES tickets(id) ON DELETE SET NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_tickets_status ON tickets(status);
CREATE INDEX IF NOT EXISTS idx_tickets_priority ON tickets(priority DESC);
CREATE INDEX IF NOT EXISTS idx_tickets_parent ON tickets(parent_id);
CREATE INDEX IF NOT EXISTS idx_tickets_updated ON tickets(updated_at DESC);

-- Trigger to update updated_at on changes
CREATE TRIGGER IF NOT EXISTS update_tickets_timestamp
AFTER UPDATE ON tickets
FOR EACH ROW
BEGIN
    UPDATE tickets SET updated_at = CURRENT_TIMESTAMP WHERE id = OLD.id;
END;
