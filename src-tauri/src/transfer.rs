//! 分类数据包导入/导出（设置页「导入」「导出」）。
//!
//! 包是一个 zip：
//! - `manifest.json`：类型校验文件（格式名、格式版本、分类类型、导出时间、行数、文件清单）。
//!   导入时先校验它，类型不符/缺失/版本不支持一律拒绝，什么都不改。
//! - `data.json`：该分类在库里的全部行（含旧 id，用于建立旧→新 id 映射）。
//! - `files/<相对 app_data 路径>`：行引用的封面（covers/...）与按 category/category_path/title
//!   推导出的字幕（subtitles/...）。
//!
//! 导入：清空该分类的行 → 整表按分类顺序重排、ID 从 1 重新编号 → 按包内顺序无损还原；
//! 封面/字幕放回 app_data 原相对路径（封面同名但内容不同时改名并改写 cover_path）。
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::library::paths;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

pub const FORMAT: &str = "collector-category-export";
pub const FORMAT_VERSION: u32 = 1;
const MANIFEST: &str = "manifest.json";
const DATA: &str = "data.json";
const FILES_PREFIX: &str = "files/";

/// 可导入/导出的分类。key 为前端传入值（与设置页 settings key 前缀对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CategorySpec {
    pub key: &'static str,
    pub table: &'static str,
    /// 行的 category 列值
    pub category: &'static str,
    /// 设置页显示名
    pub label: &'static str,
}

pub const SPECS: &[CategorySpec] = &[
    CategorySpec { key: "video_movie", table: "media", category: "电影", label: "电影" },
    CategorySpec { key: "video_anime", table: "media", category: "动漫", label: "动漫" },
    CategorySpec { key: "video_tv", table: "media", category: "剧集", label: "剧集" },
    CategorySpec { key: "game", table: "game", category: "游戏", label: "游戏" },
];

pub fn spec_of(key: &str) -> AppResult<CategorySpec> {
    SPECS
        .iter()
        .copied()
        .find(|s| s.key == key)
        .ok_or_else(|| AppError::Invalid(format!("不支持导入/导出的分类: {key}")))
}

/// 进度回调：(阶段文字, 总体百分比 0–100)。
pub type ProgressFn<'a> = &'a dyn Fn(&str, u8);

/// 分阶段进度：阶段占总进度 [base, base+span)，阶段内按 done/total 线性推进。
/// 只在百分比变化时回调，避免上千个文件时事件刷屏。
struct Stage<'a> {
    on: ProgressFn<'a>,
    label: &'static str,
    base: f64,
    span: f64,
    last: std::cell::Cell<i16>,
}

impl<'a> Stage<'a> {
    fn new(on: ProgressFn<'a>, label: &'static str, base: u8, end: u8) -> Self {
        let s = Stage { on, label, base: base as f64, span: (end - base) as f64, last: std::cell::Cell::new(-1) };
        s.tick(0, 1);
        s
    }
    fn tick(&self, done: usize, total: usize) {
        let frac = if total == 0 { 1.0 } else { done as f64 / total as f64 };
        let pct = (self.base + self.span * frac).floor().clamp(0.0, 100.0) as i16;
        if pct != self.last.get() {
            self.last.set(pct);
            (self.on)(self.label, pct as u8);
        }
    }
}

/// 视频表三分类的固定顺序（与 maintenance::db_reset 一致），整表重排时用。
fn category_rank(c: &str) -> u8 {
    match c {
        "电影" => 0,
        "动漫" => 1,
        "剧集" => 2,
        _ => 3,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub format: String,
    pub format_version: u32,
    /// 分类类型 key（video_movie / video_anime / video_tv / game）
    pub category_key: String,
    pub category: String,
    pub table: String,
    /// 导出时间（UTC，ISO 8601）
    pub exported_at: String,
    pub app_version: String,
    pub row_count: usize,
    /// 包内文件（相对 app_data），导入时逐个核对存在
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Row {
    pub id: i64,
    pub category: String,
    pub category_path: String,
    pub title: String,
    pub description: Option<String>,
    pub cover_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DataFile {
    rows: Vec<Row>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ExportReport {
    pub rows: usize,
    pub covers: usize,
    pub subtitles: usize,
    /// 库里引用了但磁盘上找不到的文件（行照常导出，cover_path 原样保留）
    pub missing_files: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ImportReport {
    pub rows: usize,
    pub covers: usize,
    pub subtitles: usize,
    /// 该分类导入后 ID 范围（含两端）；空包为 None
    pub first_id: Option<i64>,
    pub last_id: Option<i64>,
    /// 因同名但内容不同而改名的封面：(包内路径, 实际写入路径)
    pub renamed_covers: Vec<(String, String)>,
}

fn db_err(e: rusqlite::Error) -> AppError {
    AppError::Db(e.to_string())
}
fn zip_err(e: zip::result::ZipError) -> AppError {
    AppError::Other(format!("zip: {e}"))
}

/// UTC 时间戳 → ISO 8601（不引入 chrono：civil-from-days 算法）。
fn iso_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if mo <= 2 { 1 } else { 0 };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    iso_utc(secs)
}

/// 相对 app_data 的包内路径是否安全且属于该分类：只允许 covers/<sub>/.. 与
/// subtitles/<category>/..，禁止绝对路径、`..`、盘符等。
fn safe_rel(rel: &str, spec: &CategorySpec) -> bool {
    if rel.is_empty() || rel.contains('\\') || rel.contains(':') {
        return false;
    }
    let p = Path::new(rel);
    if !p.components().all(|c| matches!(c, Component::Normal(_))) {
        return false;
    }
    let cover_sub = if spec.table == "game" { "covers/game/" } else { "covers/media/" };
    rel.starts_with(cover_sub) || rel.starts_with(&format!("subtitles/{}/", spec.category))
}

fn subtitle_rel(spec: &CategorySpec, row: &Row) -> Option<String> {
    if spec.table == "media" {
        Some(paths::subtitle_rel_path(&row.category, &row.category_path, &row.title))
    } else {
        None
    }
}

/// 读 table 中 category 等于（eq=true）或不等于（eq=false）给定值的行，按 id 升序。
fn query_rows(conn: &rusqlite::Connection, table: &str, category: &str, eq: bool) -> AppResult<Vec<Row>> {
    let op = if eq { "=" } else { "<>" };
    let mut stmt = conn
        .prepare(&format!(
            "SELECT id,category,category_path,title,description,cover_path FROM {table}              WHERE category{op}?1 ORDER BY id"
        ))
        .map_err(db_err)?;
    let it = stmt
        .query_map([category], |r| {
            Ok(Row {
                id: r.get(0)?,
                category: r.get(1)?,
                category_path: r.get(2)?,
                title: r.get(3)?,
                description: r.get(4)?,
                cover_path: r.get(5)?,
            })
        })
        .map_err(db_err)?;
    it.collect::<Result<Vec<_>, _>>().map_err(db_err)
}

/// 导出该分类到 dest（zip）。先写临时文件再改名，失败不留半个包。
pub fn export_impl(db: &Db, app_data: &Path, key: &str, dest: &Path) -> AppResult<ExportReport> {
    export_with_progress(db, app_data, key, dest, &|_, _| {})
}

pub fn export_with_progress(
    db: &Db,
    app_data: &Path,
    key: &str,
    dest: &Path,
    on: ProgressFn,
) -> AppResult<ExportReport> {
    let spec = spec_of(key)?;
    let collect = Stage::new(on, "收集数据", 0, 5);
    let rows = {
        let conn = db.0.lock().unwrap();
        query_rows(&conn, spec.table, spec.category, true)?
    };
    let app_data_str = app_data.to_string_lossy().replace('\\', "/");

    // 收集文件：封面（去重，多行共享同一封面只打一份）+ 字幕
    let mut files: BTreeSet<String> = BTreeSet::new();
    let mut missing = Vec::new();
    let (mut covers, mut subtitles) = (0usize, 0usize);
    for (ri, row) in rows.iter().enumerate() {
        collect.tick(ri, rows.len());
        if let Some(c) = row.cover_path.as_deref().filter(|c| !c.is_empty()) {
            let rel = paths::appdata_to_relative(c, &app_data_str);
            if safe_rel(&rel, &spec) && app_data.join(&rel).is_file() {
                if files.insert(rel) {
                    covers += 1;
                }
            } else if !missing.contains(&rel) {
                missing.push(rel);
            }
        }
        if let Some(s) = subtitle_rel(&spec, row) {
            if safe_rel(&s, &spec) && app_data.join(&s).is_file() && files.insert(s) {
                subtitles += 1;
            }
        }
    }

    let manifest = Manifest {
        format: FORMAT.into(),
        format_version: FORMAT_VERSION,
        category_key: spec.key.into(),
        category: spec.category.into(),
        table: spec.table.into(),
        exported_at: now_iso(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        row_count: rows.len(),
        files: files.iter().cloned().collect(),
    };

    let tmp = dest.with_extension("zip.part");
    let write = || -> AppResult<()> {
        let f = std::fs::File::create(&tmp)?;
        let mut zw = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zw.start_file(MANIFEST, opts).map_err(zip_err)?;
        zw.write_all(&serde_json::to_vec_pretty(&manifest).map_err(|e| AppError::Other(e.to_string()))?)?;
        zw.start_file(DATA, opts).map_err(zip_err)?;
        let data = DataFile { rows: rows.clone() };
        zw.write_all(&serde_json::to_vec_pretty(&data).map_err(|e| AppError::Other(e.to_string()))?)?;
        let pack = Stage::new(on, "打包文件", 5, 99);
        for (fi, rel) in files.iter().enumerate() {
            pack.tick(fi, files.len());
            zw.start_file(format!("{FILES_PREFIX}{rel}"), opts).map_err(zip_err)?;
            zw.write_all(&std::fs::read(app_data.join(rel))?)?;
        }
        pack.tick(1, 1);
        zw.finish().map_err(zip_err)?;
        Ok(())
    };
    if let Err(e) = write() {
        std::fs::remove_file(&tmp).ok();
        return Err(e);
    }
    if dest.exists() {
        std::fs::remove_file(dest)?;
    }
    std::fs::rename(&tmp, dest)?;
    on("完成", 100);
    Ok(ExportReport { rows: rows.len(), covers, subtitles, missing_files: missing })
}

/// 读取并校验包：manifest 存在、格式/版本正确、类型与目标分类一致、data.json 行数与类型一致、
/// 文件清单全部存在且路径安全。返回 (manifest, rows, 文件内容)。不碰数据库与磁盘。
fn read_package(
    src: &Path,
    spec: &CategorySpec,
    on: ProgressFn,
) -> AppResult<(Manifest, Vec<Row>, Vec<(String, Vec<u8>)>)> {
    let f = std::fs::File::open(src)
        .map_err(|e| AppError::Invalid(format!("无法打开导入包: {e}")))?;
    let mut za = zip::ZipArchive::new(f)
        .map_err(|e| AppError::Invalid(format!("不是有效的 zip 包: {e}")))?;
    let read_entry = |za: &mut zip::ZipArchive<std::fs::File>, name: &str| -> AppResult<Vec<u8>> {
        let mut e = za
            .by_name(name)
            .map_err(|_| AppError::Invalid(format!("导入包缺少 {name}，不是本应用导出的分类数据包")))?;
        let mut buf = Vec::new();
        e.read_to_end(&mut buf)?;
        Ok(buf)
    };

    let manifest: Manifest = serde_json::from_slice(&read_entry(&mut za, MANIFEST)?)
        .map_err(|e| AppError::Invalid(format!("manifest.json 格式错误: {e}")))?;
    if manifest.format != FORMAT {
        return Err(AppError::Invalid(format!("不是本应用的分类数据包（format={}）", manifest.format)));
    }
    if manifest.format_version != FORMAT_VERSION {
        return Err(AppError::Invalid(format!(
            "不支持的数据包版本 {}（当前支持 {FORMAT_VERSION}）",
            manifest.format_version
        )));
    }
    if manifest.category_key != spec.key || manifest.category != spec.category || manifest.table != spec.table {
        let pkg_label = SPECS
            .iter()
            .find(|s| s.key == manifest.category_key)
            .map(|s| s.label)
            .unwrap_or(manifest.category.as_str());
        return Err(AppError::Invalid(format!(
            "类型不符：这是「{pkg_label}」的数据包，不能导入到「{}」",
            spec.label
        )));
    }

    let data: DataFile = serde_json::from_slice(&read_entry(&mut za, DATA)?)
        .map_err(|e| AppError::Invalid(format!("data.json 格式错误: {e}")))?;
    if data.rows.len() != manifest.row_count {
        return Err(AppError::Invalid(format!(
            "数据包不完整：manifest 记录 {} 行，data.json 实有 {} 行",
            manifest.row_count,
            data.rows.len()
        )));
    }
    let mut seen_ids = BTreeSet::new();
    let mut seen_keys = BTreeSet::new();
    for r in &data.rows {
        if r.category != spec.category {
            return Err(AppError::Invalid(format!(
                "数据包中「{}」的分类为「{}」，与包类型「{}」不符",
                r.title, r.category, spec.label
            )));
        }
        if !seen_ids.insert(r.id) {
            return Err(AppError::Invalid(format!("数据包中 id {} 重复", r.id)));
        }
        if !seen_keys.insert((r.category_path.clone(), r.title.clone())) {
            return Err(AppError::Invalid(format!("数据包中「{}/{}」重复", r.category_path, r.title)));
        }
    }

    let mut files = Vec::new();
    let read = Stage::new(on, "读取数据包", 2, 40);
    for (fi, rel) in manifest.files.iter().enumerate() {
        read.tick(fi, manifest.files.len());
        if !safe_rel(rel, spec) {
            return Err(AppError::Invalid(format!("数据包含非法文件路径: {rel}")));
        }
        let bytes = read_entry(&mut za, &format!("{FILES_PREFIX}{rel}"))
            .map_err(|_| AppError::Invalid(format!("数据包缺少文件: {rel}")))?;
        files.push((rel.clone(), bytes));
    }
    Ok((manifest, data.rows, files))
}

/// 封面落盘位置：同名文件不存在或内容相同 → 原路径；内容不同 → 加 `_impN` 后缀找空位。
fn cover_target(app_data: &Path, rel: &str, bytes: &[u8]) -> String {
    let same_or_free = |r: &str| match std::fs::read(app_data.join(r)) {
        Ok(b) => b == bytes,
        Err(_) => true,
    };
    if same_or_free(rel) {
        return rel.to_string();
    }
    let (stem, ext) = match rel.rfind('.') {
        Some(i) if i > rel.rfind('/').unwrap_or(0) => (&rel[..i], &rel[i..]),
        _ => (rel, ""),
    };
    (1..)
        .map(|n| format!("{stem}_imp{n}{ext}"))
        .find(|r| same_or_free(r))
        .unwrap()
}

/// 导入：校验 → 清空该分类 → 整表 ID 从 1 重排 → 还原行 → 文件归位。
/// 数据库操作在一个事务里；文件先全部写入暂存目录，事务提交前再移到位，任一步失败都回滚数据库。
pub fn import_impl(db: &Db, app_data: &Path, key: &str, src: &Path) -> AppResult<ImportReport> {
    import_with_progress(db, app_data, key, src, &|_, _| {})
}

pub fn import_with_progress(
    db: &Db,
    app_data: &Path,
    key: &str,
    src: &Path,
    on: ProgressFn,
) -> AppResult<ImportReport> {
    let spec = spec_of(key)?;
    on("校验数据包", 0);
    let (_manifest, pkg_rows, files) = read_package(src, &spec, on)?;

    // 1) 规划文件落点（封面可能改名）
    let mut cover_map: HashMap<String, String> = HashMap::new();
    let mut renamed = Vec::new();
    let (mut covers, mut subtitles) = (0usize, 0usize);
    let mut plan: Vec<(PathBuf, &Vec<u8>)> = Vec::new();
    for (rel, bytes) in &files {
        let target = if rel.starts_with("covers/") {
            covers += 1;
            let t = cover_target(app_data, rel, bytes);
            if &t != rel {
                renamed.push((rel.clone(), t.clone()));
            }
            cover_map.insert(rel.clone(), t.clone());
            t
        } else {
            subtitles += 1;
            rel.clone()
        };
        plan.push((app_data.join(target), bytes));
    }

    // 2) 暂存：全部写进 app_data/import_staging，保证磁盘可写后再动数据库
    let staging = app_data.join("import_staging");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;
    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new();
    let stage_w = Stage::new(on, "写入暂存", 40, 70);
    for (i, (dst, bytes)) in plan.iter().enumerate() {
        stage_w.tick(i, plan.len());
        let tmp = staging.join(format!("{i}.bin"));
        if let Err(e) = std::fs::write(&tmp, bytes) {
            std::fs::remove_dir_all(&staging).ok();
            return Err(e.into());
        }
        staged.push((tmp, dst.clone()));
    }

    // 3) 数据库：事务内重建整表
    let app_data_str = app_data.to_string_lossy().replace('\\', "/");
    let result = (|| -> AppResult<ImportReport> {
        let mut conn = db.0.lock().unwrap();
        let tx = conn.transaction().map_err(db_err)?;
        // 其他分类的行（视频表三分类共用 media 表），保持原相对顺序
        let mut others = query_rows(&tx, spec.table, spec.category, false)?;
        others.sort_by_key(|r| category_rank(&r.category)); // 稳定排序，组内保持原 id 顺序

        // 包内行按旧 id 顺序还原；cover_path 统一存相对路径并套用改名映射
        let mut imported = pkg_rows.clone();
        imported.sort_by_key(|r| r.id);
        for r in &mut imported {
            if let Some(c) = r.cover_path.as_deref().filter(|c| !c.is_empty()) {
                let rel = paths::appdata_to_relative(c, &app_data_str);
                r.cover_path = Some(cover_map.get(&rel).cloned().unwrap_or(rel));
            }
        }

        let rank = category_rank(spec.category);
        let mut all: Vec<(&Row, bool)> = Vec::new();
        all.extend(others.iter().filter(|r| category_rank(&r.category) < rank).map(|r| (r, false)));
        all.extend(imported.iter().map(|r| (r, true)));
        all.extend(others.iter().filter(|r| category_rank(&r.category) >= rank).map(|r| (r, false)));

        tx.execute(&format!("DELETE FROM {}", spec.table), []).map_err(db_err)?;
        tx.execute("DELETE FROM sqlite_sequence WHERE name=?1", [spec.table]).map_err(db_err)?;
        let mut id_map: Vec<(i64, i64)> = Vec::new(); // 旧 id → 新 id（导入行）
        let write_db = Stage::new(on, "写入数据库", 70, 85);
        for (ai, (r, is_new)) in all.iter().enumerate() {
            write_db.tick(ai, all.len());
            tx.execute(
                &format!(
                    "INSERT INTO {} (category,category_path,title,description,cover_path) VALUES (?1,?2,?3,?4,?5)",
                    spec.table
                ),
                rusqlite::params![r.category, r.category_path, r.title, r.description, r.cover_path],
            )
            .map_err(db_err)?;
            if *is_new {
                id_map.push((r.id, tx.last_insert_rowid()));
            }
        }

        // 4) 文件归位（失败 → 事务随 tx drop 回滚）
        let place = Stage::new(on, "文件归位", 85, 99);
        for (si, (tmp, dst)) in staged.iter().enumerate() {
            place.tick(si, staged.len());
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if dst.exists() {
                std::fs::remove_file(dst)?;
            }
            std::fs::rename(tmp, dst)?;
        }
        tx.commit().map_err(db_err)?;
        on("完成", 100);
        Ok(ImportReport {
            rows: imported.len(),
            covers,
            subtitles,
            first_id: id_map.first().map(|p| p.1),
            last_id: id_map.last().map(|p| p.1),
            renamed_covers: renamed.clone(),
        })
    })();
    std::fs::remove_dir_all(&staging).ok();
    result
}

fn app_data_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    use tauri::Manager;
    app.path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("app_data_dir: {e}")))
}

/// 向前端推送进度：事件 `transfer-progress`，payload {key, op, stage, pct}。
fn emit_progress(app: &tauri::AppHandle, key: &str, op: &str, stage: &str, pct: u8) {
    use tauri::Emitter;
    let _ = app.emit(
        "transfer-progress",
        serde_json::json!({ "key": key, "op": op, "stage": stage, "pct": pct }),
    );
}

// 异步 + spawn_blocking：同步命令会跑在主线程上，界面会卡住、进度条也刷不出来。
#[tauri::command]
pub async fn category_export(app: tauri::AppHandle, key: String, dest: String) -> AppResult<ExportReport> {
    let app_data = app_data_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let db = app.state::<Db>();
        let on = |stage: &str, pct: u8| emit_progress(&app, &key, "export", stage, pct);
        export_with_progress(&db, &app_data, &key, Path::new(&dest), &on)
    })
    .await
    .map_err(|e| AppError::Other(format!("join: {e}")))?
}

#[tauri::command]
pub async fn category_import(app: tauri::AppHandle, key: String, src: String) -> AppResult<ImportReport> {
    let app_data = app_data_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let db = app.state::<Db>();
        let on = |stage: &str, pct: u8| emit_progress(&app, &key, "import", stage, pct);
        import_with_progress(&db, &app_data, &key, Path::new(&src), &on)
    })
    .await
    .map_err(|e| AppError::Other(format!("join: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(db: &Db, table: &str, cat: &str, cpath: &str, title: &str, cover: Option<&str>, desc: Option<&str>) {
        let c = db.0.lock().unwrap();
        c.execute(
            &format!("INSERT INTO {table}(category,category_path,title,cover_path,description) VALUES(?1,?2,?3,?4,?5)"),
            rusqlite::params![cat, cpath, title, cover, desc],
        )
        .unwrap();
    }

    fn rows_of(db: &Db, table: &str, cat: &str) -> Vec<Row> {
        query_rows(&db.0.lock().unwrap(), table, cat, true).unwrap()
    }

    fn write(p: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    /// 造一个含三分类视频 + 游戏的库：电影两条共享封面，一条带字幕，一条封面缺失。
    fn seed(app: &Path) -> Db {
        let db = Db::open_in_memory().unwrap();
        insert(&db, "media", "动漫", "热血", "海贼王", None, None);
        insert(&db, "media", "电影", "科幻/星战", "[1977].星球大战", Some("covers/media/a.jpg"), Some("简介A"));
        insert(&db, "media", "电影", "科幻/星战", "[1980].帝国反击战", Some("covers/media/a.jpg"), None);
        insert(&db, "media", "剧集", "", "老友记", Some("covers/media/tv.jpg"), None);
        insert(&db, "media", "电影", "", "沙丘", Some("covers/media/missing.jpg"), None);
        insert(&db, "game", "游戏", "策略", "三国志11", Some("covers/game/g.jpg"), Some("光荣"));
        write(&app.join("covers/media/a.jpg"), b"A");
        write(&app.join("covers/media/tv.jpg"), b"TV");
        write(&app.join("covers/game/g.jpg"), b"G");
        write(&app.join("subtitles/电影/科幻/星战/[1977].星球大战.ass"), "字幕".as_bytes());
        db
    }

    #[test]
    fn iso_utc_formats() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(1_700_000_000), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn safe_rel_rejects_traversal_and_foreign_dirs() {
        let movie = spec_of("video_movie").unwrap();
        assert!(safe_rel("covers/media/a.jpg", &movie));
        assert!(safe_rel("subtitles/电影/x/y.ass", &movie));
        assert!(!safe_rel("subtitles/动漫/y.ass", &movie));
        assert!(!safe_rel("covers/game/a.jpg", &movie));
        assert!(!safe_rel("covers/media/../../x", &movie));
        assert!(!safe_rel("C:/x/covers/media/a.jpg", &movie));
        assert!(!safe_rel("/covers/media/a.jpg", &movie));
    }

    #[test]
    fn export_then_import_roundtrip_movie() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        let db = seed(&app);
        let zip = tmp.path().join("movie.zip");
        let rep = export_impl(&db, &app, "video_movie", &zip).unwrap();
        assert_eq!(rep.rows, 3);
        assert_eq!(rep.covers, 1, "共享封面只打一份");
        assert_eq!(rep.subtitles, 1);
        assert_eq!(rep.missing_files, vec!["covers/media/missing.jpg".to_string()]);
        let before = rows_of(&db, "media", "电影");

        // 换一台"新机器"：空库 + 空 app_data，库里先有其他分类与旧电影数据
        let app2 = tmp.path().join("app2");
        let db2 = Db::open_in_memory().unwrap();
        insert(&db2, "media", "剧集", "", "老友记", None, None);
        insert(&db2, "media", "电影", "", "旧电影", None, None);
        insert(&db2, "media", "动漫", "", "海贼王", None, None);
        let r = import_impl(&db2, &app2, "video_movie", &zip).unwrap();
        assert_eq!((r.rows, r.covers, r.subtitles), (3, 1, 1));
        assert_eq!((r.first_id, r.last_id), (Some(1), Some(3)), "电影 ID 从 1 开始");

        let after = rows_of(&db2, "media", "电影");
        assert_eq!(after.len(), 3, "旧电影被清空");
        for (a, b) in after.iter().zip(before.iter()) {
            assert_eq!((&a.category_path, &a.title, &a.description, &a.cover_path),
                       (&b.category_path, &b.title, &b.description, &b.cover_path));
        }
        assert_eq!(after.iter().map(|r| r.id).collect::<Vec<_>>(), vec![1, 2, 3]);
        // 其他分类保留并重排在其后：动漫 4、剧集 5
        assert_eq!(rows_of(&db2, "media", "动漫")[0].id, 4);
        assert_eq!(rows_of(&db2, "media", "剧集")[0].id, 5);
        // 文件归位
        assert_eq!(std::fs::read(app2.join("covers/media/a.jpg")).unwrap(), b"A");
        assert_eq!(std::fs::read(app2.join("subtitles/电影/科幻/星战/[1977].星球大战.ass")).unwrap(), "字幕".as_bytes());
        assert!(!app2.join("import_staging").exists());
    }

    #[test]
    fn import_game_resets_ids_from_one() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        let db = seed(&app);
        insert(&db, "game", "游戏", "动作", "鬼泣", None, None);
        { db.0.lock().unwrap().execute("DELETE FROM game WHERE title='三国志11'", []).unwrap(); }
        insert(&db, "game", "游戏", "策略", "三国志11", Some("covers/game/g.jpg"), Some("光荣"));
        let zip = tmp.path().join("game.zip");
        export_impl(&db, &app, "game", &zip).unwrap();
        let r = import_impl(&db, &app, "game", &zip).unwrap();
        assert_eq!((r.first_id, r.last_id), (Some(1), Some(2)));
        let rows = rows_of(&db, "game", "游戏");
        assert_eq!(rows[0].title, "鬼泣");
        assert_eq!(rows[1].cover_path.as_deref(), Some("covers/game/g.jpg"));
        assert_eq!(rows[1].description.as_deref(), Some("光荣"));
    }

    #[test]
    fn progress_is_monotonic_and_reaches_100() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        let db = seed(&app);
        insert(&db, "game", "游戏", "动作", "鬼泣", Some("covers/game/g.jpg"), None);
        let zip = tmp.path().join("game.zip");
        for op in ["export", "import"] {
            let seen = std::cell::RefCell::new(Vec::<u8>::new());
            let on = |_: &str, pct: u8| seen.borrow_mut().push(pct);
            if op == "export" {
                export_with_progress(&db, &app, "game", &zip, &on).unwrap();
            } else {
                import_with_progress(&db, &app, "game", &zip, &on).unwrap();
            }
            let v = seen.into_inner();
            assert!(v.windows(2).all(|w| w[0] <= w[1]), "{op} 进度回退: {v:?}");
            assert_eq!(v.last(), Some(&100), "{op} 未到 100: {v:?}");
        }
    }

    #[test]
    fn cross_type_import_is_rejected_without_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        let db = seed(&app);
        let zip = tmp.path().join("movie.zip");
        export_impl(&db, &app, "video_movie", &zip).unwrap();
        let anime_before = rows_of(&db, "media", "动漫");
        let err = import_impl(&db, &app, "video_anime", &zip).unwrap_err().to_string();
        assert!(err.contains("类型不符") && err.contains("电影") && err.contains("动漫"), "{err}");
        assert!(import_impl(&db, &app, "game", &zip).is_err());
        assert_eq!(rows_of(&db, "media", "动漫"), anime_before);
        assert_eq!(rows_of(&db, "media", "电影").len(), 3);
    }

    #[test]
    fn missing_manifest_or_file_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        let db = seed(&app);
        // 无 manifest 的 zip
        let bad = tmp.path().join("bad.zip");
        {
            let mut zw = zip::ZipWriter::new(std::fs::File::create(&bad).unwrap());
            zw.start_file("data.json", zip::write::SimpleFileOptions::default()).unwrap();
            zw.write_all(b"{\"rows\":[]}").unwrap();
            zw.finish().unwrap();
        }
        let err = import_impl(&db, &app, "video_movie", &bad).unwrap_err().to_string();
        assert!(err.contains("manifest.json"), "{err}");
        // manifest 列出的文件在包里缺失
        let good = tmp.path().join("movie.zip");
        export_impl(&db, &app, "video_movie", &good).unwrap();
        let partial = tmp.path().join("partial.zip");
        {
            let mut za = zip::ZipArchive::new(std::fs::File::open(&good).unwrap()).unwrap();
            let mut zw = zip::ZipWriter::new(std::fs::File::create(&partial).unwrap());
            for i in 0..za.len() {
                let e = za.by_index(i).unwrap();
                if e.name().starts_with("files/subtitles") { continue; }
                zw.raw_copy_file(e).unwrap();
            }
            zw.finish().unwrap();
        }
        let err = import_impl(&db, &app, "video_movie", &partial).unwrap_err().to_string();
        assert!(err.contains("缺少文件"), "{err}");
        // 非 zip
        let junk = tmp.path().join("junk.zip");
        std::fs::write(&junk, b"not a zip").unwrap();
        assert!(import_impl(&db, &app, "video_movie", &junk).is_err());
        assert_eq!(rows_of(&db, "media", "电影").len(), 3, "失败不改库");
    }

    #[test]
    fn conflicting_cover_is_renamed_and_rewritten() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        let db = seed(&app);
        let zip = tmp.path().join("movie.zip");
        export_impl(&db, &app, "video_movie", &zip).unwrap();
        // 目标机上同名封面内容不同（被别的分类引用）
        let app2 = tmp.path().join("app2");
        write(&app2.join("covers/media/a.jpg"), b"OTHER");
        let db2 = Db::open_in_memory().unwrap();
        insert(&db2, "media", "剧集", "", "X", Some("covers/media/a.jpg"), None);
        let r = import_impl(&db2, &app2, "video_movie", &zip).unwrap();
        assert_eq!(r.renamed_covers, vec![("covers/media/a.jpg".into(), "covers/media/a_imp1.jpg".into())]);
        assert_eq!(std::fs::read(app2.join("covers/media/a.jpg")).unwrap(), b"OTHER");
        assert_eq!(std::fs::read(app2.join("covers/media/a_imp1.jpg")).unwrap(), b"A");
        let movies = rows_of(&db2, "media", "电影");
        assert_eq!(movies[0].cover_path.as_deref(), Some("covers/media/a_imp1.jpg"));
        assert_eq!(movies[1].cover_path.as_deref(), Some("covers/media/a_imp1.jpg"));
        assert_eq!(rows_of(&db2, "media", "剧集")[0].cover_path.as_deref(), Some("covers/media/a.jpg"));
    }
}
