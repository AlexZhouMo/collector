pub const MIGRATIONS: &[&str] = &[
    // v1
    "CREATE TABLE IF NOT EXISTS media_item (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        kind TEXT NOT NULL,
        category TEXT NOT NULL,
        category_path TEXT NOT NULL,
        title TEXT NOT NULL,
        path TEXT NOT NULL UNIQUE,
        subtitle_path TEXT,
        cover_path TEXT,
        description TEXT,
        platform_ok INTEGER NOT NULL DEFAULT 1,
        exec_path TEXT,
        scanned_at INTEGER NOT NULL
    );",
    "CREATE INDEX IF NOT EXISTS idx_media_kind ON media_item(kind);",
    "CREATE TABLE IF NOT EXISTS watch_state (
        item_id INTEGER PRIMARY KEY REFERENCES media_item(id) ON DELETE CASCADE,
        position_secs REAL DEFAULT 0,
        comic_page INTEGER DEFAULT 0,
        last_opened_at INTEGER
    );",
    "CREATE TABLE IF NOT EXISTS game_state (
        item_id INTEGER PRIMARY KEY REFERENCES media_item(id) ON DELETE CASCADE,
        last_launched_at INTEGER,
        launch_count INTEGER DEFAULT 0
    );",
    "CREATE TABLE IF NOT EXISTS settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );",
];
