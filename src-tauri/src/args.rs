//! 把「请求 + 探测结果 + 工具能力」编译成 ffmpeg 参数表。
//!
//! 这是整个应用里最需要可测的部分，因此保持**纯函数**：
//! 不启动进程、不读写文件、不依赖环境变量。
//! 输出路径由调用方算好后传进来，本模块只负责拼参数。
//!
//! 有一条测试会读本文件源码，断言其中不出现 `std::process` 与 `std::fs`。

use crate::caps::Caps;
use crate::copy;
use crate::formats::{Format, Kind, Pipeline};
use crate::params::Tune;
use crate::probe::Probe;

/// 用户可调的转换参数。
///
/// 全局默认值来自设置文件，任务级的细调放在 `tune` 里。
/// 两者合成后才交给参数构建：设置给的是「新任务默认长什么样」，
/// `tune` 给的是「这一条任务想怎么调」。
#[derive(Debug, Clone)]
pub struct Options {
    /// 0–100，越大质量越高、体积越大。
    pub quality: u8,
    pub audio_bitrate_kbps: Option<u32>,
    /// 缩放上限，保持宽高比，不放大。
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    /// 允许直拷码流。真正的直拷还要过 `copy` 模块的兼容表：
    /// 容器不收这种码流时自动回退重编码，而不是让 ffmpeg 报错。
    pub copy_streams: bool,
    pub strip_metadata: bool,
    pub video_encoder_override: Option<String>,
    pub audio_encoder_override: Option<String>,
    pub gif_fps: u8,
    pub gif_max_width: u32,
    /// 任务级细调，覆盖上面的默认值。
    pub tune: Tune,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            quality: 80,
            audio_bitrate_kbps: Some(192),
            max_width: None,
            max_height: None,
            copy_streams: false,
            strip_metadata: false,
            video_encoder_override: None,
            audio_encoder_override: None,
            gif_fps: 12,
            gif_max_width: 480,
            tune: Tune::default(),
        }
    }
}

impl Options {
    fn clamped_quality(&self) -> u8 {
        self.tune.quality.unwrap_or(self.quality).min(100)
    }

    /// 图像质量与视频质量分开：同一套默认值下，
    /// 图片 80 分往往比视频 80 分更保守，因此单独取。
    fn clamped_image_quality(&self) -> u8 {
        self.tune
            .image_quality
            .or(self.tune.quality)
            .unwrap_or(self.quality)
            .min(100)
    }

    fn video_bitrate(&self) -> Option<u32> {
        self.tune.video_bitrate_kbps.filter(|b| *b > 0)
    }

    fn audio_bitrate(&self) -> u32 {
        self.tune
            .audio_bitrate_kbps
            .or(self.audio_bitrate_kbps)
            .unwrap_or(192)
            .clamp(32, 1536)
    }

    fn max_w(&self) -> u32 {
        self.tune.max_width.or(self.max_width).unwrap_or(0)
    }

    fn max_h(&self) -> u32 {
        self.tune.max_height.or(self.max_height).unwrap_or(0)
    }

    fn gif_fps_value(&self) -> u8 {
        self.tune.gif_fps.unwrap_or(self.gif_fps).clamp(1, 50)
    }

    fn gif_width_value(&self) -> u32 {
        self.tune
            .gif_width
            .unwrap_or(self.gif_max_width)
            .clamp(32, 1920)
    }
}

/// 需要本程序直接写文本的目标（ffmpeg 产不出的格式）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectWrite {
    /// `{起帧}{止帧}文本`，首行声明帧率。
    MicroDvd,
    /// `0:00:00.500,0:00:01.500` + 文本。
    Sbv,
}

/// 构建结果。
#[derive(Debug, Clone)]
pub struct Plan {
    pub output_path: String,
    /// 要执行的参数数组（不含 ffmpeg 自身路径与输出路径）。
    pub args: Vec<String>,
    pub video_encoder: Option<String>,
    pub audio_encoder: Option<String>,
    /// 需要告知用户的自动降级。
    pub notice: Option<String>,
    /// 非空表示不走 ffmpeg。
    pub direct_write: Option<DirectWrite>,
}

/// 主要入口。
pub fn build_plan(
    source_path: &str,
    output_path: &str,
    target: &Format,
    options: &Options,
    probe: &Probe,
    caps: &Caps,
) -> Result<Plan, String> {
    // 本程序直接写文本的格式，参数表为空。
    match target.pipeline {
        Pipeline::MicroDvdText => {
            return Ok(Plan {
                output_path: output_path.to_string(),
                args: Vec::new(),
                video_encoder: None,
                audio_encoder: None,
                notice: None,
                direct_write: Some(DirectWrite::MicroDvd),
            });
        }
        Pipeline::SbvText => {
            return Ok(Plan {
                output_path: output_path.to_string(),
                args: Vec::new(),
                video_encoder: None,
                audio_encoder: None,
                notice: None,
                direct_write: Some(DirectWrite::Sbv),
            });
        }
        _ => {}
    }

    let mut ctx = Ctx::new(source_path, target, options, probe, caps);

    match target.kind {
        Kind::Video => ctx.video()?,
        Kind::Audio => ctx.audio()?,
        Kind::Image => ctx.image()?,
        Kind::Subtitle => ctx.subtitle()?,
    }

    if options.strip_metadata {
        ctx.args.push("-map_metadata".into());
        ctx.args.push("-1".into());
    }

    if !target.muxer.is_empty() {
        ctx.args.push("-f".into());
        ctx.args.push(target.muxer.into());
    }

    let notice = ctx.notice();

    Ok(Plan {
        output_path: output_path.to_string(),
        args: ctx.args,
        video_encoder: ctx.video_encoder,
        audio_encoder: ctx.audio_encoder,
        notice,
        direct_write: None,
    })
}

/// 构建过程中的可变状态。带 `args` 与几项决议结果。
struct Ctx<'a> {
    args: Vec<String>,
    target: &'a Format,
    options: &'a Options,
    probe: &'a Probe,
    caps: &'a Caps,
    video_encoder: Option<String>,
    audio_encoder: Option<String>,
    notices: Vec<String>,
}

impl<'a> Ctx<'a> {
    fn new(
        source_path: &str,
        target: &'a Format,
        options: &'a Options,
        probe: &'a Probe,
        caps: &'a Caps,
    ) -> Self {
        Self {
            args: vec![
                "-hide_banner".into(),
                "-nostdin".into(),
                "-loglevel".into(),
                "error".into(),
                "-stats_period".into(),
                "0.4".into(),
                "-progress".into(),
                "pipe:1".into(),
                "-y".into(),
                "-i".into(),
                source_path.into(),
            ],
            target,
            options,
            probe,
            caps,
            video_encoder: None,
            audio_encoder: None,
            notices: Vec::new(),
        }
    }

    fn notice(&mut self) -> Option<String> {
        if self.notices.is_empty() {
            None
        } else {
            let mut seen: Vec<String> = Vec::new();
            for n in &self.notices {
                if !seen.contains(n) {
                    seen.push(n.clone());
                }
            }
            Some(seen.join("；"))
        }
    }

    fn push(&mut self, a: &str) {
        self.args.push(a.into());
    }

    fn push2(&mut self, a: &str, b: &str) {
        self.args.push(a.into());
        self.args.push(b.into());
    }

    // ---------- 直拷判定 ----------

    fn source_video_codec(&self) -> Option<&str> {
        self.probe.video().map(|v| v.codec.as_str())
    }

    fn source_audio_codec(&self) -> Option<&str> {
        self.probe.audio().map(|a| a.codec.as_str())
    }

    fn can_copy_video(&self) -> bool {
        let Some(codec) = self.source_video_codec() else {
            return false;
        };
        copy::video_copy_ok(self.target.muxer, codec)
    }

    fn can_copy_audio(&self) -> bool {
        let Some(codec) = self.source_audio_codec() else {
            return false;
        };
        copy::audio_copy_ok(self.target.muxer, codec)
    }

    fn can_copy_audio_in_video(&self) -> bool {
        let Some(codec) = self.source_audio_codec() else {
            return false;
        };
        copy::audio_in_video_copy_ok(self.target.muxer, codec, self.source_video_codec())
    }

    /// 编码预设：用户显式选了就用，否则按质量推算。
    fn preset_name(&self, encoder: &str) -> String {
        if let Some(preset) = self.options.tune.preset.as_deref() {
            // libsvtav1 的 preset 是数字 0–13，与 x264 的单词预设不通用。
            let numeric = matches!(encoder, "libsvtav1");
            let is_word = matches!(
                preset,
                "ultrafast"
                    | "superfast"
                    | "veryfast"
                    | "faster"
                    | "fast"
                    | "medium"
                    | "slow"
                    | "slower"
                    | "veryslow"
                    | "placebo"
            );
            let is_number = !preset.is_empty() && preset.chars().all(|c| c.is_ascii_digit());
            if (numeric && is_number) || (!numeric && is_word) {
                return preset.to_string();
            }
        }
        quality_preset(self.options.clamped_quality()).to_string()
    }

    fn push_preset(&mut self, encoder: &str) {
        let preset = self.preset_name(encoder);
        self.push2("-preset", &preset);
    }

    // ---------- 视频 ----------

    fn video(&mut self) -> Result<(), String> {
        // GIF 动图走独立的调色板管线，整条 mapping 交给 filter_complex。
        if self.target.pipeline == Pipeline::Gif {
            return self.gif();
        }

        if !self.probe.has_video() {
            return Err("源文件没有视频轨，无法输出视频".into());
        }

        // DV 与 MXF 对尺寸/采样率有强约束，交给专用管线。
        match self.target.pipeline {
            Pipeline::Dv => return self.dv(),
            Pipeline::Mxf => return self.mxf(),
            _ => {}
        }

        self.push2("-map", "0:v:0");

        let wants_audio =
            !self.target.audio_encoders.is_empty() || self.options.audio_encoder_override.is_some();

        // 直拷的前提是容器真收这种码流。兼容表不通过就回退重编码，
        // 并在任务上说明原因 —— 让 ffmpeg 报错是最差的处理方式。
        let copy_video = self.options.copy_streams && self.can_copy_video();
        let copy_audio = copy_video && wants_audio && self.can_copy_audio_in_video();

        if copy_video {
            self.push2("-c:v", "copy");
            self.video_encoder = Some("copy".into());
            if wants_audio {
                self.push2("-map", "0:a:0?");
                if copy_audio {
                    self.push2("-c:a", "copy");
                    self.audio_encoder = Some("copy".into());
                } else if self.probe.has_audio() {
                    let aenc = self.resolve_audio_encoder();
                    self.push2("-c:a", &aenc);
                    self.append_audio_quality(&aenc);
                    self.audio_encoder = Some(aenc);
                    self.notices.push(format!(
                        "{} 容器不接受 {} 音轨直拷，音频已重新编码",
                        self.target.name,
                        self.source_audio_codec().unwrap_or("源")
                    ));
                } else {
                    self.push("-an");
                }
            } else {
                self.push("-an");
            }
        } else {
            if self.options.copy_streams {
                self.notices.push(format!(
                    "{} 容器不接受 {} 码流直拷，已改为重新编码",
                    self.target.name,
                    self.source_video_codec().unwrap_or("源")
                ));
            }

            let encoder = self.resolve_video_encoder();
            self.push2("-c:v", &encoder);
            self.append_video_quality(&encoder);
            self.video_encoder = Some(encoder);

            if wants_audio {
                if self.probe.has_audio() || self.options.audio_encoder_override.is_some() {
                    // 问号表示「有则映射」。少了它，无音轨源会直接失败。
                    self.push2("-map", "0:a:0?");
                    let aenc = self.resolve_audio_encoder();
                    self.push2("-c:a", &aenc);
                    self.append_audio_quality(&aenc);
                    self.append_audio_format();
                    self.audio_encoder = Some(aenc);
                } else {
                    self.push("-an");
                }
            } else {
                self.push("-an");
            }
        }

        // 字幕只在支持的容器里保留，其余丢弃以免 mux 报错。
        if self.probe.has_subtitle() && self.muxer_keeps_subtitles() {
            self.push2("-map", "0:s:0?");
            let codec = if matches!(self.target.muxer, "mp4" | "mov") {
                "mov_text"
            } else {
                "copy"
            };
            self.push2("-c:s", codec);
        }

        self.append_scale();

        match self.target.muxer {
            "mp4" | "mov" => {
                self.push2("-movflags", "+faststart");
            }
            "webm"
                if self
                    .video_encoder
                    .as_deref()
                    .is_some_and(|e| e.starts_with("libvpx")) =>
            {
                self.push2("-row-mt", "1");
            }
            _ => {}
        }

        Ok(())
    }

    fn muxer_keeps_subtitles(&self) -> bool {
        matches!(
            self.target.muxer,
            "matroska" | "mp4" | "mov" | "mpegts" | "nut" | "webm" | "mxf" | "avi"
        )
    }

    /// 动图 GIF：先 palettegen 生成调色板，再 paletteuse 套用。
    /// 直接用默认 256 色调色板会得到锯齿与色带。
    fn gif(&mut self) -> Result<(), String> {
        if !self.probe.has_video() {
            return Err("源文件没有视频轨，无法输出 GIF".into());
        }

        let fps = self.options.gif_fps_value();
        let width = self.options.gif_width_value();
        let colors = self.options.tune.gif_colors.unwrap_or(256).clamp(2, 256);
        let dither = self
            .options
            .tune
            .gif_dither
            .as_deref()
            .filter(|d| matches!(*d, "bayer" | "sierra2_4a" | "none"))
            .unwrap_or("bayer");
        let loops = self.options.tune.gif_loop.unwrap_or(true);

        let paletteuse = if dither == "none" {
            "[b][p]paletteuse=dither=none".to_string()
        } else {
            let scale = if dither == "bayer" {
                ":bayer_scale=5"
            } else {
                ""
            };
            format!("[b][p]paletteuse=dither={dither}{scale}:diff_mode=rectangle")
        };

        self.push2(
            "-filter_complex",
            &format!(
                "[0:v]fps={fps},scale={width}:-1:flags=lanczos,split[a][b];\
                 [a]palettegen=max_colors={colors}:stats_mode=diff[p];\
                 {paletteuse}"
            ),
        );
        // -loop 0 表示无限循环；-loop -1 表示不循环。
        self.push2("-loop", if loops { "0" } else { "-1" });
        self.push("-an");
        self.video_encoder = Some("gif".into());
        self.notices
            .push(format!("GIF 输出 {fps}fps、宽 {width}px、{colors} 色"));

        Ok(())
    }

    /// DV：尺寸与采样率约束极严，手写 `scale=720:480,fps=30000/1001` 会失败，
    /// 用 `-target` 让 ffmpeg 自己约束。
    fn dv(&mut self) -> Result<(), String> {
        self.push2("-target", "pal-dv");
        self.push2("-map", "0:v:0");
        self.push2("-c:v", "dvvideo");
        self.push2("-pix_fmt", "yuv420p");
        self.video_encoder = Some("dvvideo".into());

        if self.probe.has_audio() {
            self.push2("-map", "0:a:0?");
            self.push2("-c:a", "pcm_s16le");
            self.push2("-ar", "48000");
            self.push2("-ac", "2");
            self.audio_encoder = Some("pcm_s16le".into());
        } else {
            self.push("-an");
        }

        self.notices
            .push("DV 固定为 PAL 720×576、48 kHz 立体声".into());
        Ok(())
    }

    /// MXF：复用器只认 48 kHz，编码器组合用 dnxhd 保证稳定。
    fn mxf(&mut self) -> Result<(), String> {
        let (want_w, want_h) = self
            .probe
            .video()
            .map(|v| (v.width, v.height))
            .unwrap_or((1920, 1080));
        let (w, h) = if want_w >= want_h {
            (1920, 1080)
        } else {
            (1080, 1920)
        };

        self.push2("-map", "0:v:0");
        self.push2("-vf", &format!("scale={w}:{h}"));
        self.push2("-c:v", "dnxhd");
        self.push2("-b:v", "120M");
        self.push2("-pix_fmt", "yuv422p");
        self.video_encoder = Some("dnxhd".into());

        if self.probe.has_audio() {
            self.push2("-map", "0:a:0?");
            self.push2("-c:a", "pcm_s24le");
            self.push2("-ar", "48000");
            self.push2("-ac", "2");
            self.audio_encoder = Some("pcm_s24le".into());
        } else {
            self.push("-an");
        }

        self.notices
            .push(format!("MXF 固定为 {w}×{h}、dnxhd 120M、48 kHz"));
        Ok(())
    }

    // ---------- 音频 ----------

    fn audio(&mut self) -> Result<(), String> {
        if !self.probe.has_audio() {
            return Err("源文件没有音轨，无法输出音频".into());
        }

        self.push("-vn");
        self.push("-sn");
        self.push("-dn");
        self.push2("-map", "0:a:0");

        if self.options.copy_streams {
            if self.can_copy_audio() {
                self.push2("-c:a", "copy");
                self.audio_encoder = Some("copy".into());
                return Ok(());
            }
            // 容器不收这种码流时静默回退会让人以为直拷生效了，
            // 所以退回去的同时说明一句。
            self.notices.push(format!(
                "{} 容器不接受 {} 码流直拷，已改为重新编码",
                self.target.name,
                self.source_audio_codec().unwrap_or("源")
            ));
        }

        let encoder = self.resolve_audio_encoder();
        self.push2("-c:a", &encoder);
        self.append_audio_quality(&encoder);
        self.append_audio_format();
        self.audio_encoder = Some(encoder);

        Ok(())
    }

    // ---------- 图像 ----------

    fn image(&mut self) -> Result<(), String> {
        if !self.probe.has_video() {
            return Err("源文件没有图像轨，无法输出图像".into());
        }

        if self.target.pipeline == Pipeline::Ico {
            return self.ico();
        }

        self.push2("-map", "0:v:0");
        let encoder = self.resolve_image_encoder();
        self.push2("-c:v", &encoder);
        self.append_image_quality(&encoder);

        let animated = self.target.animated;
        if animated {
            self.push2("-loop", "0");
            self.append_scale();
        } else {
            // 单帧输出必须加 -update 1，否则多帧会写成多个文件或直接失败。
            self.push2("-frames:v", "1");
            self.push2("-update", "1");
        }

        Ok(())
    }

    /// ICO：容器只接受不超过 256 的尺寸。缩放与像素格式都必须显式给出，
    /// 漏了任何一个都会得到 0 字节文件。
    fn ico(&mut self) -> Result<(), String> {
        self.push2("-map", "0:v:0");
        self.push2(
            "-vf",
            "scale=w=256:h=256:force_original_aspect_ratio=decrease,format=rgba",
        );
        self.push2("-c:v", "png");
        self.push2("-frames:v", "1");
        self.push2("-update", "1");
        self.video_encoder = Some("png".into());
        Ok(())
    }

    // ---------- 字幕 ----------

    fn subtitle(&mut self) -> Result<(), String> {
        if !self.probe.has_subtitle() {
            return Err("源文件不含字幕轨".into());
        }

        self.push("-an");
        self.push("-vn");
        self.push2("-map", "0:s:0");

        let codec = match self.target.muxer {
            "ass" => "ass",
            "webvtt" => "webvtt",
            "ttml" => "ttml",
            "lrc" => "text",
            _ => "subrip",
        };
        self.push2("-c:s", codec);

        // MicroDVD 用帧号计时，帧率决定时间换算，必须显式给出。
        if self.target.pipeline == Pipeline::MicroDvdText
            && let Some(fps) = self.options.tune.subtitle_fps
        {
            self.push2("-r", &format!("{fps}"));
        }

        Ok(())
    }

    // ---------- 编码器决议 ----------

    fn resolve_video_encoder(&mut self) -> String {
        // 任务级 override 优先，其次是设置里的全局 override。
        let wanted = self
            .options
            .tune
            .video_encoder
            .clone()
            .or_else(|| self.options.video_encoder_override.clone());

        if let Some(override_name) = wanted.as_deref() {
            if self.caps.has_encoder(override_name) {
                return override_name.to_string();
            }
            self.notices.push(format!(
                "编码器 {override_name} 在当前 ffmpeg 中不可用，已回退"
            ));
        }

        let candidates: &[&str] = if self.target.video_encoders.is_empty() {
            &["libx264"]
        } else {
            self.target.video_encoders
        };

        match self.caps.resolve_encoder(candidates) {
            Some(resolved) => {
                if resolved != candidates[0] {
                    self.notices
                        .push(format!("{} 不可用，改用 {resolved}", candidates[0]));
                }
                resolved
            }
            None => {
                // 能力清单可能不完整；按格式首选继续，真跑不动时由 ffmpeg 报错。
                let first = candidates[0].to_string();
                self.notices
                    .push(format!("视频编码器 {first} 不在能力清单中，仍按首选尝试"));
                first
            }
        }
    }

    fn resolve_audio_encoder(&mut self) -> String {
        let wanted = self
            .options
            .tune
            .audio_encoder
            .clone()
            .or_else(|| self.options.audio_encoder_override.clone());

        if let Some(override_name) = wanted.as_deref() {
            if self.caps.has_encoder(override_name) {
                return override_name.to_string();
            }
            self.notices
                .push(format!("音频编码器 {override_name} 不可用，已回退"));
        }

        let candidates: &[&str] = if self.target.audio_encoders.is_empty() {
            &["aac", "libmp3lame", "flac"]
        } else {
            self.target.audio_encoders
        };

        match self.caps.resolve_encoder(candidates) {
            Some(resolved) => {
                if resolved != candidates[0] {
                    self.notices
                        .push(format!("{} 不可用，改用 {resolved}", candidates[0]));
                }
                resolved
            }
            None => {
                let first = candidates[0].to_string();
                self.notices
                    .push(format!("音频编码器 {first} 不在能力清单中，仍按首选尝试"));
                first
            }
        }
    }

    fn resolve_image_encoder(&mut self) -> String {
        if let Some(name) = self.options.video_encoder_override.as_deref()
            && self.caps.has_encoder(name)
        {
            return name.to_string();
        }

        if !self.target.video_encoders.is_empty()
            && let Some(resolved) = self.caps.resolve_encoder(self.target.video_encoders)
        {
            return resolved;
        }

        // 没有显式声明时按扩展名推断。
        let ext = self.target.ext.trim_start_matches('.').to_ascii_lowercase();
        let guessed = match ext.as_str() {
            "png" => "png",
            "jpg" | "jpeg" => "mjpeg",
            "webp" => "libwebp",
            "bmp" => "bmp",
            "tiff" | "tif" => "tiff",
            "tga" => "targa",
            "gif" => "gif",
            "pcx" => "pcx",
            "qoi" => "qoi",
            "jp2" => "jpeg2000",
            "jls" => "jpegls",
            "pam" => "pam",
            "pgm" => "pgm",
            "ppm" => "ppm",
            "pbm" => "pbm",
            "pfm" => "pfm",
            "sgi" => "sgi",
            "ras" => "sunrast",
            "xbm" => "xbm",
            "wbmp" => "wbmp",
            "fits" => "fits",
            "hdr" => "hdr",
            "exr" => "exr",
            "dpx" => "dpx",
            "apng" => "apng",
            "jxl" => "libjxl",
            "avif" => "libsvtav1",
            "ico" => "png",
            _ => "png",
        };
        guessed.to_string()
    }

    // ---------- 质量参数 ----------

    fn append_video_quality(&mut self, encoder: &str) {
        let q = self.options.clamped_quality();

        // 给了目标码率就走码率模式：CRF 类参数与固定码率互斥，
        // 同时给会让 ffmpeg 用后者覆盖前者，结果不可预期。
        if let Some(kbps) = self.options.video_bitrate() {
            self.push2("-b:v", &format!("{kbps}k"));
            self.push2("-maxrate", &format!("{kbps}k"));
            self.push2("-bufsize", &format!("{}k", kbps.saturating_mul(2)));
            if encoder == "libx264" || encoder == "libx265" {
                self.push_preset(encoder);
                if encoder == "libx264" {
                    self.push2("-profile:v", "high");
                }
                if !self
                    .probe
                    .video()
                    .and_then(|v| v.pix_fmt.as_deref())
                    .is_some_and(|f| f.starts_with("yuv420p"))
                {
                    self.push2("-pix_fmt", "yuv420p");
                }
            }
            return;
        }

        match encoder {
            "libx264" | "libx265" => {
                let crf = map_crf(q, 51, 12);
                self.push2("-crf", &crf.to_string());
                self.push_preset(encoder);
                if encoder == "libx264" {
                    self.push2("-profile:v", "high");
                }
                if !self
                    .probe
                    .video()
                    .and_then(|v| v.pix_fmt.as_deref())
                    .is_some_and(|f| f.starts_with("yuv420p"))
                {
                    self.push2("-pix_fmt", "yuv420p");
                }
            }

            "libvpx-vp9" => {
                let crf = map_crf(q, 63, 18);
                self.push2("-crf", &crf.to_string());
                self.push2("-b:v", "0");
                self.push2("-deadline", if q >= 80 { "good" } else { "realtime" });
                self.push2("-cpu-used", if q >= 80 { "1" } else { "4" });
            }

            "libvpx" => {
                let crf = map_crf(q, 63, 8);
                self.push2("-crf", &crf.to_string());
                self.push2("-b:v", "0");
                self.push2("-cpu-used", "2");
            }

            // libsvtav1 没有 still_picture 参数，别照抄 libaom 的写法。
            "libsvtav1" | "libaom-av1" => {
                let crf = map_crf(q, 63, 20);
                self.push2("-crf", &crf.to_string());
                if encoder == "libsvtav1" {
                    let preset = (13 - (u32::from(q) * 13 / 100)).clamp(1, 13);
                    self.push2("-preset", &preset.to_string());
                } else {
                    self.push2("-preset", "6");
                }
            }

            "h264_nvenc" | "hevc_nvenc" | "av1_nvenc" => {
                self.push2("-rc", "constqp");
                let qp = map_crf(q, 51, 12);
                self.push2("-qp", &qp.to_string());
            }

            "h264_qsv" | "hevc_qsv" => {
                let qp = map_crf(q, 51, 12);
                self.push2("-global_quality", &qp.to_string());
            }

            "h264_amf" | "hevc_amf" | "av1_amf" => {
                self.push2("-rc", "cqp");
                let qp = map_crf(q, 51, 12).to_string();
                self.push2("-qp_i", &qp);
                self.push2("-qp_p", &qp);
            }

            "mpeg4" | "libxvid" | "mpeg2video" | "mpeg1video" | "wmv2" | "flv1" | "h263"
            | "mjpeg" => {
                let qv = map_crf(q, 31, 2);
                self.push2("-q:v", &qv.to_string());
                if matches!(
                    encoder,
                    "mpeg4" | "libxvid" | "wmv2" | "flv1" | "h263" | "mpeg2video" | "mpeg1video"
                ) {
                    self.push2("-pix_fmt", "yuv420p");
                }
            }

            "libtheora" => {
                let qv = (u32::from(q) / 10).min(10);
                self.push2("-q:v", &qv.to_string());
            }

            "prores" => {
                self.push2("-profile:v", if q >= 80 { "4444" } else { "hq" });
                self.push2("-vendor", "apl0");
            }

            "dnxhd" => {
                self.push2("-b:v", "120M");
                self.push2("-pix_fmt", "yuv422p");
            }

            "dvvideo" => {
                self.push2("-pix_fmt", "yuv420p");
            }

            // ffv1 / utvideo / huffyuv / rawvideo 等无损编码器不需要质量参数。
            _ => {}
        }
    }

    fn append_image_quality(&mut self, encoder: &str) {
        let q = self.options.clamped_image_quality();

        match encoder {
            "mjpeg" => {
                let qv = map_crf(q, 31, 2);
                self.push2("-q:v", &qv.to_string());
                self.push2("-pix_fmt", "yuvj420p");
            }

            "libwebp" | "libwebp_anim" => {
                self.push2("-quality", &q.to_string());
                self.push2("-compression_level", "4");
            }

            "libjxl" => {
                // libjxl 用 distance：0 为无损，数字越大损失越多。
                let distance = if q >= 99 {
                    "0".to_string()
                } else {
                    format!("{}", 15 - (u32::from(q) * 15 / 100))
                };
                self.push2("-distance", &distance);
                self.push2("-effort", "7");
            }

            // libsvtav1 没有 still_picture，单帧由 -frames:v 1 保证。
            "libsvtav1" => {
                let crf = map_crf(q, 63, 18);
                self.push2("-crf", &crf.to_string());
                self.push2("-svtav1-params", "tune=0");
            }

            "libaom-av1" => {
                let crf = map_crf(q, 63, 18);
                self.push2("-crf", &crf.to_string());
                self.push2("-still-picture", "1");
                self.push2("-cpu-used", "8");
            }

            "tiff" => {
                self.push2("-compression_algo", "deflate");
            }

            // PNG 想保留 alpha 时不要强转 yuv。
            "png" | "apng" if self.probe.has_alpha() => {
                self.push2("-pix_fmt", "rgba");
            }

            _ => {}
        }
    }

    fn append_audio_quality(&mut self, encoder: &str) {
        let bitrate = self.effective_audio_bitrate(encoder);

        match encoder {
            "flac" => {
                // 压缩级别可调，默认 8 是体积与耗时的折中。
                let level = self.options.tune.audio_quality.unwrap_or(8).min(12);
                self.push2("-compression_level", &level.to_string());
            }

            // WavPack 编码器要求帧对齐：源是 mp3 这类帧长非 4096 的流时，
            // 直接喂会得到 `invalid block size`，必须显式重排采样数。
            "wavpack" => {
                self.push2("-af", "asetnsamples=n=4096");
            }

            "libmp3lame" => {
                self.push2("-b:a", &format!("{bitrate}k"));
            }

            "libvorbis" => {
                // Vorbis 是 VBR：优先用 -q:a，没有质量设置时按码率反推。
                let qa = self
                    .options
                    .tune
                    .audio_quality
                    .map(|q| u32::from(q).min(10))
                    .unwrap_or((bitrate / 32).min(10));
                self.push2("-q:a", &qa.to_string());
            }

            // DTS 编码器在 ffmpeg 里标记为实验性，必须 -strict -2 才允许使用。
            "dca" => {
                self.push2("-ar", "48000");
                self.push2("-ac", "2");
                self.push2("-b:a", "768k");
                self.push2("-strict", "-2");
            }

            // 无损编码器不设码率。
            "alac" | "pcm_s16le" | "pcm_s24le" | "pcm_s32le" | "pcm_f32le" | "pcm_s16be"
            | "pcm_s24be" | "pcm_alaw" | "pcm_mulaw" | "tta" | "truehd" | "mlp" | "copy" => {}

            _ => {
                self.push2("-b:a", &format!("{bitrate}k"));
            }
        }
    }

    /// 采样率与声道：用户没指定就不动，保持源参数最安全。
    fn append_audio_format(&mut self) {
        if let Some(rate) = self.options.tune.audio_sample_rate.filter(|r| *r >= 8000) {
            self.push2("-ar", &rate.to_string());
        }
        if let Some(channels) = self.options.tune.audio_channels.filter(|c| *c >= 1) {
            self.push2("-ac", &channels.to_string());
        }
    }

    /// 实际生效的音频码率。
    ///
    /// 每个有损编码器都有自己的取值范围，超了 ffmpeg 直接拒绝打开编码器，
    /// 用户看到的是「无法转换」。libopus 与 libvorbis 的上限还随声道数变化：
    /// 单声道 libopus 最多 256k，立体声能到 512k。这里按目标声道数夹紧，
    /// 并把夹紧的事实写进 notice，避免用户以为设置生效了。
    fn effective_audio_bitrate(&mut self, encoder: &str) -> u32 {
        let wanted = self.options.audio_bitrate();
        // 目标声道数：用户显式指定优先，否则用源的。
        let channels = self
            .options
            .tune
            .audio_channels
            .or_else(|| self.probe.audio().and_then(|a| a.channels))
            .unwrap_or(2)
            .max(1);

        let ceiling = match encoder {
            "libopus" => {
                // 实测：单声道 256k、立体声 512k 是硬上限。
                if channels <= 1 { 256 } else { 512 }
            }
            "libvorbis" => {
                // 实测：单声道 192k、立体声 384k。
                if channels <= 1 { 192 } else { 384 }
            }
            "mp2" => 384,
            _ => 1536,
        };

        let floor = match encoder {
            "libopus" => 6,
            "libvorbis" => 32,
            _ => 32,
        };

        let actual = wanted.clamp(floor, ceiling);
        if actual != wanted {
            self.notices.push(format!(
                "{encoder} 的码率上限是 {ceiling}k（{channels} 声道），已从 {wanted}k 调整"
            ));
        }
        actual
    }

    /// 缩放与帧率：两者都是滤镜，必须合成一条 `-vf`，否则后一条会覆盖前一条。
    fn append_scale(&mut self) {
        let max_w = self.options.max_w();
        let max_h = self.options.max_h();
        let fps = self.options.tune.fps;

        let mut filters: Vec<String> = Vec::new();

        if max_w > 0 || max_h > 0 {
            // 用 force_original_aspect_ratio 限制长边，避免小图被放大。
            let expr = match (max_w > 0, max_h > 0) {
                (true, true) => format!(
                    "scale='min({max_w},iw)':'min({max_h},ih)':force_original_aspect_ratio=decrease"
                ),
                (true, false) => format!("scale='min({max_w},iw)':-2"),
                (false, true) => format!("scale=-2:'min({max_h},ih)'"),
                (false, false) => String::new(),
            };
            if !expr.is_empty() {
                filters.push(format!("{expr}:flags=lanczos"));
            }
        }

        if let Some(fps) = fps.filter(|f| *f > 0.0 && *f <= 240.0) {
            // 保留三位小数，避免 29.97 被写成 29.97 之外的奇怪值。
            let text = format!("{fps:.3}");
            let text = text.trim_end_matches('0').trim_end_matches('.').to_string();
            filters.push(format!("fps={text}"));
        }

        if !filters.is_empty() {
            self.push2("-vf", &filters.join(","));
        }
    }
}

/// 把 0–100 质量映射到编码器原生范围（数值越小越好）。
pub fn map_crf(quality: u8, worst: u8, best: u8) -> u8 {
    let q = quality.min(100) as f64;
    let w = f64::from(worst);
    let b = f64::from(best);
    (w - (w - b) * q / 100.0).round() as u8
}

fn quality_preset(quality: u8) -> &'static str {
    match quality {
        90..=100 => "slow",
        70..=89 => "medium",
        40..=69 => "fast",
        _ => "veryfast",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::by_id;
    use crate::probe::parse as parse_probe;

    const VIDEO_JSON: &str = include_str!("../tests/fixtures/probe_video.json");
    const AUDIO_JSON: &str = include_str!("../tests/fixtures/probe_audio.json");
    const MP3_JSON: &str = include_str!("../tests/fixtures/probe_mp3.json");
    const SUB_JSON: &str = include_str!("../tests/fixtures/probe_subtitle.json");
    const IMAGE_JSON: &str = include_str!("../tests/fixtures/probe_image.json");
    const SILENT_JSON: &str = include_str!("../tests/fixtures/probe_video_silent.json");

    fn caps() -> Caps {
        Caps::from_names(
            [
                "libx264",
                "libx265",
                "libsvtav1",
                "libaom-av1",
                "libvpx-vp9",
                "libvpx",
                "libtheora",
                "libmp3lame",
                "aac",
                "libopus",
                "libvorbis",
                "flac",
                "alac",
                "wmav2",
                "ac3",
                "eac3",
                "dca",
                "tta",
                "wavpack",
                "libwebp",
                "libwebp_anim",
                "libjxl",
                "apng",
                "gif",
                "tiff",
                "prores",
                "dnxhd",
                "dvvideo",
                "ffv1",
                "utvideo",
                "huffyuv",
                "png",
                "mjpeg",
                "jpeg2000",
                "jpegls",
                "qoi",
                "targa",
                "sgi",
                "sunrast",
                "xbm",
                "wbmp",
                "fits",
                "exr",
                "hdr",
                "dpx",
                "pam",
                "pgm",
                "ppm",
                "pbm",
                "pfm",
                "pcx",
                "bmp",
                "subrip",
                "ass",
                "webvtt",
                "ttml",
                "text",
                "mov_text",
                "mpeg4",
                "libxvid",
                "mpeg2video",
                "mpeg1video",
                "wmv2",
                "h263",
                "mp2",
                "pcm_s16le",
                "pcm_s24le",
                "pcm_s16be",
            ]
            .iter()
            .map(|s| s.to_string()),
            [
                "mp4", "matroska", "webm", "mov", "avi", "asf", "flv", "mpegts", "mpeg", "vob",
                "3gp", "ogg", "nut", "ivf", "mxf", "dv", "gif", "h264", "hevc", "mp3", "ipod",
                "adts", "flac", "wav", "aiff", "opus", "ac3", "eac3", "dts", "tta", "wv", "image2",
                "webp", "avif", "apng", "ico", "srt", "ass", "webvtt", "ttml", "lrc",
            ]
            .iter()
            .map(|s| s.to_string()),
        )
    }

    fn probe_of(json: &str) -> Probe {
        parse_probe(json, "C:/media/source.mp4").unwrap()
    }

    fn build(id: &str, opts: Options, probe_json: &str) -> Plan {
        let target = by_id(id).unwrap_or_else(|| panic!("格式 {id} 不在格式表里"));
        let out = format!("C:/out/result{}", target.ext);
        build_plan(
            "C:/media/source.mp4",
            &out,
            target,
            &opts,
            &probe_of(probe_json),
            &caps(),
        )
        .unwrap_or_else(|e| panic!("{id} 构建失败：{e}"))
    }

    fn joined(plan: &Plan) -> String {
        plan.args.join(" ")
    }

    // ---------- 基础 ----------

    #[test]
    fn every_plan_starts_with_quiet_progress_flags() {
        let plan = build("mp4", Options::default(), VIDEO_JSON);
        assert_eq!(plan.args[0], "-hide_banner");
        assert_eq!(plan.args[1], "-nostdin");
        let text = joined(&plan);
        assert!(
            text.contains("-progress pipe:1"),
            "必须用 -progress 做进度：{text}"
        );
        assert!(text.contains("-stats_period 0.4"));
        assert!(text.contains("-y"));
    }

    #[test]
    fn output_path_is_passed_through_untouched() {
        let plan = build("mp4", Options::default(), VIDEO_JSON);
        assert_eq!(plan.output_path, "C:/out/result.mp4");
        // 输出路径不进 args，由 runner 追加，避免纯函数里出现路径处理逻辑。
        assert!(!plan.args.iter().any(|a| a.contains("result.mp4")));
    }

    // ---------- 视频 ----------

    #[test]
    fn mp4_maps_first_video_and_optional_audio() {
        let plan = build("mp4", Options::default(), VIDEO_JSON);
        let text = joined(&plan);
        assert!(text.contains("-map 0:v:0"), "{text}");
        // 问号是「有则映射」，漏了会让无音轨源直接失败。
        assert!(text.contains("-map 0:a:0?"), "{text}");
        assert!(text.contains("-c:v libx264"), "{text}");
        assert!(text.contains("-c:a aac"), "{text}");
        assert!(text.contains("-movflags +faststart"), "{text}");
        assert!(text.contains("-f mp4"), "{text}");
        assert_eq!(plan.video_encoder.as_deref(), Some("libx264"));
    }

    #[test]
    fn quality_changes_crf_monotonically() {
        let low = build(
            "mp4",
            Options {
                quality: 20,
                ..Default::default()
            },
            VIDEO_JSON,
        );
        let high = build(
            "mp4",
            Options {
                quality: 95,
                ..Default::default()
            },
            VIDEO_JSON,
        );
        let crf_of = |p: &Plan| -> u32 {
            let i = p.args.iter().position(|a| a == "-crf").unwrap();
            p.args[i + 1].parse().unwrap()
        };
        assert!(
            crf_of(&high) < crf_of(&low),
            "质量高应得到更小的 crf：{} vs {}",
            crf_of(&high),
            crf_of(&low)
        );
    }

    #[test]
    fn stream_copy_skips_encoder_and_quality() {
        let plan = build(
            "mkv",
            Options {
                copy_streams: true,
                ..Default::default()
            },
            VIDEO_JSON,
        );
        let text = joined(&plan);
        assert!(text.contains("-c:v copy"), "{text}");
        assert!(text.contains("-c:a copy"), "{text}");
        assert!(!text.contains("-crf"), "直拷不应带质量参数：{text}");
        assert!(!text.contains("-preset"), "{text}");
    }

    #[test]
    fn audio_bitrate_is_applied() {
        let plan = build(
            "mp4",
            Options {
                audio_bitrate_kbps: Some(320),
                ..Default::default()
            },
            VIDEO_JSON,
        );
        assert!(joined(&plan).contains("-b:a 320k"), "{}", joined(&plan));
    }

    // ---------- 直拷回退 ----------

    #[test]
    fn mp3_to_flac_falls_back_to_reencode() {
        // 这是用户实际遇到的故障：开了直拷后 mp3 转 flac 全部失败，
        // 因为 flac 容器不收 mp3 码流。必须自动回退并说明原因。
        let plan = build(
            "flac",
            Options {
                copy_streams: true,
                ..Default::default()
            },
            MP3_JSON,
        );
        let text = joined(&plan);
        assert!(!text.contains("-c:a copy"), "flac 不该直拷 mp3：{text}");
        assert!(text.contains("-c:a flac"), "{text}");
        let notice = plan.notice.as_deref().unwrap_or("");
        assert!(notice.contains("直拷"), "应提示回退原因：{notice}");
    }

    #[test]
    fn mp3_to_mp3_still_copies() {
        // 容器收这种码流时，直拷应当照常生效。
        let plan = build(
            "mp3",
            Options {
                copy_streams: true,
                ..Default::default()
            },
            MP3_JSON,
        );
        assert!(joined(&plan).contains("-c:a copy"), "{}", joined(&plan));
    }

    #[test]
    fn mp3_to_wav_and_wma_still_copy() {
        for id in ["wav", "wma"] {
            let plan = build(
                id,
                Options {
                    copy_streams: true,
                    ..Default::default()
                },
                MP3_JSON,
            );
            assert!(
                joined(&plan).contains("-c:a copy"),
                "{id} 应能直拷 mp3：{}",
                joined(&plan)
            );
        }
    }

    #[test]
    fn every_audio_target_works_with_copy_enabled() {
        // 直拷开关打开时，所有音频目标都必须产出可执行参数表，
        // 而不是一条注定失败的 -c copy。
        for f in crate::formats::FORMATS
            .iter()
            .filter(|f| f.kind == Kind::Audio)
        {
            let plan = build(
                f.id,
                Options {
                    copy_streams: true,
                    ..Default::default()
                },
                MP3_JSON,
            );
            let text = joined(&plan);
            let copies = text.contains("-c:a copy");
            let encodes = text.contains("-c:a ");
            assert!(encodes, "{} 缺少 -c:a：{text}", f.id);
            // 直拷只允许在容器真支持时出现。
            if copies {
                assert!(
                    crate::copy::audio_copy_ok(f.muxer, "mp3"),
                    "{} 不该直拷 mp3：{text}",
                    f.id
                );
            }
        }
    }

    #[test]
    fn gif_never_copies_stream() {
        let plan = build(
            "gifv",
            Options {
                copy_streams: true,
                ..Default::default()
            },
            VIDEO_JSON,
        );
        let text = joined(&plan);
        assert!(!text.contains("-c:v copy"), "GIF 必须重编码：{text}");
        assert!(text.contains("palettegen"), "{text}");
    }

    // ---------- 细调参数 ----------

    #[test]
    fn tune_video_bitrate_switches_to_bitrate_mode() {
        let plan = build(
            "mp4",
            Options {
                tune: Tune {
                    video_bitrate_kbps: Some(4000),
                    ..Default::default()
                },
                ..Default::default()
            },
            VIDEO_JSON,
        );
        let text = joined(&plan);
        assert!(text.contains("-b:v 4000k"), "{text}");
        assert!(!text.contains("-crf"), "码率模式下不该再给 crf：{text}");
    }

    #[test]
    fn tune_preset_overrides_quality_derived_preset() {
        let plan = build(
            "mp4",
            Options {
                tune: Tune {
                    preset: Some("veryslow".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
            VIDEO_JSON,
        );
        assert!(
            joined(&plan).contains("-preset veryslow"),
            "{}",
            joined(&plan)
        );
    }

    #[test]
    fn tune_max_width_and_fps_share_one_filter_chain() {
        let plan = build(
            "mp4",
            Options {
                tune: Tune {
                    max_width: Some(640),
                    fps: Some(30.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            VIDEO_JSON,
        );
        let text = joined(&plan);
        // 两条 -vf 会互相覆盖，必须合成一条。
        assert_eq!(text.matches("-vf").count(), 1, "{text}");
        assert!(text.contains("scale="), "{text}");
        assert!(text.contains("fps=30"), "{text}");
    }

    #[test]
    fn tune_audio_sample_rate_and_channels_are_applied() {
        let plan = build(
            "flac",
            Options {
                tune: Tune {
                    audio_sample_rate: Some(48000),
                    audio_channels: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            },
            AUDIO_JSON,
        );
        let text = joined(&plan);
        assert!(text.contains("-ar 48000"), "{text}");
        assert!(text.contains("-ac 1"), "{text}");
    }

    #[test]
    fn tune_flac_compression_level_is_honoured() {
        let plan = build(
            "flac",
            Options {
                tune: Tune {
                    audio_quality: Some(12),
                    ..Default::default()
                },
                ..Default::default()
            },
            AUDIO_JSON,
        );
        assert!(
            joined(&plan).contains("-compression_level 12"),
            "{}",
            joined(&plan)
        );
    }

    #[test]
    fn wavpack_always_gets_frame_alignment() {
        // 不重排采样数时 wavpack 会报 invalid block size 并产出 0 字节。
        let plan = build("wavpack", Options::default(), AUDIO_JSON);
        assert!(
            joined(&plan).contains("asetnsamples=n=4096"),
            "{}",
            joined(&plan)
        );
    }

    #[test]
    fn tune_encoder_override_beats_format_default() {
        let plan = build(
            "mp4",
            Options {
                tune: Tune {
                    video_encoder: Some("libx265".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
            VIDEO_JSON,
        );
        assert!(joined(&plan).contains("-c:v libx265"), "{}", joined(&plan));
    }

    #[test]
    fn gif_tune_controls_palette() {
        let plan = build(
            "gifv",
            Options {
                tune: Tune {
                    gif_colors: Some(64),
                    gif_dither: Some("none".into()),
                    gif_loop: Some(false),
                    ..Default::default()
                },
                ..Default::default()
            },
            VIDEO_JSON,
        );
        let text = joined(&plan);
        assert!(text.contains("max_colors=64"), "{text}");
        assert!(text.contains("dither=none"), "{text}");
        assert!(text.contains("-loop -1"), "不循环应为 -1：{text}");
    }

    // ---------- 音频码率上限 ----------

    #[test]
    fn opus_bitrate_is_capped_for_mono() {
        // 实测：单声道 libopus 最多 256k。超出时 ffmpeg 拒绝打开编码器，
        // 用户看到的是「无法转换」。必须夹紧并说明。
        let plan = build(
            "opus",
            Options {
                audio_bitrate_kbps: Some(320),
                ..Default::default()
            },
            MP3_JSON,
        );
        let text = joined(&plan);
        assert!(text.contains("-b:a 256k"), "单声道应夹到 256k：{text}");
        assert!(
            plan.notice.as_deref().unwrap_or("").contains("256k"),
            "应提示码率被调整：{:?}",
            plan.notice
        );
    }

    #[test]
    fn opus_bitrate_allows_more_for_stereo() {
        let plan = build(
            "opus",
            Options {
                audio_bitrate_kbps: Some(320),
                tune: Tune {
                    audio_channels: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            },
            MP3_JSON,
        );
        assert!(
            joined(&plan).contains("-b:a 320k"),
            "立体声能到 512k，320k 不该被改：{}",
            joined(&plan)
        );
    }

    #[test]
    fn vorbis_bitrate_is_capped_too() {
        // vorbis 走 -q:a，码率只用于反推质量等级；但码率本身也要夹，
        // 否则反过来算出的质量等级会超出 0-10。
        let plan = build(
            "ogg",
            Options {
                audio_bitrate_kbps: Some(512),
                ..Default::default()
            },
            MP3_JSON,
        );
        let text = joined(&plan);
        let qa: u32 = text
            .split("-q:a ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse().ok())
            .expect("应有 -q:a");
        assert!(qa <= 10, "vorbis 质量等级必须在 0-10：{qa}");
    }

    #[test]
    fn mp3_accepts_high_bitrates_without_clamping() {
        let plan = build(
            "mp3",
            Options {
                audio_bitrate_kbps: Some(320),
                ..Default::default()
            },
            MP3_JSON,
        );
        assert!(joined(&plan).contains("-b:a 320k"), "{}", joined(&plan));
        assert!(plan.notice.is_none(), "不该有夹紧提示：{:?}", plan.notice);
    }

    #[test]
    fn resolution_cap_uses_scale_not_upscale() {
        let plan = build(
            "mp4",
            Options {
                max_height: Some(720),
                ..Default::default()
            },
            VIDEO_JSON,
        );
        let text = joined(&plan);
        assert!(text.contains("scale=-2:'min(720,ih)'"), "{text}");
        assert!(text.contains("flags=lanczos"), "{text}");
    }

    #[test]
    fn strip_metadata_adds_map_metadata_minus_one() {
        let plan = build(
            "mp4",
            Options {
                strip_metadata: true,
                ..Default::default()
            },
            VIDEO_JSON,
        );
        assert!(joined(&plan).contains("-map_metadata -1"));
    }

    #[test]
    fn silent_source_does_not_ask_for_audio_track() {
        // 无音轨时不能出现 -c:a，否则 ffmpeg 会因为找不到流而失败。
        let plan = build("mp4", Options::default(), SILENT_JSON);
        let text = joined(&plan);
        assert!(text.contains("-an"), "{text}");
        assert!(!text.contains("-c:a"), "{text}");
    }

    #[test]
    fn gif_uses_palette_filter_chain() {
        let plan = build("gifv", Options::default(), VIDEO_JSON);
        let text = joined(&plan);
        assert!(text.contains("palettegen"), "{text}");
        assert!(text.contains("paletteuse"), "{text}");
        assert!(text.contains("split[a][b]"), "{text}");
        assert!(text.contains("-loop 0"), "{text}");
    }

    #[test]
    fn dv_uses_target_flag_instead_of_manual_scale() {
        let plan = build("dv", Options::default(), VIDEO_JSON);
        let text = joined(&plan);
        assert!(text.contains("-target pal-dv"), "{text}");
        assert!(!text.contains("-vf"), "DV 不应手写 scale：{text}");
        assert!(text.contains("-ar 48000"), "{text}");
        assert!(text.contains("-ac 2"), "{text}");
    }

    #[test]
    fn mxf_forces_48khz_pcm_and_square_pixels() {
        let plan = build("mxf", Options::default(), VIDEO_JSON);
        let text = joined(&plan);
        // 复用器只认 48 kHz，漏了会报 "only 48khz is implemented"。
        assert!(text.contains("-ar 48000"), "{text}");
        assert!(text.contains("-c:a pcm_s24le"), "{text}");
        assert!(text.contains("-c:v dnxhd"), "{text}");
        assert!(text.contains("-pix_fmt yuv422p"), "{text}");
        assert!(text.contains("scale=1920:1080"), "{text}");
    }

    // ---------- 音频 ----------

    #[test]
    fn audio_target_drops_video_streams() {
        let plan = build("mp3", Options::default(), VIDEO_JSON);
        let text = joined(&plan);
        assert!(text.contains("-vn"), "{text}");
        assert!(text.contains("-sn"), "{text}");
        assert!(text.contains("-dn"), "{text}");
        assert!(text.contains("-map 0:a:0"), "{text}");
        assert!(text.contains("-c:a libmp3lame"), "{text}");
    }

    #[test]
    fn dts_needs_strict_experimental_flag() {
        let plan = build("dts", Options::default(), VIDEO_JSON);
        let text = joined(&plan);
        // dca 在 ffmpeg 里是实验性编码器，不加 -strict -2 会被拒绝。
        assert!(text.contains("-strict -2"), "{text}");
        assert!(text.contains("-ar 48000"), "{text}");
        assert!(text.contains("-ac 2"), "{text}");
    }

    #[test]
    fn lossless_audio_gets_no_bitrate() {
        for id in ["flac", "wav", "alac", "tta", "wavpack"] {
            let plan = build(id, Options::default(), AUDIO_JSON);
            let text = joined(&plan);
            assert!(
                !text.contains("-b:a"),
                "{id} 是无损格式，不该带 -b:a：{text}"
            );
        }
    }

    #[test]
    fn audio_source_missing_track_is_rejected() {
        let target = by_id("mp3").unwrap();
        let err = build_plan(
            "C:/media/source.mp4",
            "C:/out/x.mp3",
            target,
            &Options::default(),
            &probe_of(SUB_JSON),
            &caps(),
        )
        .unwrap_err();
        assert!(err.contains("音轨"), "错误信息应说明缺音轨：{err}");
    }

    // ---------- 图像 ----------

    #[test]
    fn single_frame_image_outputs_one_frame_with_update() {
        let plan = build("png", Options::default(), VIDEO_JSON);
        let text = joined(&plan);
        // 少了 -update 1，image2 复用器会写成序列或多帧。
        assert!(text.contains("-frames:v 1"), "{text}");
        assert!(text.contains("-update 1"), "{text}");
    }

    #[test]
    fn ico_scales_into_256_and_forces_rgba() {
        let plan = build("ico", Options::default(), VIDEO_JSON);
        let text = joined(&plan);
        // 超过 256 的尺寸会让 ico 复用器写 0 字节。
        assert!(
            text.contains("force_original_aspect_ratio=decrease"),
            "{text}"
        );
        assert!(text.contains("w=256:h=256"), "{text}");
        assert!(text.contains("format=rgba"), "{text}");
        assert!(text.contains("-frames:v 1"), "{text}");
    }

    #[test]
    fn avif_svtav1_never_sends_still_picture() {
        let plan = build("avif", Options::default(), IMAGE_JSON);
        let text = joined(&plan);
        // libsvtav1 没有 still_picture 选项，带上会 "Option not found"。
        assert!(!text.contains("still-picture"), "{text}");
        assert!(!text.contains("still_picture"), "{text}");
        assert!(text.contains("-c:v libsvtav1"), "{text}");
        assert!(text.contains("-frames:v 1"), "{text}");
    }

    #[test]
    fn jpeg_uses_quantizer_scale() {
        let plan = build("jpg", Options::default(), IMAGE_JSON);
        let text = joined(&plan);
        assert!(text.contains("-c:v mjpeg"), "{text}");
        assert!(text.contains("-q:v"), "{text}");
    }

    #[test]
    fn animated_formats_loop_instead_of_single_frame() {
        for id in ["apng", "webp_anim"] {
            let plan = build(id, Options::default(), VIDEO_JSON);
            let text = joined(&plan);
            assert!(
                !text.contains("-frames:v 1"),
                "{id} 是动图，不该只取一帧：{text}"
            );
            assert!(text.contains("-loop 0"), "{id} 应循环：{text}");
        }
    }

    #[test]
    fn image_from_audio_source_is_rejected() {
        let target = by_id("png").unwrap();
        let err = build_plan(
            "C:/media/a.wav",
            "C:/out/x.png",
            target,
            &Options::default(),
            &probe_of(AUDIO_JSON),
            &caps(),
        )
        .unwrap_err();
        assert!(err.contains("图像"), "{err}");
    }

    // ---------- 字幕 ----------

    #[test]
    fn subtitle_targets_pick_matching_codec() {
        let cases = [
            ("srt", "subrip"),
            ("ass", "ass"),
            ("vtt", "webvtt"),
            ("ttml", "ttml"),
            ("lrc", "text"),
        ];
        for (id, expected) in cases {
            let plan = build(id, Options::default(), SUB_JSON);
            let text = joined(&plan);
            assert!(text.contains(&format!("-c:s {expected}")), "{id}：{text}");
            assert!(text.contains("-map 0:s:0"), "{id}：{text}");
        }
    }

    #[test]
    fn microdvd_is_written_directly_not_by_ffmpeg() {
        let plan = build("microdvd", Options::default(), SUB_JSON);
        assert_eq!(plan.direct_write, Some(DirectWrite::MicroDvd));
        assert!(plan.args.is_empty(), "直接写文本时不该有 ffmpeg 参数");
    }

    #[test]
    fn sbv_is_written_directly_not_by_ffmpeg() {
        let plan = build("sbv", Options::default(), SUB_JSON);
        assert_eq!(plan.direct_write, Some(DirectWrite::Sbv));
        assert!(plan.args.is_empty());
    }

    // ---------- 降级 ----------

    #[test]
    fn unavailable_first_choice_falls_back_with_notice() {
        let limited = Caps::from_names(
            ["aac", "png", "mjpeg", "mpeg4"]
                .iter()
                .map(|s| s.to_string()),
            ["mp4", "image2"].iter().map(|s| s.to_string()),
        );
        let target = by_id("mp4").unwrap();
        let plan = build_plan(
            "C:/media/source.mp4",
            "C:/out/x.mp4",
            target,
            &Options::default(),
            &probe_of(VIDEO_JSON),
            &limited,
        )
        .unwrap();
        // 候选链首选 libx264 不可用，应当降级到 mpeg4 并说明。
        let text = plan.args.join(" ");
        assert!(text.contains("-c:v mpeg4"), "{text}");
        assert!(plan.notice.as_deref().unwrap_or("").contains("mpeg4"));
    }

    #[test]
    fn explicit_override_wins_when_available() {
        let plan = build(
            "mkv",
            Options {
                video_encoder_override: Some("libx265".into()),
                ..Default::default()
            },
            VIDEO_JSON,
        );
        assert!(joined(&plan).contains("-c:v libx265"));
    }

    // ---------- 纯度 ----------

    #[test]
    fn source_has_no_process_or_filesystem_access() {
        let source = include_str!("args.rs");
        // 只看测试模块之前的生产代码；测试里出现的字面量不算。
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        let code: String = production
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        // 参数构建必须是纯函数，这条测试守住边界。
        assert!(
            !code.contains("std::process"),
            "args.rs 不应出现 std::process"
        );
        assert!(!code.contains("std::fs"), "args.rs 不应出现 std::fs");
        assert!(!code.contains("Command::new"), "args.rs 不应启动进程");
    }

    #[test]
    fn quality_maps_to_valid_ranges() {
        assert_eq!(map_crf(0, 51, 12), 51);
        assert_eq!(map_crf(100, 51, 12), 12);
        assert_eq!(map_crf(50, 51, 12), 32);
        // 越界的输入被夹住。
        assert_eq!(map_crf(200, 51, 12), 12);
    }
}
