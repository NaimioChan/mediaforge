//! 全格式扫描：每种源类型 × 每个目标格式，真跑一遍。
//! 这条测试是「核心功能没有大问题」的证据，格式表或参数构建改动后必须跑。
//!
//! 不静默跳过：找不到 ffmpeg 直接 panic。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use mediaforge_lib::args::{self, Options};
use mediaforge_lib::formats::FORMATS;
use mediaforge_lib::paths::ToolPaths;
use mediaforge_lib::probe::Probe;
use mediaforge_lib::probe::parse as parse_probe;
use mediaforge_lib::runner::{self, Outcome};

fn tools() -> ToolPaths {
    ToolPaths::locate(None).unwrap_or_else(|| {
        panic!("找不到 ffmpeg。先运行 scripts/fetch-ffmpeg.ps1 把 9 个文件放到 vendor/ffmpeg/bin/")
    })
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir();
        let unique = format!(
            "mediaforge-sweep-{tag}-{}-{}",
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

fn make(dir: &TempDir, name: &str, args: &[&str]) -> PathBuf {
    let t = tools();
    let out = dir.join(name);
    let status = Command::new(&t.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .arg(&out)
        .status()
        .expect("启动 ffmpeg 失败");
    assert!(status.success(), "生成测试素材 {name} 失败");
    out
}

fn probe_of(path: &Path) -> Probe {
    let t = tools();
    let json = runner::run_ffprobe(&t.ffprobe, path).expect("ffprobe 失败");
    parse_probe(&json, &path.to_string_lossy()).expect("解析失败")
}

/// 扫一遍该类源的全部目标格式，返回失败明细。
fn sweep(dir: &TempDir, tag: &str, source: &Path, options: Options) -> Vec<String> {
    let t = tools();
    let caps = runner::probe_capabilities(&t.ffmpeg).expect("能力探测失败");
    let probe = probe_of(source);
    let kind = probe.kind();
    let mut failures = Vec::new();

    for f in FORMATS.iter().filter(|f| f.kind == kind) {
        let out = dir.join(&format!("{tag}-{}{}", f.id, f.ext));
        let _ = std::fs::remove_file(&out);

        let plan = match args::build_plan(
            &source.to_string_lossy(),
            &out.to_string_lossy(),
            f,
            &options,
            &probe,
            &caps,
        ) {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("{} 构建失败：{e}", f.id));
                continue;
            }
        };

        // 本程序直接写文本的格式跳过（另有测试覆盖）。
        if plan.direct_write.is_some() {
            continue;
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let outcome = runner::run_ffmpeg(
            &t.ffmpeg,
            &plan.args,
            &out,
            probe.duration_secs,
            &cancel,
            None,
        );

        let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        match outcome {
            Outcome::Done if size == 0 => failures.push(format!("{} 产出 0 字节", f.id)),
            Outcome::Done => {}
            Outcome::Failed(reason) => {
                let brief: String = reason.chars().take(160).collect();
                failures.push(format!("{} 失败：{brief}", f.id));
            }
            Outcome::Cancelled => failures.push(format!("{} 被意外取消", f.id)),
        }
    }

    failures
}

/// 断言某类源在给定参数下全部格式都能产出。
fn assert_all_ok(tag: &str, source: &Path, options: Options) {
    let dir = TempDir::new(tag);
    // 源文件要放进临时目录里，产物才好清理。
    let local = dir.join(
        source
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "src".into())
            .as_str(),
    );
    std::fs::copy(source, &local).expect("复制源文件失败");

    let failures = sweep(&dir, tag, &local, options.clone());
    assert!(
        failures.is_empty(),
        "[{tag}] 以下格式转换失败：\n{}",
        failures.join("\n")
    );
}

fn fixtures() -> TempDir {
    let dir = TempDir::new("src");
    // 素材在这里生成一次，由各测试复制到自己的工作目录。
    make(
        &dir,
        "video.mp4",
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=320x240:d=1:r=25",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ],
    );
    make(
        &dir,
        "audio.mp3",
        &[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-c:a",
            "libmp3lame",
            "-b:a",
            "192k",
        ],
    );
    make(
        &dir,
        "image.png",
        &["-f", "lavfi", "-i", "testsrc2=s=160x120", "-frames:v", "1"],
    );
    std::fs::write(
        dir.join("subtitle.srt"),
        "1\n00:00:00,200 --> 00:00:00,800\nhello\n\n2\n00:00:00,900 --> 00:00:01,500\nworld\n",
    )
    .unwrap();
    dir
}

// ---------- 重编码：每个格式都必须产出 ----------

#[test]
fn every_video_format_produces_output() {
    let fx = fixtures();
    assert_all_ok("video", &fx.join("video.mp4"), Options::default());
}

#[test]
fn every_audio_format_produces_output() {
    let fx = fixtures();
    assert_all_ok("audio", &fx.join("audio.mp3"), Options::default());
}

#[test]
fn every_image_format_produces_output() {
    let fx = fixtures();
    assert_all_ok("image", &fx.join("image.png"), Options::default());
}

#[test]
fn every_subtitle_format_produces_output() {
    let fx = fixtures();
    assert_all_ok("subtitle", &fx.join("subtitle.srt"), Options::default());
}

// ---------- 开到直拷：容器不收时要自动回退，不能报错 ----------

#[test]
fn video_formats_survive_copy_enabled() {
    let fx = fixtures();
    assert_all_ok(
        "video-copy",
        &fx.join("video.mp4"),
        Options {
            copy_streams: true,
            ..Default::default()
        },
    );
}

#[test]
fn audio_formats_survive_copy_enabled() {
    let fx = fixtures();
    // 这是用户实测报错的场景：mp3 开直拷转其它音频格式。
    assert_all_ok(
        "audio-copy",
        &fx.join("audio.mp3"),
        Options {
            copy_streams: true,
            ..Default::default()
        },
    );
}

#[test]
fn image_formats_survive_copy_enabled() {
    let fx = fixtures();
    assert_all_ok(
        "image-copy",
        &fx.join("image.png"),
        Options {
            copy_streams: true,
            ..Default::default()
        },
    );
}
