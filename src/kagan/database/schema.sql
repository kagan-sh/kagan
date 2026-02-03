-- Kagan Database Schema
-- Version: 1 (tracked via PRAGMA user_version in migrations.py)
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
    ticket_type TEXT DEFAULT 'PAIR' CHECK(ticket_type IN ('AUTO', 'PAIR')),  -- AUTO=scheduler, PAIR=tmux
    assigned_hat TEXT,             -- Agent/hat identifier for this ticket
    agent_backend TEXT,            -- Agent backend to use (e.g., 'claude', 'opencode')
    parent_id TEXT REFERENCES tickets(id) ON DELETE SET NULL,
    acceptance_criteria TEXT,      -- Acceptance criteria for the ticket
    review_summary TEXT,           -- Summary from code review
    checks_passed INTEGER CHECK(checks_passed IN (0, 1)),  -- NULL=not run, 0=false, 1=true
    session_active INTEGER DEFAULT 0 CHECK(session_active IN (0, 1)),  -- 0=false, 1=true
    total_iterations INTEGER DEFAULT 0,  -- Lifetime iteration counter (monotonically increasing)
    merge_failed INTEGER DEFAULT 0 CHECK(merge_failed IN (0, 1)),  -- 0=false, 1=true
    merge_error TEXT,  -- Error message when merge fails
    merge_readiness TEXT DEFAULT 'risk' CHECK(merge_readiness IN ('ready', 'risk', 'blocked')),
    last_error TEXT,  -- Last runtime error from agent
    block_reason TEXT,  -- Reason when agent signals <blocked/>
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

-- Scratchpads for agent iteration memory
CREATE TABLE IF NOT EXISTS scratchpads (
    ticket_id TEXT PRIMARY KEY REFERENCES tickets(id) ON DELETE CASCADE,
    content TEXT DEFAULT '',
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Agent execution logs (implementation and review phases)
CREATE TABLE IF NOT EXISTS agent_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ticket_id TEXT NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    log_type TEXT NOT NULL CHECK(log_type IN ('implementation', 'review')),
    iteration INTEGER DEFAULT 1,
    content TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_agent_logs_ticket ON agent_logs(ticket_id, log_type, iteration);

-- Ticket event audit trail
CREATE TABLE IF NOT EXISTS ticket_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ticket_id TEXT NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    message TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_ticket_events_ticket ON ticket_events(ticket_id, created_at DESC);
