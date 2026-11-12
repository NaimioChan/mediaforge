//! 每个格式可调参数的规格表与取值容器。
//!
//! 界面的参数面板不再写死两个下拉框，而是按当前目标格式向本模块要一份
//! `ParamSpec` 列表，据此动态渲染控件。这样「MP4 能调编码器和 preset、
//! FLAC 能调压缩级别、GIF 能调颜色数」这类差异全部由数据描述，
//! 界面不需要认识具体格式。
//!
//! 本模块是纯函数：只读格式定义与能力清单，不碰进程与文件系统。

use serde::{Deserialize, Serialize};

use crate::caps::Caps;
use crate::formats::{Format, Kind, Pipeline};

// ---------- 规格 ----------

/// 控件形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ParamKind {
    /// 下拉选择。
    Select,
    /// 滑块，配合 `min` / `max` / `step`。
    Slider,
    /// 数值输入，留空表示不限制。
    Number,
    /// 开关。
    Toggle,
}

/// 下拉项。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub value: String,
    pub label: String,
}

impl Choice {
    fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// 一条可调参数。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamSpec {
    /// 与 `Tune` 的字段一一对应。
    pub id: &'static str,
    pub label: &'static str,
    /// 面板分组：视频 / 音频 / 容器 / 图像 / 字幕。
    pub group: &'static str,
    pub kind: ParamKind,
    /// 默认值的字符串形式，前端直接填进控件。
    pub default: String,
    pub choices: Vec<Choice>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub unit: &'static str,
    pub help: &'static str,
}

impl ParamSpec {
    fn select(
        id: &'static str,
        label: &'static str,
        group: &'static str,
        choices: Vec<Choice>,
        default: &str,
        help: &'static str,
    ) -> Self {
        Self {
            id,
            label,
            group,
            kind: ParamKind::Select,
            default: default.to_string(),
            choices,
            min: None,
            max: None,
            step: None,
            unit: "",
            help,
        }
    }

    /// 滑块：min/max/step/default 四个数值参数是规格本身的形态，
    /// 拆成结构体反而让调用处更难读。
    #[allow(clippy::too_many_arguments)]
    fn slider(
        id: &'static str,
        label: &'static str,
        group: &'static str,
        min: f64,
        max: f64,
        step: f64,
        default: f64,
        help: &'static str,
    ) -> Self {
        Self {
            id,
            label,
            group,
            kind: ParamKind::Slider,
            default: format!("{}", default as u32),
            choices: Vec::new(),
            min: Some(min),
            max: Some(max),
            step: Some(step),
            unit: "",
            help,
        }
    }

    fn number(
        id: &'static str,
        label: &'static str,
        group: &'static str,
        unit: &'static str,
        help: &'static str,
    ) -> Self {
        Self {
            id,
            label,
            group,
            kind: ParamKind::Number,
            default: String::new(),
            choices: Vec::new(),
            min: None,
            max: None,
            step: None,
            unit,
            help,
        }
    }

    fn toggle(
        id: &'static str,
        label: &'static str,
        group: &'static str,
        default: bool,
        help: &'static str,
    ) -> Self {
        Self {
            id,
            label,
            group,
            kind: ParamKind::Toggle,
            default: if default { "1" } else { "0" }.to_string(),
            choices: Vec::new(),
            min: None,
            max: None,
            step: None,
            unit: "",
            help,
        }
    }
}

// ---------- 取值 ----------

/// 用户对某条任务做的细调。字段全为 `None` 时表示「全用默认」。
///
/// 之所以不把所有字段做成非空：切换目标格式时旧值可能不再适用，
/// 留空让参数构建阶段退回该格式的默认行为，比强行保留一个失效值好。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Tune {
    pub video_encoder: Option<String>,
    pub audio_encoder: Option<String>,
    /// 0–100 的综合质量，映射到编码器原生参数。
    pub quality: Option<u8>,
    /// 目标视频码率（kbps）。给了就不用 CRF 类质量参数。
    pub video_bitrate_kbps: Option<u32>,
    /// libx264 / libx265 / libsvtav1 的预设。
    pub preset: Option<String>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    /// 输出帧率。
    pub fps: Option<f64>,
    pub audio_bitrate_kbps: Option<u32>,
    pub audio_sample_rate: Option<u32>,
    pub audio_channels: Option<u32>,
    /// Vorbis 的 `-q:a`（0–10）或 FLAC 的压缩级别（0–12），按编码器解释。
    pub audio_quality: Option<u8>,
    pub gif_fps: Option<u8>,
    pub gif_width: Option<u32>,
    pub gif_colors: Option<u16>,
    pub gif_dither: Option<String>,
    pub gif_loop: Option<bool>,
    /// MicroDVD 的帧率基准。
    pub subtitle_fps: Option<f64>,
    /// 图像单独的质量，避免和视频质量互相干扰。
    pub image_quality: Option<u8>,
}

impl Tune {
    /// 丢弃该格式不认识的字段，并把数值夹到合法区间。
    ///
    /// 切换目标格式后必须调用一次：从 MP4 切到 FLAC 时，
    /// 残留的 `video_encoder` 会被参数构建阶段忽略，
    /// 但面板会显示一个并不存在的选项，所以直接清掉更干净。
    pub fn sanitize(mut self, target: &Format, caps: &Caps) -> Self {
        let specs = specs_for(target, caps);
        let has = |id: &str| specs.iter().any(|s| s.id == id);

        // 该字段不被格式支持，或值是规格里不存在的选项，都要清掉。
        let keep = |id: &str, value: &Option<String>| -> Option<String> {
            let name = value.as_ref()?;
            if !has(id) {
                return None;
            }
            if spec_choices(&specs, id).contains(name) {
                Some(name.clone())
            } else {
                None
            }
        };
        self.video_encoder = keep("videoEncoder", &self.video_encoder);
        self.audio_encoder = keep("audioEncoder", &self.audio_encoder);

        if !has("preset") {
            self.preset = None;
        }
        if !has("quality") {
            self.quality = None;
        }
        if !has("videoBitrateKbps") {
            self.video_bitrate_kbps = None;
        }
        if !has("maxWidth") {
            self.max_width = None;
        }
        if !has("maxHeight") {
            self.max_height = None;
        }
        if !has("fps") {
            self.fps = None;
        }
        if !has("audioBitrateKbps") {
            self.audio_bitrate_kbps = None;
        }
        if !has("audioSampleRate") {
            self.audio_sample_rate = None;
        }
        if !has("audioChannels") {
            self.audio_channels = None;
        }
        if !has("audioQuality") {
            self.audio_quality = None;
        }
        if !has("gifFps") {
            self.gif_fps = None;
        }
        if !has("gifWidth") {
            self.gif_width = None;
        }
        if !has("gifColors") {
            self.gif_colors = None;
        }
        if !has("gifDither") {
            self.gif_dither = None;
        }
        if !has("gifLoop") {
            self.gif_loop = None;
        }
        if !has("subtitleFps") {
            self.subtitle_fps = None;
        }
        if !has("imageQuality") {
            self.image_quality = None;
        }

        self.quality = self.quality.map(|q| q.min(100));
        self.image_quality = self.image_quality.map(|q| q.min(100));
        self.audio_quality = self.audio_quality.map(|q| q.min(12));
        self.audio_channels = self.audio_channels.map(|c| c.clamp(1, 8));
        self.gif_fps = self.gif_fps.map(|f| f.clamp(1, 50));
        self.gif_width = self.gif_width.map(|w| w.clamp(32, 1920));
        self.gif_colors = self.gif_colors.map(|c| c.clamp(2, 256));
        self.fps = self.fps.filter(|f| *f > 0.0 && *f <= 240.0);
        self.subtitle_fps = self.subtitle_fps.filter(|f| *f > 0.0 && *f <= 240.0);
        self
    }
}

fn spec_choices(specs: &[ParamSpec], id: &str) -> Vec<String> {
    specs
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.choices.iter().map(|c| c.value.clone()).collect())
        .unwrap_or_default()
}

// ---------- 规格生成 ----------

/// 某个格式的可调参数。顺序即面板显示顺序。
pub fn specs_for(target: &Format, caps: &Caps) -> Vec<ParamSpec> {
    match target.kind {
        Kind::Video => video_specs(target, caps),
        Kind::Audio => audio_specs(target, caps),
        Kind::Image => image_specs(target, caps),
        Kind::Subtitle => subtitle_specs(target),
    }
}

fn video_specs(target: &Format, caps: &Caps) -> Vec<ParamSpec> {
    let mut out = Vec::new();

    // GIF 走调色板管线，参数完全是另一套。
    if target.pipeline == Pipeline::Gif {
        out.push(ParamSpec::slider(
            "gifFps",
            "帧率",
            "动图",
            1.0,
            50.0,
            1.0,
            12.0,
            "帧率越高越流畅，体积也越大",
        ));
        out.push(ParamSpec::slider(
            "gifWidth",
            "宽度",
            "动图",
            32.0,
            1920.0,
            16.0,
            480.0,
            "高度按原比例自动推算",
        ));
        out.push(ParamSpec::slider(
            "gifColors",
            "颜色数",
            "动图",
            2.0,
            256.0,
            1.0,
            256.0,
            "调色板容量，少于 256 色体积更小",
        ));
        out.push(ParamSpec::select(
            "gifDither",
            "抖动",
            "动图",
            vec![
                Choice::new("bayer", "有序抖动（体积小）"),
                Choice::new("sierra2_4a", "误差扩散（画质好）"),
                Choice::new("none", "不抖动"),
            ],
            "bayer",
            "渐变区域如何近似到调色板",
        ));
        out.push(ParamSpec::toggle(
            "gifLoop",
            "无限循环",
            "动图",
            true,
            "关闭后只播放一次",
        ));
        return out;
    }

    // 编码器：只列出当前 ffmpeg 真有的。
    let encoders = encoder_choices(target, caps);
    if encoders.len() > 1 {
        out.push(ParamSpec::select(
            "videoEncoder",
            "视频编码器",
            "视频",
            encoders,
            target.video_encoders.first().copied().unwrap_or("libx264"),
            "换编码器会改变兼容性与速度",
        ));
    }

    // DV 与 MXF 由专用管线接管：尺寸、采样率、码率全部固定，
    // 给用户可调的质量与分辨率只会误导，因此只保留真正的接缝参数。
    let fixed_pipeline = matches!(target.pipeline, Pipeline::Dv | Pipeline::Mxf);
    if fixed_pipeline {
        out.push(ParamSpec::number(
            "fps",
            "帧率",
            "视频",
            "fps",
            "留空保持源帧率",
        ));
        if !target.audio_encoders.is_empty() {
            out.push(ParamSpec::select(
                "audioSampleRate",
                "采样率",
                "音频",
                sample_rate_choices(),
                "",
                "专用管线强制 48 kHz，改动会覆盖该约束",
            ));
        }
        return out;
    }

    out.push(ParamSpec::slider(
        "quality",
        "质量",
        "视频",
        0.0,
        100.0,
        1.0,
        80.0,
        "数值越高越清晰，体积也越大",
    ));
    out.push(ParamSpec::number(
        "videoBitrateKbps",
        "目标码率",
        "视频",
        "kbps",
        "填了就用固定码率，质量滑块失效；留空用质量模式",
    ));
    out.push(ParamSpec::select(
        "preset",
        "编码预设",
        "视频",
        vec![
            Choice::new("veryfast", "veryfast（最快）"),
            Choice::new("fast", "fast"),
            Choice::new("medium", "medium（均衡）"),
            Choice::new("slow", "slow"),
            Choice::new("veryslow", "veryslow（最慢最好）"),
        ],
        "medium",
        "越慢压缩率越高，画质相同而体积更小",
    ));
    out.push(ParamSpec::number(
        "maxWidth",
        "宽度上限",
        "视频",
        "px",
        "留空不限制；只填一边时另一边按比例推算",
    ));
    out.push(ParamSpec::number(
        "maxHeight",
        "高度上限",
        "视频",
        "px",
        "不会放大超过原始尺寸",
    ));
    out.push(ParamSpec::number(
        "fps",
        "帧率",
        "视频",
        "fps",
        "留空保持源帧率",
    ));

    if !target.audio_encoders.is_empty() {
        let audio_encoders = audio_encoder_choices(target, caps);
        if audio_encoders.len() > 1 {
            out.push(ParamSpec::select(
                "audioEncoder",
                "音频编码器",
                "音频",
                audio_encoders,
                target.audio_encoders[0],
                "容器不支持某些编码器，选了会自动回退",
            ));
        }
        out.push(ParamSpec::slider(
            "audioBitrateKbps",
            "音频码率",
            "音频",
            32.0,
            512.0,
            16.0,
            192.0,
            "只对有损编码器生效",
        ));
        out.push(ParamSpec::select(
            "audioSampleRate",
            "采样率",
            "音频",
            sample_rate_choices(),
            "",
            "保持源采样率通常最安全",
        ));
        out.push(ParamSpec::select(
            "audioChannels",
            "声道",
            "音频",
            channel_choices(),
            "",
            "保持源声道数通常最安全",
        ));
    }

    out
}

fn audio_specs(target: &Format, caps: &Caps) -> Vec<ParamSpec> {
    let mut out = Vec::new();

    let encoders = audio_encoder_choices(target, caps);
    if encoders.len() > 1 {
        out.push(ParamSpec::select(
            "audioEncoder",
            "音频编码器",
            "音频",
            encoders,
            target.audio_encoders.first().copied().unwrap_or("aac"),
            "容器不支持的编码器会被拒绝",
        ));
    }

    let primary = target
        .audio_encoders
        .first()
        .copied()
        .unwrap_or("aac")
        .to_string();

    if is_lossless(&primary) {
        // 无损编码器没有码率概念，只有压缩级别。
        if matches!(primary.as_str(), "flac" | "wavpack") {
            out.push(ParamSpec::slider(
                "audioQuality",
                "压缩级别",
                "音频",
                0.0,
                12.0,
                1.0,
                8.0,
                "越高压得越小，耗时也越长",
            ));
        }
    } else if matches!(primary.as_str(), "libvorbis") {
        out.push(ParamSpec::slider(
            "audioQuality",
            "质量",
            "音频",
            0.0,
            10.0,
            1.0,
            6.0,
            "Vorbis 的 -q:a，约等于 128–192 kbps",
        ));
    } else {
        out.push(ParamSpec::slider(
            "audioBitrateKbps",
            "码率",
            "音频",
            32.0,
            512.0,
            16.0,
            192.0,
            "越高越接近原始音质",
        ));
    }

    out.push(ParamSpec::select(
        "audioSampleRate",
        "采样率",
        "音频",
        sample_rate_choices(),
        "",
        "转换采样率会触发重采样",
    ));
    out.push(ParamSpec::select(
        "audioChannels",
        "声道",
        "音频",
        channel_choices(),
        "",
        "降为单声道可显著减小体积",
    ));

    out
}

fn image_specs(target: &Format, caps: &Caps) -> Vec<ParamSpec> {
    let mut out = Vec::new();

    let encoders = encoder_choices(target, caps);
    if encoders.len() > 1 {
        out.push(ParamSpec::select(
            "videoEncoder",
            "图像编码器",
            "图像",
            encoders,
            target.video_encoders.first().copied().unwrap_or("png"),
            "决定压缩算法与是否支持透明",
        ));
    }

    if has_quality_knob(target) {
        out.push(ParamSpec::slider(
            "imageQuality",
            "质量",
            "图像",
            0.0,
            100.0,
            1.0,
            90.0,
            "无损格式忽略此项",
        ));
    }

    out.push(ParamSpec::number(
        "maxWidth",
        "宽度上限",
        "图像",
        "px",
        "留空不限制；不会放大",
    ));
    out.push(ParamSpec::number(
        "maxHeight",
        "高度上限",
        "图像",
        "px",
        "留空不限制",
    ));

    out
}

fn subtitle_specs(target: &Format) -> Vec<ParamSpec> {
    let mut out = Vec::new();

    if target.pipeline == Pipeline::MicroDvdText {
        out.push(ParamSpec::slider(
            "subtitleFps",
            "帧率",
            "字幕",
            23.0,
            60.0,
            1.0,
            25.0,
            "MicroDVD 用帧号计时，帧率决定时间换算",
        ));
    }

    out
}

/// 该格式是否有质量参数可调。无损图像格式忽略它，干脆不显示。
fn has_quality_knob(target: &Format) -> bool {
    !matches!(
        target.ext,
        ".bmp" | ".tga" | ".qoi" | ".pam" | ".pgm" | ".ppm" | ".pbm" | ".pfm"
    )
}

fn is_lossless(encoder: &str) -> bool {
    matches!(
        encoder,
        "flac"
            | "alac"
            | "wavpack"
            | "tta"
            | "truehd"
            | "mlp"
            | "pcm_s16le"
            | "pcm_s24le"
            | "pcm_s32le"
            | "pcm_f32le"
            | "pcm_s16be"
            | "pcm_s24be"
            | "pcm_alaw"
            | "pcm_mulaw"
    )
}

fn encoder_choices(target: &Format, caps: &Caps) -> Vec<Choice> {
    let mut out = Vec::new();
    for name in target
        .video_encoders
        .iter()
        .chain(target.video_alternatives.iter())
    {
        if caps.has_encoder(name) && !out.iter().any(|c: &Choice| c.value == *name) {
            out.push(Choice::new(*name, encoder_label(name)));
        }
    }
    out
}

fn audio_encoder_choices(target: &Format, caps: &Caps) -> Vec<Choice> {
    let mut out = Vec::new();
    for name in target
        .audio_encoders
        .iter()
        .chain(target.audio_alternatives.iter())
    {
        if caps.has_encoder(name) && !out.iter().any(|c: &Choice| c.value == *name) {
            out.push(Choice::new(*name, encoder_label(name)));
        }
    }
    out
}

/// 编码器的人话名字。没收录的直接显示原名，不做猜测。
fn encoder_label(name: &str) -> String {
    let label = match name {
        "libx264" => "H.264 (x264)",
        "libx265" => "H.265 (x265)",
        "h264_nvenc" => "H.264 (NVENC 显卡)",
        "hevc_nvenc" => "H.265 (NVENC 显卡)",
        "av1_nvenc" => "AV1 (NVENC 显卡)",
        "h264_qsv" => "H.264 (QSV 核显)",
        "hevc_qsv" => "H.265 (QSV 核显)",
        "h264_amf" => "H.264 (AMF 显卡)",
        "hevc_amf" => "H.265 (AMF 显卡)",
        "av1_amf" => "AV1 (AMF 显卡)",
        "libsvtav1" => "AV1 (SVT)",
        "libaom-av1" => "AV1 (aom，慢)",
        "libvpx-vp9" => "VP9",
        "libvpx" => "VP8",
        "mpeg4" => "MPEG-4 Part 2",
        "mpeg2video" => "MPEG-2",
        "mpeg1video" => "MPEG-1",
        "libxvid" => "Xvid",
        "wmv2" => "WMV2",
        "flv1" => "Sorenson FLV1",
        "h263" => "H.263",
        "mjpeg" => "Motion JPEG",
        "prores" => "ProRes",
        "dnxhd" => "DNxHD",
        "dvvideo" => "DV",
        "ffv1" => "FFV1（无损）",
        "utvideo" => "Ut Video（无损）",
        "huffyuv" => "HuffYUV（无损）",
        "libtheora" => "Theora",
        "png" => "PNG（无损）",
        "gif" => "GIF",
        "libwebp" => "WebP",
        "libwebp_anim" => "WebP 动图",
        "libjxl" => "JPEG XL",
        "tiff" => "TIFF",
        "apng" => "APNG",
        "libmp3lame" => "MP3 (LAME)",
        "aac" => "AAC",
        "libopus" => "Opus",
        "libvorbis" => "Vorbis",
        "flac" => "FLAC（无损）",
        "alac" => "ALAC（无损）",
        "wmav2" => "WMA 2",
        "wma1" => "WMA 1",
        "ac3" => "AC-3",
        "eac3" => "E-AC-3",
        "dca" => "DTS",
        "mp2" => "MP2",
        "tta" => "TTA（无损）",
        "wavpack" => "WavPack（无损）",
        "truehd" => "TrueHD（无损）",
        "pcm_s16le" => "PCM 16 bit",
        "pcm_s24le" => "PCM 24 bit",
        "pcm_s32le" => "PCM 32 bit",
        "pcm_f32le" => "PCM 32 bit 浮点",
        "pcm_s16be" => "PCM 16 bit 大端",
        "pcm_s24be" => "PCM 24 bit 大端",
        "pcm_alaw" => "PCM A-law",
        "pcm_mulaw" => "PCM μ-law",
        _ => return name.to_string(),
    };
    label.to_string()
}

fn sample_rate_choices() -> Vec<Choice> {
    vec![
        Choice::new("", "保持源采样率"),
        Choice::new("8000", "8000 Hz"),
        Choice::new("11025", "11025 Hz"),
        Choice::new("16000", "16000 Hz"),
        Choice::new("22050", "22050 Hz"),
        Choice::new("32000", "32000 Hz"),
        Choice::new("44100", "44100 Hz"),
        Choice::new("48000", "48000 Hz"),
        Choice::new("88200", "88200 Hz"),
        Choice::new("96000", "96000 Hz"),
        Choice::new("192000", "192000 Hz"),
    ]
}

fn channel_choices() -> Vec<Choice> {
    vec![
        Choice::new("", "保持源声道"),
        Choice::new("1", "单声道"),
        Choice::new("2", "立体声"),
        Choice::new("6", "5.1 环绕"),
        Choice::new("8", "7.1 环绕"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::by_id;

    fn caps() -> Caps {
        Caps::from_names(
            [
                "libx264",
                "libx265",
                "libsvtav1",
                "libvpx-vp9",
                "h264_nvenc",
                "mpeg4",
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
                "pcm_s16le",
                "pcm_s24le",
                "pcm_f32le",
                "pcm_s16be",
                "png",
                "mjpeg",
                "libwebp",
                "libjxl",
                "tiff",
                "gif",
            ]
            .iter()
            .map(|s| s.to_string()),
            std::iter::empty::<String>(),
        )
    }

    fn spec_ids(id: &str) -> Vec<&'static str> {
        let target = by_id(id).unwrap();
        specs_for(target, &caps()).iter().map(|s| s.id).collect()
    }

    #[test]
    fn every_format_produces_at_least_one_spec() {
        let c = caps();
        for f in crate::formats::FORMATS {
            let specs = specs_for(f, &c);
            // 字幕格式除了 MicroDVD 之外没有可调项，属于正常。
            if f.kind == Kind::Subtitle && f.pipeline != Pipeline::MicroDvdText {
                continue;
            }
            assert!(!specs.is_empty(), "{} 没有任何可调参数", f.id);
        }
    }

    #[test]
    fn spec_ids_are_unique_within_a_format() {
        let c = caps();
        for f in crate::formats::FORMATS {
            let specs = specs_for(f, &c);
            let mut seen = Vec::new();
            for s in &specs {
                assert!(!seen.contains(&s.id), "{} 的 {} 重复出现", f.id, s.id);
                seen.push(s.id);
            }
        }
    }

    #[test]
    fn audio_format_offers_no_video_knobs() {
        let ids = spec_ids("flac");
        assert!(!ids.contains(&"videoEncoder"), "{ids:?}");
        assert!(!ids.contains(&"maxHeight"), "{ids:?}");
        assert!(
            ids.contains(&"audioQuality"),
            "FLAC 应能调压缩级别：{ids:?}"
        );
    }

    #[test]
    fn lossy_audio_offers_bitrate_instead_of_compression() {
        let ids = spec_ids("mp3");
        assert!(ids.contains(&"audioBitrateKbps"), "{ids:?}");
        assert!(!ids.contains(&"audioQuality"), "{ids:?}");
    }

    #[test]
    fn vorbis_uses_quality_scale() {
        let ids = spec_ids("ogg");
        assert!(ids.contains(&"audioQuality"), "{ids:?}");
        assert!(!ids.contains(&"audioBitrateKbps"), "{ids:?}");
    }

    #[test]
    fn video_format_offers_encoder_and_audio_knobs() {
        let ids = spec_ids("mp4");
        assert!(ids.contains(&"videoEncoder"));
        assert!(ids.contains(&"preset"));
        assert!(ids.contains(&"quality"));
        assert!(ids.contains(&"audioEncoder"));
        assert!(ids.contains(&"audioBitrateKbps"));
    }

    #[test]
    fn gif_uses_palette_knobs_only() {
        let ids = spec_ids("gifv");
        assert!(ids.contains(&"gifColors"), "{ids:?}");
        assert!(ids.contains(&"gifDither"), "{ids:?}");
        assert!(!ids.contains(&"quality"), "{ids:?}");
    }

    #[test]
    fn microdvd_offers_fps() {
        let ids = spec_ids("microdvd");
        assert_eq!(ids, vec!["subtitleFps"]);
    }

    #[test]
    fn fixed_pipelines_hide_knobs_they_would_ignore() {
        // DV 与 MXF 的尺寸/码率由专用管线固定，暴露质量滑块只会误导。
        for id in ["dv", "mxf"] {
            let ids = spec_ids(id);
            assert!(!ids.contains(&"quality"), "{id} 不该有质量滑块：{ids:?}");
            assert!(
                !ids.contains(&"videoBitrateKbps"),
                "{id} 不该有码率输入：{ids:?}"
            );
            assert!(!ids.contains(&"maxWidth"), "{id} 尺寸固定：{ids:?}");
            assert!(ids.contains(&"fps"), "{id} 应能调帧率：{ids:?}");
        }
    }

    #[test]
    fn encoder_choices_only_list_available_encoders() {
        let target = by_id("mp4").unwrap();
        // 只有两个编码器可用时，选择器只应列出这两个。
        let limited = Caps::from_names(
            ["libx264".to_string(), "mpeg4".to_string()],
            std::iter::empty::<String>(),
        );
        let specs = specs_for(target, &limited);
        let choices: Vec<String> = specs
            .iter()
            .find(|s| s.id == "videoEncoder")
            .map(|s| s.choices.iter().map(|c| c.value.clone()).collect())
            .unwrap_or_default();
        assert_eq!(
            choices,
            vec!["libx264", "mpeg4"],
            "只应列出真实存在的编码器"
        );
    }

    #[test]
    fn single_available_encoder_hides_the_picker() {
        // 只有一个选择时不该显示下拉框，那是无意义的界面噪音。
        let target = by_id("mp4").unwrap();
        let limited = Caps::from_names(["libx264".to_string()], std::iter::empty::<String>());
        let specs = specs_for(target, &limited);
        assert!(!specs.iter().any(|s| s.id == "videoEncoder"));
    }

    #[test]
    fn sanitize_drops_fields_the_format_does_not_know() {
        let tune = Tune {
            video_encoder: Some("libx264".into()),
            max_height: Some(1080),
            gif_colors: Some(64),
            ..Default::default()
        };
        let cleaned = tune.sanitize(by_id("flac").unwrap(), &caps());
        assert!(cleaned.video_encoder.is_none());
        assert!(cleaned.max_height.is_none());
        assert!(cleaned.gif_colors.is_none());
    }

    #[test]
    fn sanitize_keeps_valid_fields() {
        let tune = Tune {
            video_encoder: Some("libx265".into()),
            max_height: Some(720),
            ..Default::default()
        };
        let cleaned = tune.sanitize(by_id("mp4").unwrap(), &caps());
        assert_eq!(cleaned.video_encoder.as_deref(), Some("libx265"));
        assert_eq!(cleaned.max_height, Some(720));
    }

    #[test]
    fn sanitize_rejects_encoder_not_in_choices() {
        let tune = Tune {
            video_encoder: Some("不存在的编码器".into()),
            ..Default::default()
        };
        let cleaned = tune.sanitize(by_id("mp4").unwrap(), &caps());
        assert!(cleaned.video_encoder.is_none());
    }

    #[test]
    fn sanitize_clamps_out_of_range_values() {
        let tune = Tune {
            quality: Some(200),
            gif_colors: Some(9999),
            audio_channels: Some(64),
            ..Default::default()
        };
        let cleaned = tune.sanitize(by_id("gifv").unwrap(), &caps());
        assert_eq!(cleaned.quality, None, "GIF 没有质量参数，应被清掉");
        assert_eq!(cleaned.gif_colors, Some(256));
        assert_eq!(cleaned.audio_channels, None, "GIF 没有音轨参数");
    }
}
