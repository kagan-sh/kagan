-- migrate:up
PRAGMA foreign_keys=OFF;

CREATE TABLE execution_process_logs_new (
    id TEXT PRIMARY KEY,
    execution_process_id TEXT NOT NULL REFERENCES execution_processes(id),
    logs TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    inserted_at DATETIME NOT NULL
);

INSERT INTO execution_process_logs_new (id, execution_process_id, logs, byte_size, inserted_at)
SELECT lower(hex(randomblob(4))), execution_id, logs, byte_size, inserted_at
FROM execution_process_logs;

DROP TABLE execution_process_logs;
ALTER TABLE execution_process_logs_new RENAME TO execution_process_logs;

CREATE INDEX IF NOT EXISTS ix_execution_process_logs_execution_process_id
    ON execution_process_logs (execution_process_id);
CREATE INDEX IF NOT EXISTS ix_execution_process_logs_inserted_at
    ON execution_process_logs (inserted_at);
CREATE INDEX IF NOT EXISTS ix_sessions_external_id ON sessions (external_id);

PRAGMA foreign_keys=ON;

-- migrate:down
PRAGMA foreign_keys=OFF;

CREATE TABLE execution_process_logs_old (
    execution_id TEXT PRIMARY KEY REFERENCES execution_processes(id),
    logs TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    inserted_at DATETIME NOT NULL
);

INSERT INTO execution_process_logs_old (execution_id, logs, byte_size, inserted_at)
SELECT execution_process_id,
       group_concat(logs, '\n'),
       sum(byte_size),
       max(inserted_at)
FROM execution_process_logs
GROUP BY execution_process_id;

DROP TABLE execution_process_logs;
ALTER TABLE execution_process_logs_old RENAME TO execution_process_logs;

DROP INDEX IF EXISTS ix_execution_process_logs_execution_process_id;
DROP INDEX IF EXISTS ix_execution_process_logs_inserted_at;
DROP INDEX IF EXISTS ix_sessions_external_id;

PRAGMA foreign_keys=ON;
