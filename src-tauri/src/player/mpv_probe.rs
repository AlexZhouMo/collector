//! 阶段 0 可行性验证：验证 libmpv2 (libmpv FFI) 能否创建实例并 demux/probe 本机 mkv。
//! 这是"视频秒开 libmpv"方案的 go/no-go 闸门验证代码，不参与正式播放路径。
//!
//! 验证目标：
//! - 通过 FFI 创建 mpv 实例（确认链接 libmpv.dylib 成功）
//! - 无视频输出（vo=null）加载 mkv，不弹窗
//! - 读到 duration ≈ 6365s
//! - 识别到 h264 视频轨 + ac3 音频轨
//!
//! 运行：`cargo test mpv_probe -- --nocapture`

#[cfg(test)]
mod tests {
    use libmpv2::Mpv;
    use std::thread;
    use std::time::{Duration, Instant};

    const TEST_MKV: &str = "/Users/zhoumo/Downloads/电影/恐怖/安娜贝尔/[2019].安娜贝尔3：回家.mkv";

    #[test]
    fn probe_mkv_via_libmpv() {
        // 1. 创建 mpv 实例，初始化阶段设置为无音视频输出（纯 demux/probe，不弹窗不出声）。
        let mpv = Mpv::with_initializer(|init| {
            init.set_property("vo", "null")?;
            init.set_property("ao", "null")?;
            // 只 probe，不真播；pause 避免无谓解码。
            Ok(())
        })
        .expect("创建 mpv 实例失败（FFI 链路或 API 版本不匹配）");

        eprintln!("[probe] mpv 实例创建成功");

        // 2. 加载文件。loadfile 是异步命令，返回 Ok 仅表示命令入队成功。
        mpv.command("loadfile", &[TEST_MKV])
            .expect("loadfile 命令失败");
        eprintln!("[probe] loadfile 已入队: {TEST_MKV}");

        // 3. 轮询 duration，直到读到有效值或超时。文件打开+demux 需要一点时间。
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut duration: Option<f64> = None;
        while Instant::now() < deadline {
            if let Ok(d) = mpv.get_property::<f64>("duration") {
                if d > 0.0 {
                    duration = Some(d);
                    break;
                }
            }
            thread::sleep(Duration::from_millis(200));
        }

        let dur = duration.expect("超时未读到 duration（文件未成功 demux）");
        eprintln!("[probe] duration = {dur:.2} 秒");
        assert!(
            (dur - 6365.0).abs() < 60.0,
            "duration {dur} 与预期 6365s 偏差过大"
        );

        // 4. 读取轨道信息（track-list 属性以 JSON 字符串返回）。
        let track_json = mpv
            .get_property::<String>("track-list")
            .expect("读取 track-list 失败");
        let tracks: serde_json::Value =
            serde_json::from_str(&track_json).expect("track-list JSON 解析失败");

        let mut has_h264 = false;
        let mut has_ac3 = false;
        if let Some(arr) = tracks.as_array() {
            for t in arr {
                let ttype = t.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let codec = t.get("codec").and_then(|v| v.as_str()).unwrap_or("");
                eprintln!("[probe] track type={ttype} codec={codec}");
                if ttype == "video" && codec.contains("h264") {
                    has_h264 = true;
                }
                if ttype == "audio" && codec.contains("ac3") {
                    has_ac3 = true;
                }
            }
        }

        assert!(has_h264, "未识别到 h264 视频轨");
        assert!(has_ac3, "未识别到 ac3 音频轨");
        eprintln!("[probe] 验证通过：h264 视频轨 + ac3 音频轨均识别");
    }
}
