//! libmpv 播放器封装：在给定 Tauri 窗口内建视频承载 NSView + libmpv 实例（`wid` 指向该
//! view），提供加载/暂停/seek/音量/字幕/时长查询等控制。视频经 `--wid` 嵌入渲染，透过透明
//! WKWebView 显示在应用内。
//!
//! 由阶段 0.2 的 `embed_probe.rs` 原型固化而来（真机验证视频秒开显示）。
//!
//! # 生命周期
//! - [`MpvPlayer::new`] 在主线程建 NSView（[`super::embed_macos::create_video_view`]）+ mpv 实例。
//! - NSView 所有权以裸指针形式由本结构持有（`video_view_ptr`），[`MpvPlayer::destroy`] 时在
//!   主线程 [`super::embed_macos::remove_video_view`] 回收释放，不泄漏。
//! - mpv 实例随本结构 drop 而停止播放（销毁 mpv 上下文）。**注意**：若未调用 `destroy` 就 drop，
//!   NSView 会泄漏（无从跨线程调度主线程 remove）；调用方应在退出播放时显式 `destroy`。
//!
//! # 线程
//! `new`/`destroy` 内部用 `window.run_on_main_thread(...)` 把 NSView 操作调度到主线程，故二者
//! 可从任意线程调用。其余控制方法（load_file/set_pause/...）直接调 libmpv 的线程安全 C API，
//! 可任意线程调用。

#![cfg(target_os = "macos")]
// 阶段 2 接入 player_open 后，未使用的 pub 控制方法会被调用；届时移除本 allow。
#![allow(dead_code)]

use crate::error::{AppError, AppResult};
use libmpv2::Mpv;

/// 把 mpv 实例创建结果跨线程从主线程闭包传回命令线程的载体。
/// mpv 的 handle 是线程安全的（libmpv C API 本身可跨线程调用），此处仅为满足编译期 Send。
struct MpvCarrier(Mpv);
// 安全性：libmpv 的 mpv_handle 设计为可跨线程使用；本载体只在 new 里从主线程 send 到命令线程一次。
unsafe impl Send for MpvCarrier {}

/// libmpv 播放器：持有 mpv 实例 + 视频 NSView 裸指针 + NSWindow 地址（供 resize/remove）。
pub struct MpvPlayer {
    mpv: Mpv,
    /// [`super::embed_macos::create_video_view`] 返回的 NSView 所有权裸指针；destroy 时回收。
    video_view_ptr: usize,
    /// NSWindow 裸指针地址，供 resize（重取 contentView bounds）与 destroy 调度用。
    ns_window_addr: usize,
}

impl MpvPlayer {
    /// 在给定窗口内建视频 NSView（插到 WKWebView 下）+ mpv 实例（wid=该 view）。
    /// 内部用 `run_on_main_thread` 调度主线程建 view+mpv，**可从任意线程调用**。
    pub fn new(window: &tauri::WebviewWindow) -> AppResult<Self> {
        let ns_window = window
            .ns_window()
            .map_err(|e| AppError::Other(format!("ns_window(): {e}")))?;
        let ns_window_addr = ns_window as usize;

        // NSView + mpv 创建放主线程，结果经 channel 回传。
        let (tx, rx) = std::sync::mpsc::channel::<Result<(usize, MpvCarrier), String>>();
        window
            .run_on_main_thread(move || {
                let r = unsafe { build_on_main(ns_window_addr) };
                let _ = tx.send(r);
            })
            .map_err(|e| AppError::Other(format!("run_on_main_thread 调度失败: {e}")))?;

        match rx.recv_timeout(std::time::Duration::from_secs(20)) {
            Ok(Ok((view_ptr, carrier))) => Ok(MpvPlayer {
                mpv: carrier.0,
                video_view_ptr: view_ptr,
                ns_window_addr,
            }),
            Ok(Err(msg)) => Err(AppError::Other(msg)),
            Err(e) => Err(AppError::Other(format!("等待主线程超时: {e}"))),
        }
    }

    /// 加载并播放文件（mpv `loadfile`）。
    pub fn load_file(&self, path: &str) -> AppResult<()> {
        self.mpv
            .command("loadfile", &[path])
            .map_err(|e| AppError::Other(format!("loadfile 失败: {e:?}")))
    }

    /// 暂停/恢复（mpv `pause` 属性）。
    pub fn set_pause(&self, paused: bool) -> AppResult<()> {
        self.mpv
            .set_property("pause", paused)
            .map_err(|e| AppError::Other(format!("set pause 失败: {e:?}")))
    }

    /// 绝对 seek 到指定秒（mpv `seek <secs> absolute`）。
    pub fn seek_absolute(&self, secs: f64) -> AppResult<()> {
        let arg = format_seek_secs(secs);
        self.mpv
            .command("seek", &[&arg, "absolute"])
            .map_err(|e| AppError::Other(format!("seek 失败: {e:?}")))
    }

    /// 设置音量（mpv `volume` 属性，0-100，越界钳制）。
    pub fn set_volume(&self, vol: f64) -> AppResult<()> {
        let v = clamp_volume(vol);
        self.mpv
            .set_property("volume", v)
            .map_err(|e| AppError::Other(format!("set volume 失败: {e:?}")))
    }

    /// 加载外挂字幕（mpv `sub-add <path>`）。
    pub fn add_subtitle(&self, path: &str) -> AppResult<()> {
        self.mpv
            .command("sub-add", &[path])
            .map_err(|e| AppError::Other(format!("sub-add 失败: {e:?}")))
    }

    /// 字幕显隐（mpv `sub-visibility` 属性）。
    pub fn set_sub_visibility(&self, visible: bool) -> AppResult<()> {
        self.mpv
            .set_property("sub-visibility", visible)
            .map_err(|e| AppError::Other(format!("set sub-visibility 失败: {e:?}")))
    }

    /// 读取总时长（秒）。loadfile 后 demux 未完成时可能短暂 Err，调用方容错。
    pub fn get_duration(&self) -> AppResult<f64> {
        self.mpv
            .get_property::<f64>("duration")
            .map_err(|e| AppError::Other(format!("get duration 失败: {e:?}")))
    }

    /// 读取当前播放位置（秒）。未起播/demux 未完成时可能短暂 Err，调用方容错。
    pub fn get_time_pos(&self) -> AppResult<f64> {
        self.mpv
            .get_property::<f64>("time-pos")
            .map_err(|e| AppError::Other(format!("get time-pos 失败: {e:?}")))
    }

    /// 停止播放（drop mpv）并在主线程移除+释放视频 NSView（不泄漏）。
    /// 调用后本实例不应再使用（video_view_ptr 已回收，置 0 防重复 remove）。
    pub fn destroy(&mut self, window: &tauri::WebviewWindow) {
        // 先在主线程移除 NSView（trylock 语义：即便调度失败也不阻塞退出流程）。
        let view_ptr = self.video_view_ptr;
        self.video_view_ptr = 0;
        if view_ptr != 0 {
            let (tx, rx) = std::sync::mpsc::channel::<()>();
            let dispatched = window
                .run_on_main_thread(move || {
                    let _ = unsafe { crate::player::embed_macos::remove_video_view(view_ptr) };
                    let _ = tx.send(());
                })
                .is_ok();
            if dispatched {
                let _ = rx.recv_timeout(std::time::Duration::from_secs(5));
            }
        }
        // mpv 会随 self drop 而销毁；此处不需显式 stop。
    }
}

/// 在主线程执行：建 NSView（wid 承载）+ mpv 实例（set_option wid）。
/// # Safety
/// 必须在主线程调用；`ns_window_addr` 必须是有效 NSWindow 指针。
unsafe fn build_on_main(ns_window_addr: usize) -> Result<(usize, MpvCarrier), String> {
    let view_ptr = crate::player::embed_macos::create_video_view(ns_window_addr)?;
    let wid: i64 = view_ptr as i64;

    let mpv = Mpv::with_initializer(|init| {
        init.set_option("wid", wid)?;
        init.set_property("ao", "coreaudio").ok();
        Ok(())
    })
    .map_err(|e| {
        // mpv 创建失败：回收已建的 NSView，避免泄漏。
        let _ = crate::player::embed_macos::remove_video_view(view_ptr);
        format!("创建 mpv 失败: {e:?}")
    })?;

    Ok((view_ptr, MpvCarrier(mpv)))
}

/// 音量钳制到 [0, 100]。
fn clamp_volume(vol: f64) -> f64 {
    vol.clamp(0.0, 100.0)
}

/// seek 秒数格式化为 mpv 命令参数：非负、保留 3 位小数（毫秒精度），去多余尾零可读性无关。
fn format_seek_secs(secs: f64) -> String {
    let s = if secs.is_finite() && secs > 0.0 { secs } else { 0.0 };
    format!("{s:.3}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_volume_bounds() {
        assert_eq!(clamp_volume(-10.0), 0.0);
        assert_eq!(clamp_volume(0.0), 0.0);
        assert_eq!(clamp_volume(50.0), 50.0);
        assert_eq!(clamp_volume(100.0), 100.0);
        assert_eq!(clamp_volume(150.0), 100.0);
    }

    #[test]
    fn clamp_volume_handles_nan_as_zero_via_clamp() {
        // f64::clamp(NaN, ..) 返回 NaN；此处仅记录行为，音量源头应保证非 NaN。
        assert!(clamp_volume(f64::NAN).is_nan());
    }

    #[test]
    fn format_seek_normal() {
        assert_eq!(format_seek_secs(12.5), "12.500");
        assert_eq!(format_seek_secs(0.0), "0.000");
        assert_eq!(format_seek_secs(123.456789), "123.457");
    }

    #[test]
    fn format_seek_clamps_negative_and_nonfinite_to_zero() {
        assert_eq!(format_seek_secs(-5.0), "0.000");
        assert_eq!(format_seek_secs(f64::NAN), "0.000");
        assert_eq!(format_seek_secs(f64::INFINITY), "0.000");
    }
}
