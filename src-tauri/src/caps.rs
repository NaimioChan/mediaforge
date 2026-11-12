//! ffmpeg 能力清单解析。
//!
//! 启动时读一次 `-encoders` 与 `-muxers`，据此过滤界面里的格式与编码器。
//!
//! 这里有个必须记住的坑：清单每行的**行首带空格**，而且开头是表头与分隔行。
//!
//! ```text
//! Encoders:
//!  V..... = Video            <- 表头，第一列是标志位、第二列是 "="
//!  ------
//!  V....D libx264   H.264 ... <- 真正的数据行，行首一个空格
//! ```
//!
//! 直接按空格分词而不先 `trim_start`，第一个字段会变成空串，第二列错位，
//! 结果是一个编码器都解析不出来（v1 踩过）。

use std::collections::BTreeSet;

use serde::Serialize;

/// 当前 ffmpeg 构建真实具备的能力。
#[derive(Debug, Clone, Default, Serialize)]
pub struct Caps {
    encoders: BTreeSet<String>,
    muxers: BTreeSet<String>,
}

impl Caps {
    /// 从两组清单文本构造。文本即 `-encoders` / `-muxers` 的原始输出。
    pub fn from_output(encoders_text: &str, muxers_text: &str) -> Self {
        Self {
            encoders: parse_names(encoders_text).into_iter().collect(),
            muxers: parse_names(muxers_text).into_iter().collect(),
        }
    }

    /// 直接给出名字构造，供测试使用。
    pub fn from_names<I, M>(encoders: I, muxers: M) -> Self
    where
        I: IntoIterator<Item = String>,
        M: IntoIterator<Item = String>,
    {
        Self {
            encoders: encoders.into_iter().map(|s| s.to_lowercase()).collect(),
            muxers: muxers.into_iter().map(|s| s.to_lowercase()).collect(),
        }
    }

    pub fn has_encoder(&self, name: &str) -> bool {
        self.encoders.contains(&name.to_lowercase())
    }

    pub fn has_muxer(&self, name: &str) -> bool {
        self.muxers.contains(&name.to_lowercase())
    }

    /// 候选链里第一个真正存在的编码器。
    pub fn resolve_encoder(&self, candidates: &[&str]) -> Option<String> {
        candidates
            .iter()
            .find(|name| self.has_encoder(name))
            .map(|name| (*name).to_string())
    }

    pub fn encoder_count(&self) -> usize {
        self.encoders.len()
    }

    pub fn muxer_count(&self) -> usize {
        self.muxers.len()
    }

    /// 供界面展示的编码器名字列表（已排序）。
    pub fn encoder_names(&self) -> Vec<String> {
        self.encoders.iter().cloned().collect()
    }
}

/// 解析 ffmpeg 清单文本里的名字列。
///
/// 规则：
/// 1. 先 `trim_start` 去掉行首空格，否则第一个字段是空串。
/// 2. 第一列必须是标志位列（长度 1–6，只含大写字母、`.` 与设备标记 `d`）。
/// 3. 第二列若含 `=` 说明是表头（`V..... = Video`），丢弃。
/// 4. `------` 这类分隔行分词后只有一个字段，自然被跳过。
pub fn parse_names(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    for raw in text.lines() {
        let line = raw.trim_end_matches('\r').trim_start();
        if line.len() < 4 {
            continue;
        }

        let mut fields = line.split_whitespace();
        let Some(flags) = fields.next() else { continue };
        let Some(name) = fields.next() else { continue };

        if !is_flag_column(flags) {
            continue;
        }

        // 表头行的第二列是 "="，必须排除。
        if name.is_empty() || name.contains('=') {
            continue;
        }

        let lowered = name.to_lowercase();
        if !out.contains(&lowered) {
            out.push(lowered);
        }
    }

    out
}

/// 标志列：1–6 位，只可能是大写字母、点，以及设备标记小写 `d`。
fn is_flag_column(text: &str) -> bool {
    if text.is_empty() || text.len() > 6 {
        return false;
    }

    text.chars().all(|c| c == '.' || c == 'd' || c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真实 `-encoders` 输出片段，含表头、分隔行与行首空格。
    const ENCODERS: &str = "\
Encoders:
 V..... = Video
 A..... = Audio
 S..... = Subtitle
 .F.... = Frame-level multithreading
 ..S... = Slice-level multithreading
 ...X.. = Codec is experimental
 ....B. = Supports draw_horiz_band
 .....D = Supports direct rendering method 1
 ------
 V....D libx264              H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10 (codec h264)
 V....D libx265              H.265 / HEVC (codec hevc)
 V....D libsvtav1            SVT-AV1 encoder (codec av1)
 A....D libmp3lame           libmp3lame MP3 (MPEG audio layer 3) (codec mp3)
 A....D aac                  AAC (Advanced Audio Coding)
 S..... subrip               SubRip subtitle
";

    const MUXERS: &str = "\
Muxers:
  E  3gp             3GP (3GPP File Format)
  E  mp4             MP4 (MPEG-4 Part 14)
  E  matroska        Matroska
  E  webm            WebM
  E  gif             CompuServe Graphics Interchange Format (GIF)
  E  ico             Microsoft Windows ICO
  E  mxf             MXF (Material eXchange Format)
";

    #[test]
    fn parses_encoder_names_despite_leading_space() {
        let names = parse_names(ENCODERS);
        assert!(names.contains(&"libx264".to_string()));
        assert!(names.contains(&"aac".to_string()));
        assert!(names.contains(&"subrip".to_string()));
        assert_eq!(names.len(), 6, "只有 6 个真实编码器：{names:?}");
    }

    #[test]
    fn skips_header_and_separator_lines() {
        let names = parse_names(ENCODERS);
        // 表头第二列是 "="，不能被当成名字。
        assert!(!names.iter().any(|n| n.contains('=')));
        assert!(!names.contains(&"video".to_string()));
        assert!(!names.contains(&"------".to_string()));
        // "Encoders:" 这一行首列不是合法标志位。
        assert!(!names.contains(&"encoders:".to_string()));
    }

    #[test]
    fn parses_muxers() {
        let names = parse_names(MUXERS);
        assert!(names.contains(&"mp4".to_string()));
        assert!(names.contains(&"matroska".to_string()));
        assert!(names.contains(&"ico".to_string()));
        assert_eq!(names.len(), 7);
    }

    #[test]
    fn empty_input_yields_nothing() {
        assert!(parse_names("").is_empty());
        assert!(parse_names("   \n\n").is_empty());
    }

    #[test]
    fn resolves_first_available_encoder() {
        let caps = Caps::from_output(ENCODERS, MUXERS);
        assert_eq!(
            caps.resolve_encoder(&["h264_nvenc", "libx264", "mpeg4"]),
            Some("libx264".to_string())
        );
        assert_eq!(caps.resolve_encoder(&["nope", "alsonope"]), None);
    }

    #[test]
    fn lookups_are_case_insensitive() {
        let caps = Caps::from_output(ENCODERS, MUXERS);
        assert!(caps.has_encoder("libx264"));
        assert!(caps.has_encoder("LIBX264"));
        assert!(caps.has_muxer("MP4"));
        assert!(!caps.has_muxer("mkv"));
    }

    #[test]
    fn flag_column_rejects_prose() {
        assert!(!is_flag_column("Encoders:"));
        assert!(!is_flag_column(""));
        assert!(!is_flag_column("Toolong"));
        assert!(is_flag_column("V....D"));
        assert!(is_flag_column("E"));
        assert!(is_flag_column("D"));
    }
}
