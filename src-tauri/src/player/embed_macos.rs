//! NSView 嵌入管理：在 Tauri 窗口内容区创建/移除/resize 一个原生 NSView 作为 libmpv
//! 的渲染承载视图（`--wid`）。视频透过透明 WKWebView 显示在应用内。
//!
//! 由阶段 0.2 的 `embed_probe.rs` 原型固化而来（NSWindow→contentView→NSView→addSubview
//! 已真机验证）。区别：probe 用 `std::mem::forget` 泄漏 NSView（进程期不释放），本模块
//! 用「所有权裸指针」方案支持 remove——create 用 `Retained::into_raw` 转移所有权得裸指针
//! 交调用方持有；remove 用 `Retained::from_raw` 回收所有权 → removeFromSuperview → drop 释放。
//! 不泄漏。
//!
//! # 线程约束
//! 所有函数操作 NSView/NSWindow（MainThreadOnly），**必须在主线程调用**（内部用
//! `MainThreadMarker::new()` 断言，非主线程返回 Err）。调用方负责用
//! `window.run_on_main_thread(...)` 调度。

#![cfg(target_os = "macos")]
// 阶段 2 接入后 resize_video_view（窗口 resize/全屏）会被调用；届时移除本 allow。
#![allow(dead_code)]

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSView, NSWindow, NSWindowOrderingMode};
use objc2_foundation::NSRect;

/// 从 NSWindow 裸指针 retain 出 contentView。
/// # Safety
/// 必须在主线程；`ns_window_addr` 必须是有效 NSWindow 指针。
unsafe fn content_view(
    _mtm: MainThreadMarker,
    ns_window_addr: usize,
) -> Result<Retained<NSView>, String> {
    let ns_window_ptr = ns_window_addr as *mut NSWindow;
    if ns_window_ptr.is_null() {
        return Err("NSWindow 指针为空".into());
    }
    let window: Retained<NSWindow> =
        Retained::retain(ns_window_ptr).ok_or_else(|| "retain NSWindow 失败".to_string())?;
    window
        .contentView()
        .ok_or_else(|| "contentView 为空".to_string())
}

/// 在窗口内容区创建 mpv 承载 NSView（插到 WKWebView 之下），返回其裸指针(usize)供 wid 用。
///
/// 视图覆盖整个内容区、随窗口自适应、wantsLayer=true。返回的裸指针**持有一份所有权引用**
/// （`Retained::into_raw`），调用方须最终用 [`remove_video_view`] 回收，否则泄漏。
///
/// # Safety
/// 必须在主线程调用；`ns_window_addr` 必须是有效 NSWindow 指针。
pub unsafe fn create_video_view(ns_window_addr: usize) -> Result<usize, String> {
    let mtm = MainThreadMarker::new().ok_or_else(|| "create_video_view 不在主线程".to_string())?;

    let content = content_view(mtm, ns_window_addr)?;
    // bounds 才是自身坐标系（frame 原点相对父视图）。
    let bounds: NSRect = content.bounds();

    let view: Retained<NSView> = {
        let alloc = NSView::alloc(mtm);
        NSView::initWithFrame(alloc, bounds)
    };
    view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    view.setWantsLayer(true);

    // 插到所有现有 subview（含 WKWebView）之下。
    content.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Below, None);

    // 转移所有权为裸指针交调用方持有（不释放，remove 时回收）。
    Ok(Retained::into_raw(view) as usize)
}

/// 移除并释放由 [`create_video_view`] 创建的 NSView：从父视图 removeFromSuperview + 回收所有权 drop。
///
/// # Safety
/// 必须在主线程调用；`view_ptr` 必须是 [`create_video_view`] 返回过、且尚未 remove 的指针
/// （每个指针只能 remove 一次，否则 double-free）。
pub unsafe fn remove_video_view(view_ptr: usize) -> Result<(), String> {
    let _mtm =
        MainThreadMarker::new().ok_or_else(|| "remove_video_view 不在主线程".to_string())?;
    if view_ptr == 0 {
        return Err("view 指针为空".into());
    }
    let ptr = view_ptr as *mut NSView;
    // 回收 create 时 into_raw 转移出的所有权。
    let view: Retained<NSView> = Retained::from_raw(ptr).ok_or_else(|| "from_raw 失败".to_string())?;
    view.removeFromSuperview();
    // view 离开作用域 → drop → 释放最后一个强引用。
    drop(view);
    Ok(())
}

/// 把视频 NSView 尺寸同步为内容区 bounds（窗口 resize/全屏时调用）。
/// 视图已设 autoresizing mask，通常无需手动 resize；本函数用于强制同步的兜底。
///
/// # Safety
/// 必须在主线程调用；`ns_window_addr`/`view_ptr` 必须有效。此处仅 retain 借用 view（不转移
/// 所有权，故不影响 create/remove 的所有权计数）。
pub unsafe fn resize_video_view(ns_window_addr: usize, view_ptr: usize) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or_else(|| "resize_video_view 不在主线程".to_string())?;
    if view_ptr == 0 {
        return Err("view 指针为空".into());
    }
    let content = content_view(mtm, ns_window_addr)?;
    let bounds: NSRect = content.bounds();

    let ptr = view_ptr as *mut NSView;
    // 临时 retain 借用（+1/-1 平衡，不改变所有权），设 frame。
    let view: Retained<NSView> =
        Retained::retain(ptr).ok_or_else(|| "retain view 失败".to_string())?;
    view.setFrame(bounds);
    // view drop 释放这次临时 retain，调用方持有的所有权不受影响。
    Ok(())
}
