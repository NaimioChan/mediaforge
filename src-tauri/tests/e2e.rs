//! 端到端测试：真跑 ffmpeg，回读产物验证。
//!
//! 这些测试不允许「找不到 ffmpeg 就静默跳过」——静默通过等于没测。
//! 找不到工具就让测试失败，提示先跑 `scripts/fetch-ffmpeg.ps1`。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use mediaforge_lib::args::{self, Options};
use mediaforge_lib::caps::Caps;
use mediaforge_lib::formats::by_id;
use mediaforge_lib::paths::ToolPaths;
use mediaforge_lib::probe::{Probe, parse as parse_probe};
use mediaforge_lib::runner::{self, Outcome};

// ---------- 公共设施 ----------

fn tools() -> ToolPaths {
    ToolPaths::locate(None).unwrap_or_else(|| {
        panic!("找不到 ffmpeg。先运行 scripts/fetch-ffmpeg.ps1 把 9 个文件放到 vendor/ffmpeg/bin/")
    })
}

/// 一个自清理的临时目录。
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir();
        let unique = format!(
            "mediaforge-e2e-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let path = base.join(unique);
        std::fs::create_dir_all(&path).expect("创建临时目录失败");
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn caps() -> Caps {
    let t = tools();
    runner::probe_capabilities(&t.ffmpeg).expect("能力探测失败")
}

/// 用 ffmpeg 的 lavfi 造测试素材，不依赖任何外部文件。
fn make_source(dir: &TempDir, name: &str, args: &[&str]) -> PathBuf {
    let t = tools();
    let out = dir.join(name);

    let mut command = Command::new(&t.ffmpeg);
    command.args(["-hide_banner", "-loglevel", "error", "-y"]);
    command.args(args);
    command.arg(&out);

    let output = command.output().expect("无法启动 ffmpeg");
    assert!(
        output.status.success(),
        "生成测试素材 {name} 失败：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(out.is_file(), "生成测试素材 {name} 后文件不存在");
    out
}

/// 造一段 2 秒、320×240、25fps 的有声视频。
fn make_video_with_audio(dir: &TempDir, name: &str) -> PathBuf {
    make_source(
        dir,
        name,
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=25:duration=2",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-shortest",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ],
    )
}

/// 造一段没有音轨的视频。
fn make_silent_video(dir: &TempDir, name: &str) -> PathBuf {
    make_source(
        dir,
        name,
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=25:duration=2",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-an",
        ],
    )
}

fn make_srt(dir: &TempDir, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(
        &path,
        "1\n00:00:00,500 --> 00:00:01,500\nhello world\n\n2\n00:00:02,000 --> 00:00:02,800\nsecond line\n",
    )
    .expect("写入 srt 失败");
    path
}

fn probe_of(path: &Path) -> Probe {
    let t = tools();
    let json = runner::run_ffprobe(&t.ffprobe, path).expect("ffprobe 失败");
    parse_probe(&json, &path.to_string_lossy()).expect("解析 ffprobe 输出失败")
}

/// 跑一次完整转换。返回产物路径与结局。
fn convert(source: &Path, out: &Path, format_id: &str, options: &Options) -> (PathBuf, Outcome) {
    let t = tools();
    let target = by_id(format_id).unwrap_or_else(|| panic!("格式 {format_id} 不在表里"));
    let probe = probe_of(source);
    let caps = caps();

    let plan = args::build_plan(
        &source.to_string_lossy(),
        &out.to_string_lossy(),
        target,
        options,
        &probe,
        &caps,
    )
    .expect("参数构建失败");

    let cancel = Arc::new(AtomicBool::new(false));
    let outcome = runner::run_ffmpeg(
        &t.ffmpeg,
        &plan.args,
        out,
        probe.duration_secs,
        &cancel,
        None,
    );

    (out.to_path_buf(), outcome)
}

fn assert_ok(outcome: &Outcome) {
    match outcome {
        Outcome::Done => {}
        Outcome::Failed(reason) => panic!("转换失败：{reason}"),
        Outcome::Cancelled => panic!("转换被意外取消"),
    }
}

fn read_head(path: &Path, n: usize) -> Vec<u8> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).expect("打开产物失败");
    let mut buffer = vec![0u8; n];
    let read = file.read(&mut buffer).unwrap_or(0);
    buffer.truncate(read);
    buffer
}

// ---------- 1. 同容器重封装 ----------

#[test]
fn mp4_to_mkv_with_stream_copy() {
    let dir = TempDir::new("copy");
    let source = make_video_with_audio(&dir, "in.mp4");
    let out = dir.join("out.mkv");

    let options = Options {
        copy_streams: true,
        ..Default::default()
    };
    let (path, outcome) = convert(&source, &out, "mkv", &options);
    assert_ok(&outcome);

    assert!(path.is_file());
    let probe = probe_of(&path);
    assert_eq!(probe.video().expect("应有视频轨").codec, "h264");
    assert_eq!(probe.audio().expect("应有音轨").codec, "aac");
    // 直拷不重编码，体积应当与源接近（mkv 的封装开销略大一点）。
    let source_size = std::fs::metadata(&source).unwrap().len();
    let out_size = std::fs::metadata(&path).unwrap().len();
    let ratio = out_size as f64 / source_size as f64;
    assert!(
        (0.85..1.25).contains(&ratio),
        "直拷体积比例异常：{ratio:.2}（源 {source_size} / 产物 {out_size}）"
    );
}

// ---------- 2. GIF 调色板链路 ----------

#[test]
fn mp4_to_gif_uses_palette_chain() {
    let dir = TempDir::new("gif");
    let source = make_video_with_audio(&dir, "in.mp4");
    let out = dir.join("out.gif");

    let (path, outcome) = convert(&source, &out, "gifv", &Options::default());
    assert_ok(&outcome);

    assert!(path.is_file());
    let bytes = read_head(&path, 6);
    assert_eq!(&bytes[..3], b"GIF", "产物不是 GIF");

    // 多帧：GIF 头之后应该有多个图像描述符。
    let probe = probe_of(&path);
    let video = probe.video().expect("GIF 应被识别出视频流");
    assert_eq!(video.codec, "gif");
    assert!(video.width > 0 && video.height > 0);
    // 2 秒 × 12fps 约 24 帧；至少应有 10 帧才算真的动了。
    assert!(
        video.frames.unwrap_or(0) >= 10,
        "GIF 帧数过少：{:?}",
        video.frames
    );
}

// ---------- 3. AVIF 单帧 ----------

#[test]
fn png_to_avif_single_frame() {
    let dir = TempDir::new("avif");
    let video = make_video_with_audio(&dir, "in.mp4");
    let source = make_source(
        &dir,
        "in.png",
        &[
            "-i",
            &video.to_string_lossy(),
            "-frames:v",
            "1",
            "-update",
            "1",
        ],
    );
    let out = dir.join("out.avif");

    let (path, outcome) = convert(&source, &out, "avif", &Options::default());
    assert_ok(&outcome);

    assert!(path.is_file());
    let size = std::fs::metadata(&path).unwrap().len();
    assert!(size > 100, "AVIF 体积异常：{size} 字节");

    // 容器魔数：第 4 字节起是 "ftyp"。
    let head = read_head(&path, 12);
    assert_eq!(&head[4..8], b"ftyp", "AVIF 应带 ftyp 盒");

    // 回读确认是单帧且尺寸正确。
    let probe = probe_of(&path);
    let v = probe.video().expect("AVIF 应有视频流");
    assert_eq!(v.codec, "av1");
    assert_eq!((v.width, v.height), (320, 240));
    assert_eq!(v.frames.unwrap_or(1), 1, "AVIF 应只有一帧");
}

// ---------- 4. 视频抽音频 ----------

#[test]
fn video_to_m4a_extracts_audio() {
    let dir = TempDir::new("m4a");
    let source = make_video_with_audio(&dir, "in.mp4");
    let out = dir.join("out.m4a");

    let (path, outcome) = convert(&source, &out, "m4a", &Options::default());
    assert_ok(&outcome);

    let probe = probe_of(&path);
    assert!(probe.video().is_none(), "音频产物不该含视频轨");
    let audio = probe.audio().expect("应有音轨");
    assert_eq!(audio.codec, "aac");
    let diff = (probe.duration_secs - 2.0).abs();
    assert!(diff < 0.25, "时长偏差过大：{:.3}s", probe.duration_secs);
}

// ---------- 5. 无音轨不能静默成功 ----------

#[test]
fn silent_video_to_audio_fails_loudly() {
    let dir = TempDir::new("silent");
    let source = make_silent_video(&dir, "silent.mp4");
    let out = dir.join("out.m4a");

    let target = by_id("m4a").unwrap();
    let probe = probe_of(&source);
    assert!(!probe.has_audio());

    // 参数构建阶段就该拒绝，而不是让 ffmpeg 写空文件。
    let result = args::build_plan(
        &source.to_string_lossy(),
        &out.to_string_lossy(),
        target,
        &Options::default(),
        &probe,
        &caps(),
    );
    assert!(result.is_err(), "无音轨的视频转音频应在构建阶段被拒");
    assert!(!out.exists(), "被拒绝时不应留下文件");
}

// ---------- 6. 质量影响体积 ----------

#[test]
fn lower_quality_produces_smaller_file() {
    let dir = TempDir::new("quality");
    let source = make_video_with_audio(&dir, "in.mp4");

    let low_out = dir.join("low.mp4");
    let high_out = dir.join("high.mp4");

    let low = Options {
        quality: 10,
        ..Default::default()
    };
    let high = Options {
        quality: 95,
        ..Default::default()
    };

    let (low_path, low_outcome) = convert(&source, &low_out, "mp4", &low);
    assert_ok(&low_outcome);
    let (high_path, high_outcome) = convert(&source, &high_out, "mp4", &high);
    assert_ok(&high_outcome);

    let low_size = std::fs::metadata(&low_path).unwrap().len();
    let high_size = std::fs::metadata(&high_path).unwrap().len();

    assert!(
        low_size < high_size,
        "质量 10（{low_size} 字节）应小于质量 95（{high_size} 字节）"
    );
}

// ---------- 7. 字幕互转 ----------

#[test]
fn subtitle_conversions_produce_valid_syntax() {
    let dir = TempDir::new("subs");
    let source = make_srt(&dir, "in.srt");

    // SRT → ASS
    let ass_out = dir.join("out.ass");
    let (ass_path, ass_outcome) = convert(&source, &ass_out, "ass", &Options::default());
    assert_ok(&ass_outcome);
    let ass = std::fs::read_to_string(&ass_path).expect("读 ass 失败");
    assert!(ass.contains("[Script Info]"), "ASS 应带 Script Info 段");
    assert!(ass.contains("[Events]"), "ASS 应带 Events 段");
    assert!(ass.contains("hello world"), "ASS 应保留字幕正文");

    // SRT → VTT
    let vtt_out = dir.join("out.vtt");
    let (vtt_path, vtt_outcome) = convert(&source, &vtt_out, "vtt", &Options::default());
    assert_ok(&vtt_outcome);
    let vtt = std::fs::read_to_string(&vtt_path).expect("读 vtt 失败");
    assert!(vtt.starts_with("WEBVTT"), "VTT 应以 WEBVTT 开头：{vtt:.40}");
    // WebVTT 用点号分隔毫秒。
    assert!(
        vtt.contains("00:00.500 --> 00:01.500"),
        "VTT 时间戳格式不对：{vtt}"
    );

    // SRT → LRC
    let lrc_out = dir.join("out.lrc");
    let (lrc_path, lrc_outcome) = convert(&source, &lrc_out, "lrc", &Options::default());
    assert_ok(&lrc_outcome);
    let lrc = std::fs::read_to_string(&lrc_path).expect("读 lrc 失败");
    assert!(
        lrc.contains("[00:00.50]hello world"),
        "LRC 时间戳格式不对：{lrc}"
    );
}

// ---------- 8. ICO 尺寸约束 ----------

#[test]
fn video_to_ico_respects_256_limit() {
    let dir = TempDir::new("ico");
    // 用 640×480 的源：超过 256，若没缩放 ico 复用器会写 0 字节。
    let source = make_source(
        &dir,
        "big.mp4",
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=640x480:rate=25:duration=1",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
    );
    let out = dir.join("out.ico");

    let (path, outcome) = convert(&source, &out, "ico", &Options::default());
    assert_ok(&outcome);

    let size = std::fs::metadata(&path).unwrap().len();
    assert!(size > 100, "ICO 体积异常：{size} 字节");

    // ICO 文件头：reserved(2)=0, type(2)=1, count(2)>=1。
    let head = read_head(&path, 6);
    assert_eq!(&head[0..2], &[0, 0], "ICO 保留字段应为 0");
    assert_eq!(&head[2..4], &[1, 0], "ICO 类型应为 1");
    let count = u16::from_le_bytes([head[4], head[5]]);
    assert!(count >= 1, "ICO 应至少包含一个图像");

    // 目录项里的宽高是单字节，0 表示 256。
    let entry = read_head(&path, 8 + 1);
    let width = entry[6];
    assert!(
        width <= 128,
        "缩略尺寸应在 256 以内换算后不超过 128，实际 {width}"
    );
}

// ---------- 9. DV ----------

#[test]
fn video_to_dv_uses_target_constraints() {
    let dir = TempDir::new("dv");
    let source = make_video_with_audio(&dir, "in.mp4");
    let out = dir.join("out.dv");

    let (path, outcome) = convert(&source, &out, "dv", &Options::default());
    assert_ok(&outcome);

    let probe = probe_of(&path);
    let video = probe.video().expect("DV 应有视频流");
    assert_eq!(video.codec, "dvvideo");
    assert_eq!((video.width, video.height), (720, 576), "应被约束为 PAL");

    let audio = probe.audio().expect("DV 应有音轨");
    assert_eq!(audio.codec, "pcm_s16le");
    assert_eq!(audio.sample_rate, Some(48000), "DV 音频必须是 48 kHz");
}

// ---------- 10. MXF ----------

#[test]
fn video_to_mxf_forces_48khz_pcm() {
    let dir = TempDir::new("mxf");
    let source = make_video_with_audio(&dir, "in.mp4");
    let out = dir.join("out.mxf");

    let (path, outcome) = convert(&source, &out, "mxf", &Options::default());
    assert_ok(&outcome);

    let probe = probe_of(&path);
    let video = probe.video().expect("MXF 应有视频流");
    assert_eq!(video.codec, "dnxhd");
    assert_eq!((video.width, video.height), (1920, 1080));

    let audio = probe.audio().expect("MXF 应有音轨");
    // 漏了 -ar 48000 会报 "mxf: only 48khz is implemented"。
    assert_eq!(audio.sample_rate, Some(48000), "MXF 音频必须是 48 kHz");
}

// ---------- 11. 取消并清理残片 ----------

#[test]
fn cancel_kills_process_and_removes_partial_file() {
    let dir = TempDir::new("cancel");
    // 用足够长的素材，保证取消发生在编码中段。
    let source = make_source(
        &dir,
        "long.mp4",
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=1280x720:rate=30:duration=12",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
        ],
    );
    let out = dir.join("out.mp4");

    let t = tools();
    let target = by_id("mp4").unwrap();
    let probe = probe_of(&source);
    let plan = args::build_plan(
        &source.to_string_lossy(),
        &out.to_string_lossy(),
        target,
        // 用最慢的编码器组合，保证取消时还在跑。
        &Options {
            quality: 100,
            ..Default::default()
        },
        &probe,
        &caps(),
    )
    .unwrap();

    let cancel = Arc::new(AtomicBool::new(false));

    // 300 ms 后请求取消。
    let flag = cancel.clone();
    let handle = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    });

    let outcome = runner::run_ffmpeg(
        &t.ffmpeg,
        &plan.args,
        &out,
        probe.duration_secs,
        &cancel,
        None,
    );
    handle.join().ok();

    match outcome {
        Outcome::Cancelled => {}
        Outcome::Done => panic!("12 秒素材在 300 ms 内完成，测试不可靠，请换更难的任务"),
        Outcome::Failed(reason) => panic!("取消路径不该报失败：{reason}"),
    }

    // 关键断言：残片必须被删掉，否则输出目录会留下打不开的空文件。
    assert!(
        !out.exists(),
        "取消后仍留下残片：{} 字节",
        std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0)
    );
}
