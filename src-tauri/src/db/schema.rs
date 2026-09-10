pub const MIGRATIONS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS media (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        category TEXT NOT NULL,
        category_path TEXT NOT NULL,
        title TEXT NOT NULL,
        description TEXT,
        cover_path TEXT,
        UNIQUE(category_path,title)
    );",
    "CREATE TABLE IF NOT EXISTS comic (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        category TEXT NOT NULL,
        category_path TEXT NOT NULL,
        title TEXT NOT NULL,
        description TEXT,
        cover_path TEXT,
        UNIQUE(category_path,title)
    );",
    "CREATE TABLE IF NOT EXISTS game (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        category TEXT NOT NULL,
        category_path TEXT NOT NULL,
        title TEXT NOT NULL,
        description TEXT,
        cover_path TEXT,
        UNIQUE(category_path,title)
    );",
    "CREATE TABLE IF NOT EXISTS settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );",
];
