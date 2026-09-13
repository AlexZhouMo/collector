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
    // 迁移：把 comic 规范化为带固定 category 列（值恒为"漫画"，为将来三表合并预留）。
    // 用标准"新表→拷公共列并回填 category→换名"，幂等：每次重建都回填同值。
    // 注：应用逻辑不读该 category 列（category 仍从 category_path 首段推导），仅为统一表结构。
    "CREATE TABLE IF NOT EXISTS comic_new (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        category TEXT NOT NULL,
        category_path TEXT NOT NULL,
        title TEXT NOT NULL,
        description TEXT,
        cover_path TEXT,
        UNIQUE(category_path,title)
    );",
    "INSERT INTO comic_new (id,category,category_path,title,description,cover_path)
       SELECT id,'漫画',category_path,title,description,cover_path FROM comic;",
    "DROP TABLE comic;",
    "ALTER TABLE comic_new RENAME TO comic;",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_comic_cat_title ON comic(category_path,title);",
    // 迁移：把 game 规范化为带固定 category 列（值恒为"游戏"），同 comic。幂等。
    "CREATE TABLE IF NOT EXISTS game_new (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        category TEXT NOT NULL,
        category_path TEXT NOT NULL,
        title TEXT NOT NULL,
        description TEXT,
        cover_path TEXT,
        UNIQUE(category_path,title)
    );",
    "INSERT INTO game_new (id,category,category_path,title,description,cover_path)
       SELECT id,'游戏',category_path,title,description,cover_path FROM game;",
    "DROP TABLE game;",
    "ALTER TABLE game_new RENAME TO game;",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_game_cat_title ON game(category_path,title);",
];
