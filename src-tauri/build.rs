fn main() {
    // libmpv2-sys 默认只发 `cargo:rustc-link-lib=mpv`，不提供 link-search。
    // 本机 libmpv 由 Homebrew 安装于 /opt/homebrew/lib（libmpv.dylib -> Cellar/mpv）。
    // 阶段 0 验证：显式加入 link-search 让链接器找到 libmpv。
    println!("cargo:rustc-link-search=native=/opt/homebrew/lib");

    tauri_build::build()
}
