//! 把「请求 + 探测结果 + 工具能力」编译成 ffmpeg 参数表。
//!
//! 这是整个应用里最需要可测的部分，因此保持**纯函数**：
//! 不启动进程、不读写文件、不依赖环境变量。
//! 输出路径由调用方算好后传进来，本模块只负责拼参数。
//!
//! 有一条测试会读本文件源码，断言其中不出现 `std::process` 与 `std::fs`。

use crate::caps::Caps;
use crate::formats::{Format, Kind, Pipeline};
use crate::probe::Probe;

/// 用户可调的转换参数。
#[derive(Debug, Clone)]
pub struct Options {
    /// 0–100，越大质量越高、体积越大。
    pub quality: u8,
    pub audio_bitrate_kbps: Option<u32>,
    /// 缩放上限，保持宽高比，不放大。
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    /// 直接复制码流，不重新编码。
    pub copy_streams: bool,
    pub strip_metadata: bool,
    pub video_encoder_override: Option<String>,
    pub audio_encoder_override: Option<String>,
    pub gif_fps: u8,
    pub gif_max_width: u32,
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
        }
    }
}

impl Options {
    fn clamped_quality(&self) -> u8 {
        self.quality.min(100)
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

        let wants_audio = !self.target.audio_encoders.is_empty()
            || self.options.audio_encoder_override.is_some();

        if self.options.copy_streams {
            self.push2("-c:v", "copy");
            self.video_encoder = Some("copy".into());
            if wants_audio {
                self.push2("-map", "0:a:0?");
                self.push2("-c:a", "copy");
                self.audio_encoder = Some("copy".into());
            } else {
                self.push("-an");
            }
        } else {
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
                    .is_some_and(|e| e.starts_with("libvpx"))
                => {
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

        let fps = self.options.gif_fps.clamp(1, 50);
        let width = self.options.gif_max_width.clamp(32, 1920);

        self.push2(
            "-filter_complex",
            &format!(
                "[0:v]fps={fps},scale={width}:-1:flags=lanczos,split[a][b];\
                 [a]palettegen=stats_mode=diff[p];\
                 [b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle"
            ),
        );
        self.push2("-loop", "0");
        self.push("-an");
        self.video_encoder = Some("gif".into());
        self.notices
            .push(format!("GIF 输出 {fps}fps、宽 {width}px"));

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

        self.notices.push("DV 固定为 PAL 720×576、48 kHz 立体声".into());
        Ok(())
    }

    /// MXF：复用器只认 48 kHz，编码器组合用 dnxhd 保证稳定。
    fn mxf(&mut self) -> Result<(), String> {
        let (want_w, want_h) = self
            .probe
            .video()
            .map(|v| (v.width, v.height))
            .unwrap_or((1920, 1080));
        let (w, h) = if want_w >= want_h { (1920, 1080) } else { (1080, 1920) };

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
            self.push2("-c:a", "copy");
            self.audio_encoder = Some("copy".into());
            return Ok(());
        }

        let encoder = self.resolve_audio_encoder();
        self.push2("-c:a", &encoder);
        self.append_audio_quality(&encoder);
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

        Ok(())
    }

    // ---------- 编码器决议 ----------

    fn resolve_video_encoder(&mut self) -> String {
        if let Some(override_name) = self.options.video_encoder_override.as_deref() {
            if self.caps.has_encoder(override_name) {
                return override_name.to_string();
            }
            self.notices
                .push(format!("编码器 {override_name} 在当前 ffmpeg 中不可用，已回退"));
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
        if let Some(override_name) = self.options.audio_encoder_override.as_deref() {
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
            && self.caps.has_encoder(name) {
                return name.to_string();
            }

        if !self.target.video_encoders.is_empty()
            && let Some(resolved) = self.caps.resolve_encoder(self.target.video_encoders) {
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

        match encoder {
            "libx264" | "libx265" => {
                let crf = map_crf(q, 51, 12);
                self.push2("-crf", &crf.to_string());
                let preset = quality_preset(q);
                self.push2("-preset", preset);
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
        let q = self.options.clamped_quality();

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
        let bitrate = self
            .options
            .audio_bitrate_kbps
            .unwrap_or(192)
            .clamp(32, 1536);

        match encoder {
            "flac" => {
                self.push2("-compression_level", "8");
            }

            "libmp3lame" => {
                self.push2("-b:a", &format!("{bitrate}k"));
            }

            "libvorbis" => {
                let qa = (bitrate / 32).min(10);
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
            | "pcm_s24be" | "pcm_alaw" | "pcm_mulaw" | "wavpack" | "tta" | "truehd" | "mlp"
            | "copy" => {}

            _ => {
                self.push2("-b:a", &format!("{bitrate}k"));
            }
        }
    }

    fn append_scale(&mut self) {
        let max_w = self.options.max_width.unwrap_or(0);
        let max_h = self.options.max_height.unwrap_or(0);
        if max_w == 0 && max_h == 0 {
            return;
        }

        // 用 force_original_aspect_ratio 限制长边，避免小图被放大。
        let expr = match (max_w > 0, max_h > 0) {
            (true, true) => format!(
                "scale='min({max_w},iw)':'min({max_h},ih)':force_original_aspect_ratio=decrease"
            ),
            (true, false) => format!("scale='min({max_w},iw)':-2"),
            (false, true) => format!("scale=-2:'min({max_h},ih)'"),
            (false, false) => return,
        };
        self.push2("-vf", &format!("{expr}:flags=lanczos"));
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
    const SUB_JSON: &str = include_str!("../tests/fixtures/probe_subtitle.json");
    const IMAGE_JSON: &str = include_str!("../tests/fixtures/probe_image.json");
    const SILENT_JSON: &str = include_str!("../tests/fixtures/probe_video_silent.json");

    fn caps() -> Caps {
        Caps::from_names(
            [
                "libx264", "libx265", "libsvtav1", "libaom-av1", "libvpx-vp9", "libvpx",
                "libtheora", "libmp3lame", "aac", "libopus", "libvorbis", "flac", "alac",
                "wmav2", "ac3", "eac3", "dca", "tta", "wavpack", "libwebp", "libwebp_anim",
                "libjxl", "apng", "gif", "tiff", "prores", "dnxhd", "dvvideo", "ffv1",
                "utvideo", "huffyuv", "png", "mjpeg", "jpeg2000", "jpegls", "qoi", "targa",
                "sgi", "sunrast", "xbm", "wbmp", "fits", "exr", "hdr", "dpx", "pam", "pgm",
                "ppm", "pbm", "pfm", "pcx", "bmp", "subrip", "ass", "webvtt", "ttml", "text",
                "mov_text", "mpeg4", "libxvid", "mpeg2video", "mpeg1video", "wmv2", "h263",
                "mp2", "pcm_s16le", "pcm_s24le", "pcm_s16be",
            ]
            .iter()
            .map(|s| s.to_string()),
            ["mp4", "matroska", "webm", "mov", "avi", "asf", "flv", "mpegts", "mpeg", "vob",
             "3gp", "ogg", "nut", "ivf", "mxf", "dv", "gif", "h264", "hevc", "mp3", "ipod",
             "adts", "flac", "wav", "aiff", "opus", "ac3", "eac3", "dts", "tta", "wv",
             "image2", "webp", "avif", "apng", "ico", "srt", "ass", "webvtt", "ttml", "lrc"]
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
        assert!(text.contains("-progress pipe:1"), "必须用 -progress 做进度：{text}");
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
        let low = build("mp4", Options { quality: 20, ..Default::default() }, VIDEO_JSON);
        let high = build("mp4", Options { quality: 95, ..Default::default() }, VIDEO_JSON);
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
            Options { copy_streams: true, ..Default::default() },
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
            Options { audio_bitrate_kbps: Some(320), ..Default::default() },
            VIDEO_JSON,
        );
        assert!(joined(&plan).contains("-b:a 320k"), "{}", joined(&plan));
    }

    #[test]
    fn resolution_cap_uses_scale_not_upscale() {
        let plan = build(
            "mp4",
            Options { max_height: Some(720), ..Default::default() },
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
            Options { strip_metadata: true, ..Default::default() },
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
            assert!(!text.contains("-b:a"), "{id} 是无损格式，不该带 -b:a：{text}");
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
        assert!(text.contains("force_original_aspect_ratio=decrease"), "{text}");
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
            assert!(!text.contains("-frames:v 1"), "{id} 是动图，不该只取一帧：{text}");
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
            ["aac", "png", "mjpeg", "mpeg4"].iter().map(|s| s.to_string()),
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
        let production = source
            .split("#[cfg(test)]")
            .next()
            .unwrap_or(source);
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
