-- Schema for build_configs table
CREATE TABLE build_configs (
    id INTEGER PRIMARY KEY AUTOINCREMENT, -- SQLite uses AUTOINCREMENT, PostgreSQL uses SERIAL/BIGSERIAL
    name TEXT NOT NULL
);

-- Schema for files table
CREATE TABLE files (
    id INTEGER PRIMARY KEY AUTOINCREMENT, -- SQLite uses AUTOINCREMENT, PostgreSQL uses SERIAL/BIGSERIAL
    build_config_id INTEGER REFERENCES build_configs(id) ON DELETE CASCADE, -- Nullable reference to build_configs.id
    rel_path TEXT NOT NULL,
    content_hash BLOB NOT NULL, -- SQLite uses BLOB, PostgreSQL uses BYTEA
    CONSTRAINT unique_rel_path UNIQUE (rel_path) -- Unique constraint for rel_path
);

-- Sorted unique index on rel_path
CREATE UNIQUE INDEX idx_files_rel_path_sorted ON files (rel_path);

-- Sorted index on build_config_id and rel_path
CREATE INDEX idx_files_build_config_id_rel_path_sorted ON files (build_config_id, rel_path);

