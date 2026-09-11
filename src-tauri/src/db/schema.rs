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
    // 补偿迁移：早期版本建的 comic/game 表可能缺 UNIQUE(category_path,title) 约束
    // （建表用 CREATE TABLE IF NOT EXISTS，旧表存在就不重建，约束一直缺失），
    // 导致重扫入库的 `ON CONFLICT(category_path,title)` upsert 报
    // "ON CONFLICT clause does not match any PRIMARY KEY or UNIQUE constraint"。
    // 唯一索引同样能作为 upsert 的冲突目标，且对已有约束/已建索引的库幂等无害，
    // 故三表统一补建唯一索引，兼容新库与历史遗留库。
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_media_cat_title ON media(category_path,title);",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_comic_cat_title ON comic(category_path,title);",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_game_cat_title ON game(category_path,title);",
];
