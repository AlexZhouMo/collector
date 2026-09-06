use crate::db::Db;
use crate::error::{AppError, AppResult};
use std::path::Path;
use std::process::Command;

#[tauri::command(rename_all = "camelCase")]
pub fn launch_game(db: tauri::State<Db>, item_id: i64, exec_path: String) -> AppResult<()> {
    let p = Path::new(&exec_path);
    if !p.exists() {
        return Err(AppError::NotFound(exec_path));
    }

    #[cfg(target_os = "macos")]
    {
        if exec_path.ends_with(".app") {
            Command::new("open").arg(&exec_path).spawn()?;
        } else {
            Command::new(&exec_path).spawn()?;
        }
    }
    #[cfg(target_os = "windows")]
    {
        let dir = p.parent().unwrap_or(Path::new("."));
        Command::new(&exec_path).current_dir(dir).spawn()?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Command::new(&exec_path).spawn()?;
    }

    let conn = db.0.lock().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    conn.execute(
        "INSERT INTO game_state(item_id,last_launched_at,launch_count) VALUES(?1,?2,1)
         ON CONFLICT(item_id) DO UPDATE SET last_launched_at=?2, launch_count=launch_count+1",
        rusqlite::params![item_id, now],
    )
    .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}
