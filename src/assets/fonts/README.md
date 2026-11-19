# 打包字体

- `NotoSansSC-Regular.otf` — Noto Sans SC（思源黑体简体）Regular 完整版，SIL Open Font License 1.1。
  视频字幕渲染（SubtitlesOctopus/libass）唯一的字体，同时作为兜底字体（`fallbackFont`）：
  字幕样式里指名的 SimHei / Arial 并不随包携带，全部落到它上面，保证 macOS / Windows 显示一致。
  来源：Noto CJK 官方仓库
  `https://github.com/notofonts/noto-cjk/blob/main/Sans/SubsetOTF/SC/NotoSansSC-Regular.otf`
  （8,331,336 字节，SHA-256 `faa6c9df652116dde789d351359f3d7e5d2285a2b2a1f04a2d7244df706d5ea9`）。
  覆盖 30,890 个字符：简繁汉字、日文假名、拉丁扩展、希腊/西里尔字母、`℃ → Ⅰ √ ♡ ☆` 等符号，
  以及音乐符 `♪ ♫ ♬ ♩ ♭ ♮ ♯` 与 `∮`。

  2026-11-20 之前用的是 fontsource 的常用字子集（7,946 字符，1.1 MB）加一个 8 字形的音乐符补充字体。
  实测字幕库里有 287 个字符不在其中（生僻/繁体汉字、俄文和希腊字母、若干符号），播放时全部画成方框，
  因此换成完整版，补充字体随之删除。

  换字体后可以用字幕库的全部字符核对覆盖情况：libass 找不到字形时会在控制台报
  `Glyph 0x… not found in font`。
