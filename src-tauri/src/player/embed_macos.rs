//! macOS 原生视图集成：把 mpv 渲染用的 NSView 作为主窗 WebView 的子视图嵌入。
//!
//! 相比"独立窗口定位覆盖"，子视图方案用同窗口相对坐标、天然跟随窗口移动缩放、
//! 无独立窗口白底、不遮挡 stage 外的 UI。所有 NSView 操作必须在主线程执行
//! （调用方通过 Tauri 的 run_on_main_thread 保证）。
#![cfg(target_os = "macos")]

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::NSView;
use objc2_foundation::{NSPoint, NSRect, NSSize};

/// 把 Tauri 的 `ns_view()` 原始指针借用为 `&NSView`。
///
/// # Safety
/// `ptr` 必须是有效的、存活的 NSView 指针（Tauri 主窗 WebView 的 content view，
/// 生命周期 = 主窗生命周期）。仅在主线程调用。
unsafe fn as_nsview<'a>(ptr: *mut c_void) -> &'a NSView {
    &*(ptr as *const NSView)
}

/// 计算子视图 frame。父视图坐标系：若 isFlipped 则原点在左上、y 向下
/// （与 DOM 一致，直接用）；否则原点在左下、y 向上，需翻转。
fn subview_frame(parent: &NSView, x: f64, y: f64, w: f64, h: f64) -> NSRect {
    let origin_y = if parent.isFlipped() {
        y
    } else {
        let ph = parent.frame().size.height;
        ph - y - h
    };
    NSRect::new(NSPoint::new(x, origin_y), NSSize::new(w, h))
}

/// 创建 mpv 渲染子视图，加到 parent（主窗 WebView content view）之上，
/// 定位到 (x,y,w,h)（DOM 逻辑坐标，相对视口左上）。返回子视图指针作为 mpv 的 wid。
///
/// # Safety
/// 见 `as_nsview`；必须在主线程调用。
pub unsafe fn create_mpv_view(parent_ptr: *mut c_void, x: f64, y: f64, w: f64, h: f64) -> *mut c_void {
    let mtm = MainThreadMarker::new().expect("create_mpv_view must run on main thread");
    let parent = as_nsview(parent_ptr);
    let frame = subview_frame(parent, x, y, w, h);
    // alloc + initWithFrame 建一个普通 NSView 作为 mpv 渲染目标
    let view: Retained<NSView> = {
        let allocated = mtm.alloc::<NSView>();
        NSView::initWithFrame(allocated, frame)
    };
    view.setWantsLayer(true); // layer-backed，利于 mpv/GL 合成
    parent.addSubview(&view);
    // 泄漏一份 Retained 交给调用方持有（remove 时再释放）——返回裸指针，
    // 所有权由 EmbedState 逻辑管理。用 Retained::into_raw 保住引用计数。
    Retained::into_raw(view) as *mut c_void
}

/// 更新子视图 frame（stage 尺寸/位置变化时）。
///
/// # Safety
/// `view_ptr`/`parent_ptr` 必须有效；主线程调用。
pub unsafe fn set_mpv_view_frame(
    view_ptr: *mut c_void,
    parent_ptr: *mut c_void,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) {
    let parent = as_nsview(parent_ptr);
    let view = as_nsview(view_ptr);
    let frame = subview_frame(parent, x, y, w, h);
    view.setFrame(frame);
}

/// 移除并释放子视图（退出播放时）。消费掉 create 时泄漏的那份 Retained。
///
/// # Safety
/// `view_ptr` 必须是 `create_mpv_view` 返回的有效指针，且只调用一次；主线程调用。
pub unsafe fn remove_mpv_view(view_ptr: *mut c_void) {
    let view = Retained::from_raw(NonNull::new(view_ptr as *mut NSView).unwrap().as_ptr());
    if let Some(view) = view {
        view.removeFromSuperview();
        // view 在此作用域结束时 drop，释放 create 时保留的引用计数。
    }
}
