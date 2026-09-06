# Collector — 打包与 libmpv 分发说明

## macOS 打包（libmpv 随包分发）

Collector 依赖 `libmpv`。为了让 `.app` 分发到**未安装 mpv** 的 macOS 机器上仍能运行，
需要把 `libmpv.2.dylib` 打进 app 并让可执行文件在 app 内部定位到它。

已配置的部分（自动生效）：

1. **`src-tauri/tauri.conf.json` → `bundle.macOS.frameworks`**：把
   `/opt/homebrew/Cellar/mpv/0.41.0_9/lib/libmpv.2.dylib` 拷进
   `Collector.app/Contents/Frameworks/`。（指向 Cellar 里的真实文件，而非符号链接。
   升级 mpv 后需更新此版本号路径。）
2. **`src-tauri/build.rs`**：向可执行文件注入 rpath
   `@executable_path/../Frameworks`，使其能在 app 内的 Frameworks 目录搜索库。

必须手动补充的部分（**当前未自动化**）：

`libmpv.2.dylib` 自身的 `install_name` 是绝对路径
`/opt/homebrew/opt/mpv/lib/libmpv.2.dylib`，所以 `tauri build` 产出的可执行文件
load command 记录的也是这个绝对路径 —— **@rpath 不会被使用**，在没装 mpv 的机器上
仍会报库找不到。必须在打包后运行重定位脚本把引用改成 `@rpath/libmpv.2.dylib`：

```bash
npm run tauri build            # 产出 Collector.app（DMG 步骤在本机可能失败，可忽略）
./scripts/relocate-libmpv.sh   # 后处理：install_name_tool 重写引用为 @rpath
```

`scripts/relocate-libmpv.sh` 会：
- `install_name_tool -id @rpath/libmpv.2.dylib <Frameworks/libmpv.2.dylib>`
- `install_name_tool -change /opt/homebrew/opt/mpv/lib/libmpv.2.dylib @rpath/libmpv.2.dylib <MacOS/Collector>`

> 注意：`install_name_tool` 会使代码签名失效。正式分发时需在此步骤之后**重新签名并公证**
> （`codesign --force --deep --sign ...`），再打 DMG。CI 应把
> `build → relocate-libmpv.sh → codesign → notarize → dmg` 串成流水线。

### 验证状态（诚实标注）

- ✅ `.app` 能产出；`Contents/Frameworks/libmpv.2.dylib` 存在；rpath 已注入。
- ⚠️ `tauri build` 直接产物的可执行文件对 libmpv 的引用**仍是绝对路径**，@rpath 未生效。
- ✅ `relocate-libmpv.sh` 的 `install_name_tool` 重写已在产物副本上验证：
  引用成功改为 `@rpath/libmpv.2.dylib`。
- ❌ **未在干净（无 mpv）的机器上实测运行**——理论上重定位后应可加载，但本环境未做端到端验证。

## 平台可移植性（`src-tauri/.cargo/config.toml`）

`.cargo/config.toml` 的 `-L` 链接搜索路径是**本机 Homebrew 安装位置**专用：

- Apple Silicon（`aarch64-apple-darwin`）：`/opt/homebrew/lib`
- Intel（`x86_64-apple-darwin`）：`/usr/local/lib`
- Windows：`libmpv` 以 `libmpv-2.dll` 分发，需另配 `-L` 指向 SDK 的 lib 目录，
  并把 DLL 作为 resource 打进包（**本仓库暂无 Windows 环境，未实现**）。

CI / 异构环境建议改为在 `build.rs` 里用 `brew --prefix mpv` 动态探测路径并 emit
`cargo:rustc-link-search`，而非硬编码。Intel 机器上打包同样需要把
`relocate-libmpv.sh` 里的 `OLD_REF` 改为 `/usr/local/opt/mpv/lib/libmpv.2.dylib`。
