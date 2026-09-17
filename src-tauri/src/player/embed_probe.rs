//! 阶段 0.2 可行性验证（go/no-go 闸门）：把 libmpv 视频渲染嵌入 Tauri 窗口内的一个
//! 原生 NSView，让视频透过透明 WKWebView 显示在应用内。
//!
//! 技术路径 = 子选项 A（--wid 嵌入）：
//!   1. Tauri `WebviewWindow::ns_window()` 拿 NSWindow 指针。
//!   2. objc2-app-kit 从该 NSWindow 拿 contentView，新建一个覆盖内容区的 NSView，
//!      用 addSubview:positioned:relativeTo: 以 NSWindowBelow 插到 WKWebView 之下。
//!   3. 创建 Mpv 实例，在初始化阶段 set_option("wid", <NSView 指针 as i64>)，
//!      loadfile 播放本机 mkv。mpv 自己在该 NSView 里建渲染层。
//!   4. Mpv 实例存入全局 static 防 drop（drop 会销毁播放）。
//!
//! 所有 NSView 操作必须在主线程（NSView 是 MainThreadOnly），故整个流程放进
//! `window.run_on_main_thread(...)` 闭包内执行。
//!
//! 触发：前端 `invoke("mpv_embed_probe")`（SettingsView 顶部"测试 mpv 嵌入"按钮）。
//!
//! 注意：本命令是独立的探索验证，不参与正式播放路径（player_open 等回退基线不受影响）。

#![cfg(target_os = "macos")]

use crate::error::{AppError, AppResult};
use libmpv2::Mpv;
use std::sync::{Mutex, OnceLock};

const TEST_MKV: &str = "/Users/zhoumo/Downloads/电影/恐怖/安娜贝尔/[2019].安娜贝尔3：回家.mkv";

/// 全局持有嵌入播放的 Mpv 实例，防止 drop（drop 会销毁 mpv 上下文停止播放）。
/// 同时持有 NSView 的 Retained，防止其被释放（mpv 仍在往里渲染）。
static EMBED: OnceLock<Mutex<Option<EmbedHandle>>> = OnceLock::new();

struct EmbedHandle {
    _mpv: Mpv,
    // 保活：NSView 的 Retained 指针。用 usize 存裸指针值以便跨线程 Send。
    // 实际 Retained 在主线程 forget 后由本字段的存在语义上"持有"（不再显式释放）。
    _view_ptr: usize,
}

// EmbedHandle 里的裸指针不自动 Send，这里手动标注：仅在验证期使用，NSView 生命周期
// 交给全局 static 持有到进程退出，不跨线程解引用。
unsafe impl Send for EmbedHandle {}

/// 临时验证入口：在当前窗口内嵌入 libmpv 播放测试 mkv。
/// 由 lib.rs 的 `mpv_embed_probe` tauri 命令包装调用（本函数不直接注册为命令）。
pub fn mpv_embed_probe(window: tauri::WebviewWindow) -> AppResult<()> {
    // 1. 拿 NSWindow 指针（可在任意线程调用，返回裸指针）。
    let ns_window = window
        .ns_window()
        .map_err(|e| AppError::Other(format!("ns_window(): {e}")))?;
    let ns_window_addr = ns_window as usize;

    // 2. 所有 NSView / mpv 创建放到主线程执行。
    //    结果通过 channel 回传（run_on_main_thread 的闭包返回 ()）。
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    window
        .run_on_main_thread(move || {
            let r = unsafe { build_embed_on_main(ns_window_addr) };
            let _ = tx.send(r);
        })
        .map_err(|e| AppError::Other(format!("run_on_main_thread 调度失败: {e}")))?;

    // 等主线程执行结果（超时保护，避免主线程卡住时永久阻塞命令线程）。
    match rx.recv_timeout(std::time::Duration::from_secs(20)) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(msg)) => Err(AppError::Other(msg)),
        Err(e) => Err(AppError::Other(format!("等待主线程超时: {e}"))),
    }
}

/// 在主线程执行：建 NSView、插到 WKWebView 下方、建 mpv 设 wid、loadfile。
/// # Safety
/// 必须在主线程调用；ns_window_addr 必须是有效 NSWindow 指针。
unsafe fn build_embed_on_main(ns_window_addr: usize) -> Result<(), String> {
    use objc2::rc::Retained;
    use objc2::{MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{NSAutoresizingMaskOptions, NSView, NSWindow, NSWindowOrderingMode};
    use objc2_foundation::NSRect;

    let mtm = MainThreadMarker::new().ok_or_else(|| "不在主线程".to_string())?;

    // 从裸指针恢复 NSWindow（retain 一份，避免误 over-release）。
    let ns_window_ptr = ns_window_addr as *mut NSWindow;
    if ns_window_ptr.is_null() {
        return Err("NSWindow 指针为空".into());
    }
    let window: Retained<NSWindow> =
        Retained::retain(ns_window_ptr).ok_or_else(|| "retain NSWindow 失败".to_string())?;

    // 拿 contentView（WKWebView 是它的 subview）。
    let content: Retained<NSView> = window
        .contentView()
        .ok_or_else(|| "contentView 为空".to_string())?;

    // 内容区尺寸：用 contentView 的 bounds（frame 原点相对父视图，bounds 才是自身坐标系）。
    let bounds: NSRect = content.bounds();

    // 建覆盖整个内容区的 mpv 承载 NSView。
    let mpv_view: Retained<NSView> = {
        let alloc = NSView::alloc(mtm);
        NSView::initWithFrame(alloc, bounds)
    };
    // 随窗口尺寸自适应。
    mpv_view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    // 让其有独立 layer（mpv 的 macOS vo 期望目标视图可托管 layer）。
    mpv_view.setWantsLayer(true);

    // 插到所有现有 subview（含 WKWebView）之下：positioned=Below, relativeTo=None。
    content.addSubview_positioned_relativeTo(&mpv_view, NSWindowOrderingMode::Below, None);

    // mpv 承载视图的裸指针（i64 传给 wid）。
    let view_ptr = Retained::as_ptr(&mpv_view) as usize;
    let wid: i64 = view_ptr as i64;

    // 3. 创建 mpv，初始化阶段设 wid（必须在 vo 初始化前设，故用 set_option）。
    let mpv = Mpv::with_initializer(|init| {
        init.set_option("wid", wid)?;
        // 让 mpv 使用默认 macOS vo（不设 vo=null，否则不出画面）。
        init.set_property("ao", "coreaudio").ok();
        Ok(())
    })
    .map_err(|e| format!("创建 mpv 失败: {e:?}"))?;

    mpv.command("loadfile", &[TEST_MKV])
        .map_err(|e| format!("loadfile 失败: {e:?}"))?;

    eprintln!("[embed_probe] NSWindow={ns_window_addr:#x} mpv_view={view_ptr:#x} wid={wid} loadfile 已入队");

    // 4. 保活：NSView 的 Retained forget（生命周期交给 mpv/窗口，进程期不释放）；
    //    Mpv 存全局 static。
    std::mem::forget(mpv_view);
    let handle = EmbedHandle {
        _mpv: mpv,
        _view_ptr: view_ptr,
    };
    let slot = EMBED.get_or_init(|| Mutex::new(None));
    *slot.lock().unwrap() = Some(handle);

    Ok(())
}
