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
    assigned_hat TEXT,             -- Agent/hat identifier for this ticket
    agent_backend TEXT,            -- Agent backend to use (e.g., 'claude', 'opencode')
    parent_id TEXT REFERENCES tickets(id) ON DELETE SET NULL,
    acceptance_criteria TEXT,      -- Acceptance criteria for the ticket
    check_command TEXT,            -- Command to run for verification
    review_summary TEXT,           -- Summary from code review
    checks_passed INTEGER CHECK(checks_passed IN (0, 1)),  -- NULL=not run, 0=false, 1=true
    session_active INTEGER DEFAULT 0 CHECK(session_active IN (0, 1)),  -- 0=false, 1=true
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

-- Knowledge base for learnings from completed tickets
CREATE TABLE IF NOT EXISTS knowledge (
    ticket_id TEXT PRIMARY KEY,
    summary TEXT NOT NULL,
    tags TEXT DEFAULT '',
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- FTS5 index for full-text search on knowledge
CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_fts USING fts5(
    ticket_id,
    summary,
    tags,
    content='knowledge',
    content_rowid='rowid',
    tokenize='porter'
);

-- Triggers to keep FTS in sync
CREATE TRIGGER IF NOT EXISTS knowledge_ai AFTER INSERT ON knowledge BEGIN
    INSERT INTO knowledge_fts(rowid, ticket_id, summary, tags)
    VALUES (NEW.rowid, NEW.ticket_id, NEW.summary, NEW.tags);
END;

CREATE TRIGGER IF NOT EXISTS knowledge_ad AFTER DELETE ON knowledge BEGIN
    INSERT INTO knowledge_fts(knowledge_fts, rowid, ticket_id, summary, tags)
    VALUES ('delete', OLD.rowid, OLD.ticket_id, OLD.summary, OLD.tags);
END;

CREATE TRIGGER IF NOT EXISTS knowledge_au AFTER UPDATE ON knowledge BEGIN
    INSERT INTO knowledge_fts(knowledge_fts, rowid, ticket_id, summary, tags)
    VALUES ('delete', OLD.rowid, OLD.ticket_id, OLD.summary, OLD.tags);
    INSERT INTO knowledge_fts(rowid, ticket_id, summary, tags)
    VALUES (NEW.rowid, NEW.ticket_id, NEW.summary, NEW.tags);
END;

-- Scratchpads for agent iteration memory
CREATE TABLE IF NOT EXISTS scratchpads (
    ticket_id TEXT PRIMARY KEY REFERENCES tickets(id) ON DELETE CASCADE,
    content TEXT DEFAULT '',
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
