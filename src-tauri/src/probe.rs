//! ffprobe 输出解析。
//!
//! 只解析本程序需要的字段：时长、流类型、分辨率、帧率、采样率、像素格式。
//! 解析函数是纯的（输入 JSON 文本，输出结构），调用 ffprobe 的部分在 `probe_io`。

use serde::{Deserialize, Serialize};

use crate::formats::Kind;

/// 一条媒体流。
#[derive(Debug, Clone, Serialize)]
pub struct Stream {
    pub index: u32,
    pub kind: StreamKind,
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: Option<f64>,
    pub pix_fmt: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    /// 音频文件里内嵌的封面图会被 ffprobe 报成视频流，需要单独识别。
    pub attached_pic: bool,
    /// 帧数。ffprobe 对静态图常给 None，此时按 1 处理。
    pub frames: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamKind {
    Video,
    Audio,
    Subtitle,
    Other,
}

impl Stream {
    /// 「1920×1080 · h264 · 30fps」这类给人看的摘要。
    pub fn describe(&self) -> String {
        match self.kind {
            StreamKind::Video if self.attached_pic => {
                format!("{} 封面 {}×{}", self.codec, self.width, self.height)
            }
            StreamKind::Video if self.width > 0 && self.height > 0 => {
                let mut s = format!("{}×{} · {}", self.width, self.height, self.codec);
                if let Some(fps) = self.fps
                    && fps > 0.0
                {
                    s.push_str(&format!(" · {fps:.2}fps"));
                }
                s
            }
            StreamKind::Video => self.codec.clone(),
            StreamKind::Audio => {
                let mut parts = vec![self.codec.clone()];
                if let Some(ch) = self.channels {
                    parts.push(format!("{ch}ch"));
                }
                if let Some(sr) = self.sample_rate {
                    parts.push(format!("{:.1}kHz", sr as f64 / 1000.0));
                }
                parts.join(" · ")
            }
            StreamKind::Subtitle => format!("字幕 {}", self.codec),
            StreamKind::Other => self.codec.clone(),
        }
    }
}

/// ffprobe 结果的结构化视图。
#[derive(Debug, Clone, Serialize)]
pub struct Probe {
    pub path: String,
    pub format: String,
    pub duration_secs: f64,
    pub size_bytes: u64,
    pub bit_rate: u64,
    pub streams: Vec<Stream>,
}

impl Probe {
    pub fn video(&self) -> Option<&Stream> {
        self.streams
            .iter()
            .find(|s| s.kind == StreamKind::Video && !s.attached_pic)
    }

    pub fn audio(&self) -> Option<&Stream> {
        self.streams.iter().find(|s| s.kind == StreamKind::Audio)
    }

    pub fn subtitle(&self) -> Option<&Stream> {
        self.streams.iter().find(|s| s.kind == StreamKind::Subtitle)
    }

    pub fn has_video(&self) -> bool {
        self.video().is_some()
    }

    pub fn has_audio(&self) -> bool {
        self.audio().is_some()
    }

    pub fn has_subtitle(&self) -> bool {
        self.subtitle().is_some()
    }

    /// 这个文件属于哪一类。用于建任务时判定兼容性。
    ///
    /// 判定的关键难点是**静态图与视频在 ffprobe 里长得一样**：
    /// PNG 也是一个 `codec_type: video` 的流，容器名是 `png_pipe`。
    /// 如果只看流类型，一张 PNG 会被判成视频，于是「图像 → 图像」变成
    /// 「视频 → 图像」，用户还会看到 0.04 秒的假时长。
    ///
    /// 因此先看容器名，再看帧数：
    /// * 图像类容器 + 单帧 → 图像
    /// * 图像类容器 + 多帧（动图 GIF/APNG/WebP）→ 视频（这样 GIF→MP4 才可用，
    ///   而视频本来就能转图像，两种目标都不吃亏）
    /// * 其余容器有视频轨 → 视频
    pub fn kind(&self) -> Kind {
        if let Some(v) = self.video() {
            if self.is_image_container() {
                let frames = v.frames.unwrap_or(1);
                let animated = self.is_animatable_image_container() && frames > 1;
                return if animated { Kind::Video } else { Kind::Image };
            }
            return Kind::Video;
        }

        if self.has_audio() {
            return Kind::Audio;
        }
        if self.has_subtitle() {
            return Kind::Subtitle;
        }
        Kind::Image
    }

    /// 容器是否属于「图像」这一类。ffprobe 对单图给的名字是 `xxx_pipe`。
    fn is_image_container(&self) -> bool {
        let name = self.format.split(',').next().unwrap_or("").trim();
        const IMAGE_CONTAINERS: &[&str] = &[
            "image2",
            "image2pipe",
            "png_pipe",
            "jpeg_pipe",
            "mjpeg",
            "webp_pipe",
            "jxl_pipe",
            "gif",
            "apng",
            "avif",
            "ico",
            "bmp_pipe",
            "tiff_pipe",
            "targa_pipe",
            "pcx_pipe",
            "qoi_pipe",
            "exr",
            "hdr_pipe",
            "dpx_pipe",
            "sgi_pipe",
            "sunrast_pipe",
            "fits_pipe",
            "pam_pipe",
            "pgm_pipe",
            "ppm_pipe",
            "pbm_pipe",
            "pfm_pipe",
            "jpeg2000_pipe",
            "jpegls_pipe",
        ];
        IMAGE_CONTAINERS.contains(&name)
    }

    /// 允许动画的图像容器。多帧时按视频处理。
    fn is_animatable_image_container(&self) -> bool {
        matches!(self.format.as_str(), "gif" | "apng" | "webp_pipe")
    }

    /// 是否带 alpha 通道（决定 PNG/JXL 输出用不用 rgba）。
    pub fn has_alpha(&self) -> bool {
        self.video()
            .and_then(|v| v.pix_fmt.as_deref())
            .is_some_and(|fmt| {
                let f = fmt.to_ascii_lowercase();
                f.starts_with("yuva") || f.contains("rgba") || f.starts_with("gbrap") || f == "pal8"
            })
    }
}

// ---------- 原始 JSON 结构 ----------

#[derive(Debug, Deserialize)]
struct RawProbe {
    #[serde(default)]
    streams: Vec<RawStream>,
    #[serde(default)]
    format: RawFormat,
}

#[derive(Debug, Deserialize, Default)]
struct RawFormat {
    #[serde(default)]
    format_name: String,
    #[serde(default)]
    duration: Option<String>,
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    bit_rate: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RawStream {
    #[serde(default)]
    index: u32,
    #[serde(default)]
    codec_type: String,
    #[serde(default)]
    codec_name: String,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    pix_fmt: Option<String>,
    #[serde(default)]
    sample_rate: Option<String>,
    #[serde(default)]
    channels: Option<u32>,
    #[serde(default)]
    r_frame_rate: Option<String>,
    #[serde(default)]
    avg_frame_rate: Option<String>,
    #[serde(default)]
    duration: Option<String>,
    #[serde(default)]
    nb_frames: Option<String>,
    #[serde(default)]
    disposition: RawDisposition,
}

#[derive(Debug, Deserialize, Default)]
struct RawDisposition {
    #[serde(default)]
    attached_pic: u8,
}

/// 解析 ffprobe 的 `-print_format json` 输出。
pub fn parse(json: &str, source_path: &str) -> Result<Probe, String> {
    let raw: RawProbe =
        serde_json::from_str(json).map_err(|e| format!("ffprobe 输出不是合法 JSON：{e}"))?;

    let streams: Vec<Stream> = raw
        .streams
        .iter()
        .map(|s| Stream {
            index: s.index,
            kind: match s.codec_type.as_str() {
                "video" => StreamKind::Video,
                "audio" => StreamKind::Audio,
                "subtitle" => StreamKind::Subtitle,
                _ => StreamKind::Other,
            },
            codec: if s.codec_name.is_empty() {
                "unknown".to_string()
            } else {
                s.codec_name.clone()
            },
            width: s.width.unwrap_or(0),
            height: s.height.unwrap_or(0),
            fps: s
                .avg_frame_rate
                .as_deref()
                .and_then(parse_rational)
                .or_else(|| s.r_frame_rate.as_deref().and_then(parse_rational))
                .filter(|f| *f > 0.0),
            pix_fmt: s.pix_fmt.clone(),
            sample_rate: s.sample_rate.as_deref().and_then(|v| v.parse().ok()),
            channels: s.channels,
            attached_pic: s.disposition.attached_pic == 1,
            frames: s.nb_frames.as_deref().and_then(|v| v.parse().ok()),
        })
        .collect();

    // 时长可能来自 format，也可能只在流上；取第一个正数。
    let mut duration = raw
        .format
        .duration
        .as_deref()
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(0.0);

    if duration <= 0.0 {
        duration = raw
            .streams
            .iter()
            .filter_map(|s| s.duration.as_deref().and_then(|d| d.parse::<f64>().ok()))
            .fold(0.0_f64, f64::max);
    }

    Ok(Probe {
        path: source_path.to_string(),
        format: raw.format.format_name,
        duration_secs: if duration.is_finite() && duration > 0.0 {
            duration
        } else {
            0.0
        },
        size_bytes: raw
            .format
            .size
            .as_deref()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        bit_rate: raw
            .format
            .bit_rate
            .as_deref()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        streams,
    })
}

/// `30000/1001` → 29.97。分母为 0 时返回 None。
fn parse_rational(text: &str) -> Option<f64> {
    let (num, den) = text.split_once('/')?;
    let num: f64 = num.parse().ok()?;
    let den: f64 = den.parse().ok()?;
    if den == 0.0 || !num.is_finite() {
        return None;
    }
    let value = num / den;
    if value.is_finite() { Some(value) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIDEO_JSON: &str = include_str!("../tests/fixtures/probe_video.json");
    const AUDIO_JSON: &str = include_str!("../tests/fixtures/probe_audio.json");
    const SUBTITLE_JSON: &str = include_str!("../tests/fixtures/probe_subtitle.json");
    const IMAGE_JSON: &str = include_str!("../tests/fixtures/probe_image.json");
    const COVER_JSON: &str = include_str!("../tests/fixtures/probe_audio_with_cover.json");
    const GIF_JSON: &str = include_str!("../tests/fixtures/probe_animated_gif.json");

    #[test]
    fn parses_video_stream_details() {
        let p = parse(VIDEO_JSON, "a.mp4").unwrap();
        assert_eq!(p.kind(), Kind::Video);
        assert_eq!(p.duration_secs.round(), 2.0);
        let v = p.video().expect("应有视频轨");
        assert_eq!((v.width, v.height), (640, 480));
        assert_eq!(v.codec, "h264");
        assert!((v.fps.unwrap() - 25.0).abs() < 0.01);
        assert!(p.has_audio());
        assert!(!p.has_alpha());
    }

    #[test]
    fn parses_audio_and_classifies_as_audio() {
        let p = parse(AUDIO_JSON, "a.wav").unwrap();
        assert_eq!(p.kind(), Kind::Audio);
        let a = p.audio().unwrap();
        assert_eq!(a.sample_rate, Some(48000));
        assert_eq!(a.channels, Some(2));
        assert!(!p.has_video());
    }

    #[test]
    fn subtitle_only_file_classifies_as_subtitle() {
        let p = parse(SUBTITLE_JSON, "a.srt").unwrap();
        assert_eq!(p.kind(), Kind::Subtitle);
        assert!(p.has_subtitle());
    }

    #[test]
    fn image_classifies_as_image() {
        let p = parse(IMAGE_JSON, "a.png").unwrap();
        assert_eq!(p.kind(), Kind::Image);
        assert_eq!(p.video().unwrap().width, 320);
        assert_eq!(p.video().unwrap().height, 240);
    }

    #[test]
    fn cover_art_is_not_treated_as_video_track() {
        // m4a/mp3 常内嵌封面，ffprobe 会把封面报成 video 流并标记 attached_pic。
        // 若把它当视频轨，音频文件会被误判成「视频」。
        let p = parse(COVER_JSON, "a.m4a").unwrap();
        assert_eq!(p.kind(), Kind::Audio, "带封面的音频不应判成视频");
        assert!(p.has_audio());
        assert!(!p.has_video());
    }

    #[test]
    fn parses_rational_frame_rate() {
        assert!((parse_rational("30000/1001").unwrap() - 29.97).abs() < 0.01);
        assert_eq!(parse_rational("0/0"), None);
        assert_eq!(parse_rational("25"), None);
    }

    #[test]
    fn static_image_is_not_mistaken_for_video() {
        // PNG 在 ffprobe 里同样是 codec_type=video 的流，容器名是 png_pipe。
        // 只看流类型会把一张图判成视频，进而让「图像→图像」的兼容性判断出错。
        let p = parse(IMAGE_JSON, "a.png").unwrap();
        assert_eq!(p.format, "png_pipe");
        assert_eq!(p.kind(), Kind::Image);
    }

    #[test]
    fn animated_gif_counts_as_video_so_it_can_become_mp4() {
        // 多帧 GIF 应能转成视频容器；单帧的仍按图像处理。
        let p = parse(GIF_JSON, "a.gif").unwrap();
        assert_eq!(p.kind(), Kind::Video, "多帧 GIF 应能当视频源");
        assert!(p.video().unwrap().frames.unwrap_or(0) > 1);
    }

    #[test]
    fn malformed_json_reports_error() {
        assert!(parse("not json", "x").is_err());
    }
}
