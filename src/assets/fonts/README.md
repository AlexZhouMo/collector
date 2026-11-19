# 打包字体

- `NotoSansCJKsc-Regular.woff2` — Noto Sans SC（思源黑体简体），SIL Open Font License 1.1。
  作为视频字幕渲染（SubtitlesOctopus/libass）的 CJK 兜底字体（`fallbackFont`），
  保证字幕中文在 macOS / Windows 上都不会因缺字体而显示为方框。
  来源：https://fontsource.org/fonts/noto-sans-sc
  注意：这是按常用简体字切出的子集，不含 `♪` `∮` 等符号。

- `public/fonts/NotoSansJP-Symbols.woff2`（不在本目录）— Noto Sans JP 的符号子集，SIL Open Font License 1.1。
  只含 8 个字形：`♪ ♫ ♬ ♩ ♭ ♮ ♯`（U+2669–266F）与 `∮`（U+222E），1.4 KB，
  补上面那个 CJK 子集缺的音乐符（缺了字幕里会画成方框）。与 Noto Sans SC 同一套设计，字形风格一致。
  选 JP 而不是 SC 是为了字体族名不同（`Noto Sans JP`）：libass 按字符找替补字体时不会和兜底字体混淆。
  放在 `public/fonts/` 是因为它小于 4 KB，放进 `src/assets/` 会被 Vite 内联成 data: URL，libass worker 无法加载。
  来源：Google Fonts 按字符取子集
  `https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@400&text=♪♫♬♩∮♭♮♯`（字符需 URL 编码）。
