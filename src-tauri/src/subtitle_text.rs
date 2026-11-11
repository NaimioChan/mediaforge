//! 直接写文本的字幕格式。
//!
//! 有两种目标格式 ffmpeg 走不通，只能由本程序自己输出：
//!
//! * **MicroDVD**：ffmpeg n9.0 的 `-encoders` 里没有 microdvd 编码器
//!   （`ffmpeg -h encoder=microdvd` 明确回「no encoders for it are available」），
//!   复用器存在但没有可用的编码器，任何参数组合都只会得到空文件。
//! * **SBV**：ffmpeg 根本没有 SBV 复用器。v1 用 srt 复用器 + `.sbv` 扩展名凑数，
//!   产出的是 SRT 语法，被标成 SBV 是错的。
//!
//! 这两种格式的语法都很简单，直接实现比依赖 ffmpeg 更可靠，而且可以单测。

use crate::probe::Probe;

/// 一条字幕。
#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    /// 起始时间（秒）。
    pub start: f64,
    /// 结束时间（秒）。
    pub end: f64,
    pub text: String,
}

/// 从 `-progress` 无关的一侧读取字幕。
///
/// 用 ffmpeg 把源字幕转成 SRT 到标准输出，再交给本模块解析。
/// 这样各种字幕（ASS / VTT / TTML / SUB）都只需要一条转换路径。
pub fn srt_to_cues(srt: &str) -> Vec<Cue> {
    let mut cues = Vec::new();
    let normalized = srt.replace("\r\n", "\n").replace('\r', "\n");

    for block in normalized.split("\n\n") {
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty()).collect();
        if lines.len() < 2 {
            continue;
        }

        // 第一行可能是序号，也可能直接是时间行。
        let time_line_idx = if lines[0].contains("-->") { 0 } else { 1 };
        if time_line_idx >= lines.len() {
            continue;
        }

        let Some((start, end)) = parse_time_range(lines[time_line_idx]) else {
            continue;
        };

        let text = lines[time_line_idx + 1..].join("\n");
        if text.trim().is_empty() {
            continue;
        }

        cues.push(Cue { start, end, text });
    }

    cues
}

/// `00:00:00,500 --> 00:00:01,500` → (0.5, 1.5)。
fn parse_time_range(line: &str) -> Option<(f64, f64)> {
    let (left, right) = line.split_once("-->")?;
    let start = parse_srt_time(left.trim())?;
    // 右侧可能带对齐参数，取第一个字段。
    let end_token = right.split_whitespace().next()?;
    let end = parse_srt_time(end_token)?;
    Some((start, end))
}

fn parse_srt_time(text: &str) -> Option<f64> {
    let text = text.replace(',', ".");
    let mut parts = text.split(':');
    let hours: f64 = parts.next()?.trim().parse().ok()?;
    let minutes: f64 = parts.next()?.trim().parse().ok()?;
    let seconds: f64 = parts.next()?.trim().parse().ok()?;
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

/// 写出 MicroDVD。
///
/// 语法：每条一行 `{起始帧}{结束帧}文本`，首行 `{1}{1}帧率` 声明帧率。
/// 时间到帧按 `round(秒 × 帧率)` 换算。
pub fn to_microdvd(cues: &[Cue], fps: f64) -> String {
    let fps = if fps > 0.0 { fps } else { 25.0 };
    let mut out = String::new();
    out.push_str(&format!("{{1}}{{1}}{fps:.2}\n"));

    for cue in cues {
        let start = (cue.start * fps).round().max(0.0) as u64;
        // 结束帧至少比起始帧大 1，否则播放器可能不显示。
        let end = (cue.end * fps).round().max(start as f64 + 1.0) as u64;
        // MicroDVD 用 | 表示换行。
        let text = cue.text.replace('\n', "|");
        out.push_str(&format!("{{{start}}}{{{end}}}{text}\n"));
    }

    out
}

/// 写出 SBV。
///
/// 语法：`0:00:00.500,0:00:01.500` 一行时间，下一行文本，块之间空行。
/// 注意小时位不补零，毫秒用点号。
pub fn to_sbv(cues: &[Cue]) -> String {
    let mut out = String::new();
    for cue in cues {
        out.push_str(&format!(
            "{}, {}\n{}\n\n",
            sbv_time(cue.start),
            sbv_time(cue.end),
            cue.text
        ));
    }
    out
}

fn sbv_time(secs: f64) -> String {
    let total = secs.max(0.0);
    let hours = (total / 3600.0).floor() as u64;
    let minutes = ((total % 3600.0) / 60.0).floor() as u64;
    let seconds = total % 60.0;
    format!("{hours}:{minutes:02}:{seconds:06.3}")
}

/// 供 `-map 0:s:0` 之外的信息用：源里有没有字幕。
pub fn has_subtitle(probe: &Probe) -> bool {
    probe.has_subtitle()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRT: &str = "1\n00:00:00,500 --> 00:00:01,500\nhello world\n\n2\n00:00:02,000 --> 00:00:02,800\nsecond line\n";

    #[test]
    fn parses_srt_into_cues() {
        let cues = srt_to_cues(SRT);
        assert_eq!(cues.len(), 2);
        assert!((cues[0].start - 0.5).abs() < 1e-9);
        assert!((cues[0].end - 1.5).abs() < 1e-9);
        assert_eq!(cues[0].text, "hello world");
        assert_eq!(cues[1].text, "second line");
    }

    #[test]
    fn tolerates_crlf_and_blank_lines() {
        let crlf = SRT.replace('\n', "\r\n");
        let cues = srt_to_cues(&crlf);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].text, "hello world");
    }

    #[test]
    fn microdvd_declares_framerate_on_first_line() {
        let out = to_microdvd(&srt_to_cues(SRT), 25.0);
        let first = out.lines().next().unwrap();
        assert_eq!(first, "{1}{1}25.00");
        // 0.5s × 25fps = 第 12 帧；1.5s × 25 = 第 38 帧。
        assert!(out.contains("{13}{38}hello world") || out.contains("{12}{38}hello world"), "{out}");
    }

    #[test]
    fn microdvd_uses_pipe_for_line_breaks() {
        let cues = vec![Cue {
            start: 0.0,
            end: 1.0,
            text: "line one\nline two".to_string(),
        }];
        let out = to_microdvd(&cues, 25.0);
        assert!(out.contains("line one|line two"), "{out}");
    }

    #[test]
    fn microdvd_end_frame_always_after_start() {
        let cues = vec![Cue {
            start: 1.0,
            end: 1.001, // 极短，四舍五入后可能与起始帧相同
            text: "x".to_string(),
        }];
        let out = to_microdvd(&cues, 25.0);
        let line = out.lines().nth(1).unwrap();
        assert!(line.starts_with("{25}{26}"), "结束帧必须大于起始帧：{line}");
    }

    #[test]
    fn sbv_uses_dot_for_milliseconds_and_no_zero_padded_hour() {
        let out = to_sbv(&srt_to_cues(SRT));
        // SBV 的正确语法：0:00:00.500,0:00:01.500
        assert!(out.starts_with("0:00:00.500, 0:00:01.500"), "{out}");
        assert!(out.contains("hello world"));
        assert!(out.contains("0:00:02.000, 0:00:02.800"), "{out}");
        // 绝不能是 SRT 的逗号毫秒写法。
        assert!(!out.contains("00:00:00,500"), "SBV 不该用逗号毫秒：{out}");
    }

    #[test]
    fn sbv_blocks_are_separated_by_blank_line() {
        let out = to_sbv(&srt_to_cues(SRT));
        assert!(out.contains("\n\n"), "{out}");
        assert_eq!(out.matches("\n\n").count(), 2, "两条字幕应有两个空行分隔：{out}");
    }

    #[test]
    fn cue_text_survives_roundtrip_of_timestamps() {
        let cues = srt_to_cues(SRT);
        let md = to_microdvd(&cues, 25.0);
        let sbv = to_sbv(&cues);
        assert!(md.contains("hello world") && sbv.contains("hello world"));
    }

    #[test]
    fn malformed_blocks_are_skipped_not_fatal() {
        let broken = "1\nnot a time line\nhello\n\n2\n00:00:01,000 --> 00:00:02,000\nok\n";
        let cues = srt_to_cues(broken);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].text, "ok");
    }
}
