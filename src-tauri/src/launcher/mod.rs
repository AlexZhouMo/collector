use crate::db::Db;
use crate::error::{AppError, AppResult};
use std::path::Path;
use std::process::Command;

/// launcher 局部 manifest：运行时读游戏目录 game.json 取当前平台可执行文件。
#[derive(serde::Deserialize)]
struct GameExec {
    exec_win: Option<String>,
    exec_mac: Option<String>,
}

/// 启动游戏：运行时按 game_root + category_path 定位游戏目录，读 game.json
/// 取当前平台的相对可执行路径现算绝对路径后启动。游戏可启动性不入库。
#[tauri::command(rename_all = "camelCase")]
pub fn launch_game(db: tauri::State<Db>, category_path: String, title: String) -> AppResult<()> {
    let root = crate::settings::get(&db, "game_root")?
        .ok_or_else(|| AppError::Invalid("游戏目录未设置".into()))?;
    let dir = Path::new(&root).join(&category_path);

    let manifest_path = dir.join("game.json");
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| AppError::NotFound(format!("{title}: 读取 game.json 失败: {e}")))?;
    let manifest: GameExec = serde_json::from_str(&raw)
        .map_err(|e| AppError::Invalid(format!("{title}: 解析 game.json 失败: {e}")))?;

    let rel = if cfg!(target_os = "windows") {
        manifest.exec_win
    } else {
        manifest.exec_mac
    }
    .ok_or_else(|| AppError::Invalid(format!("{title}: 本平台无可执行文件")))?;

    let exec_path = dir.join(&rel);
    if !exec_path.exists() {
        return Err(AppError::NotFound(format!(
            "{title}: 可执行文件不存在: {}",
            exec_path.display()
        )));
    }

    #[cfg(target_os = "macos")]
    {
        if exec_path.to_string_lossy().ends_with(".app") {
            Command::new("open").arg(&exec_path).spawn()?;
        } else {
            Command::new(&exec_path).current_dir(&dir).spawn()?;
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Command::new(&exec_path).current_dir(&dir).spawn()?;
    }

    Ok(())
}
