use crate::error::{AppError, AppResult};
use libmpv2::{Mpv, SetData};

/// libmpv 播放器封装。
///
/// 视频输出直接渲染到宿主传入的原生窗口句柄（`wid`）。`wid`、`hwdec`、
/// `keep-open` 属于 mpv 初始化前需要落定的选项，因此在 `with_initializer`
/// 阶段通过 `set_option` 设置——初始化完成后再设 `wid` 对视频输出无效。
pub struct Player {
    mpv: Mpv,
}

impl Player {
    /// 创建 mpv 实例并把视频渲染到原生窗口句柄 `wid`。
    ///
    /// libmpv2 6.x 的 `Mpv::new()` 会立即执行 `mpv_initialize`，而 `wid`
    /// 是“只在初始化时读取”的选项，所以这里用 `with_initializer` 在初始化
    /// 之前把 `wid` / `hwdec` / `keep-open` 作为 option 写入。
    ///
    /// `wid == 0` 视为“不嵌入”的回退：跳过 wid 选项，让 mpv 自行开一个
    /// 独立窗口渲染（Task 3.3 回退A）。这样即使宿主取原生句柄失败或选择
    /// 不内嵌，核心的“能播放 + 可控制”闭环仍成立。
    pub fn new(wid: i64) -> AppResult<Self> {
        let mpv = Mpv::with_initializer(|init| {
            if wid != 0 {
                init.set_option("wid", wid)?;
            }
            // hwdec / keep-open 设失败不致命，忽略其错误但不影响初始化。
            init.set_option("hwdec", "auto").ok();
            init.set_option("keep-open", "yes").ok();
            // 渲染背景填黑：窗口透明后，视频 letterbox 边缘用黑边而非透出白底。
            init.set_option("background", "#000000").ok();
            Ok(())
        })
        .map_err(|e| AppError::Other(format!("mpv init: {e:?}")))?;
        Ok(Player { mpv })
    }

    /// 加载视频文件，可选加载外挂字幕。
    pub fn load(&self, path: &str, sub: Option<&str>) -> AppResult<()> {
        self.mpv
            .command("loadfile", &[path])
            .map_err(|e| AppError::Other(format!("loadfile: {e:?}")))?;
        if let Some(s) = sub {
            self.mpv
                .command("sub-add", &[s])
                .map_err(|e| AppError::Other(format!("sub-add: {e:?}")))?;
        }
        Ok(())
    }

    pub fn set_pause(&self, p: bool) -> AppResult<()> {
        self.setp("pause", p)
    }

    /// 相对当前位置跳转 `secs` 秒。
    pub fn seek(&self, secs: f64) -> AppResult<()> {
        self.mpv
            .command("seek", &[&secs.to_string(), "relative"])
            .map_err(|e| AppError::Other(format!("seek: {e:?}")))
    }

    /// 跳转到绝对位置 `secs` 秒。
    pub fn seek_absolute(&self, secs: f64) -> AppResult<()> {
        self.mpv
            .command("seek", &[&secs.to_string(), "absolute"])
            .map_err(|e| AppError::Other(format!("seek_absolute: {e:?}")))
    }

    pub fn set_volume(&self, v: f64) -> AppResult<()> {
        self.setp("volume", v)
    }

    /// 切换 mpv 视频窗口全屏。视频渲染在独立窗口，故全屏必须作用于 mpv
    /// 自身而非宿主 DOM。
    pub fn set_fullscreen(&self, on: bool) -> AppResult<()> {
        self.setp("fullscreen", on)
    }

    /// 停止播放并卸载当前文件。用于退出播放前的瞬时停止，
    /// 使后续 drop Player 时 libmpv 无需卸载大文件解码器，避免阻塞。
    pub fn stop(&self) -> AppResult<()> {
        self.mpv
            .command("stop", &[])
            .map_err(|e| AppError::Other(format!("stop: {e:?}")))
    }

    /// 当前播放位置（秒）。取不到（如未开始播放）时返回 0。
    pub fn position(&self) -> f64 {
        self.mpv.get_property("time-pos").unwrap_or(0.0)
    }

    /// 视频总时长（秒）。取不到时返回 0。
    pub fn duration(&self) -> f64 {
        self.mpv.get_property("duration").unwrap_or(0.0)
    }

    fn setp<T: SetData>(&self, k: &str, v: T) -> AppResult<()> {
        self.mpv
            .set_property(k, v)
            .map_err(|e| AppError::Other(format!("set {k}: {e:?}")))
    }
}
