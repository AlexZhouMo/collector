fn main() {
    // macOS 打包分发：让可执行文件在自身 .app 内的 Frameworks 目录查找 libmpv。
    // Tauri 会把 tauri.conf.json 的 bundle.macOS.frameworks 拷进
    // `Collector.app/Contents/Frameworks/`，而可执行文件位于
    // `Collector.app/Contents/MacOS/`，故 rpath 用 @executable_path/../Frameworks。
    //
    // 注意（重要局限）：仅加 rpath 不足以让分发生效。libmpv.2.dylib 的
    // install_name 是绝对路径 `/opt/homebrew/opt/mpv/lib/libmpv.2.dylib`，
    // 因此可执行文件的 load command 记录的也是该绝对路径，@rpath 不会被使用。
    // 要真正在无 mpv 的机器上运行，还需在打包后用 install_name_tool 重写引用：
    //   install_name_tool -change /opt/homebrew/opt/mpv/lib/libmpv.2.dylib \
    //       @rpath/libmpv.2.dylib \
    //       Collector.app/Contents/MacOS/Collector
    //   install_name_tool -id @rpath/libmpv.2.dylib \
    //       Collector.app/Contents/Frameworks/libmpv.2.dylib
    // 这一步需在真实打包流程/CI 中作为构建后钩子补充（见 README / 本文件注释）。
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");

    tauri_build::build()
}
