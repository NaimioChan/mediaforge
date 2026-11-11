//! `-progress pipe:1` 输出解析。
//!
//! ffmpeg 的进度是 `key=value` 行，由 `-stats_period` 控制刷新间隔。
//!
//! 两个必须记住的点：
//!
//! 1. `out_time_ms` 的单位其实是**微秒**，历史遗留命名，别当毫秒算。
//! 2. 转码刚开始时会输出 `out_time_us=N/A`。Rust 里 `"N/A".parse::<i64>()`
//!    返回 `Err`，若把这个 `Err` 当异常向上抛，任务会误判为「开始 0.3 秒就失败」。
//!    正确做法是把该行视作「本次没有进度信息」并跳过。

/// 一次任务在运行期的进度状态。
#[derive(Debug, Clone, Default)]
pub struct Progress {
    total_secs: f64,
    /// 最近一次有效进度，0–100。
    pub percent: f64,
    /// 已处理的媒体时长（秒）。
    pub processed_secs: f64,
    /// 编码速度倍数，例如 `1.5` 表示 1.5 倍速。
    pub speed: Option<f64>,
    pub frames: u64,
    /// 收到 `progress=end`。
    pub finished: bool,
}

impl Progress {
    pub fn new(total_secs: f64) -> Self {
        Self {
            total_secs: if total_secs > 0.0 { total_secs } else { 0.0 },
            ..Default::default()
        }
    }

    /// 喂一行原始输出。返回 `true` 表示有可展示的变化。
    pub fn feed(&mut self, line: &str) -> bool {
        let line = line.trim();
        if line.is_empty() {
            return false;
        }

        let Some((key, value)) = line.split_once('=') else {
            return false;
        };
        let key = key.trim();
        let value = value.trim();

        match key {
            // out_time_ms 的单位同样是微秒，不要被名字骗了。
            "out_time_us" | "out_time_ms" => {
                // "N/A" 是合法输出，表示本次还没有有效时间。
                let Ok(micros) = value.parse::<i64>() else {
                    return false;
                };
                if micros < 0 {
                    return false;
                }
                self.processed_secs = micros as f64 / 1_000_000.0;
                if self.total_secs > 0.0 {
                    let pct = (self.processed_secs / self.total_secs * 100.0).clamp(0.0, 100.0);
                    let changed = (pct - self.percent).abs() > 0.05;
                    self.percent = pct;
                    return changed;
                }
                true
            }

            "out_time" => {
                // 形如 00:00:03.000000
                let Some(secs) = parse_clock(value) else {
                    return false;
                };
                self.processed_secs = secs;
                if self.total_secs > 0.0 {
                    let pct = (secs / self.total_secs * 100.0).clamp(0.0, 100.0);
                    let changed = (pct - self.percent).abs() > 0.05;
                    self.percent = pct;
                    return changed;
                }
                false
            }

            "speed" => {
                let trimmed = value.trim_end_matches(['x', ' ']);
                let Ok(speed) = trimmed.parse::<f64>() else {
                    return false;
                };
                self.speed = Some(speed);
                true
            }

            "frame" => {
                if let Ok(frames) = value.parse::<u64>() {
                    self.frames = frames;
                }
                false
            }

            "progress" => {
                if value == "end" {
                    self.finished = true;
                    self.percent = 100.0;
                    return true;
                }
                false
            }

            _ => false,
        }
    }
}

/// 解析 `HH:MM:SS.ffffff`。
fn parse_clock(text: &str) -> Option<f64> {
    let mut parts = text.split(':');
    let hours: f64 = parts.next()?.parse().ok()?;
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_time_ms_is_actually_microseconds() {
        let mut p = Progress::new(10.0);
        p.feed("out_time_ms=3000000");
        // 3_000_000 微秒 = 3 秒；若按毫秒算会得到 50 分钟。
        assert!((p.processed_secs - 3.0).abs() < 1e-9);
        assert!((p.percent - 30.0).abs() < 1e-6);
    }

    #[test]
    fn na_does_not_abort_progress() {
        // 真实场景：转码刚开始输出 N/A，之后才给出数字。
        let mut p = Progress::new(10.0);
        assert!(!p.feed("out_time_us=N/A"));
        assert!(!p.feed("out_time_ms=N/A"));
        assert_eq!(p.percent, 0.0);
        assert!(p.feed("out_time_us=1500000"));
        assert!((p.percent - 15.0).abs() < 1e-6);
    }

    #[test]
    fn speed_strips_trailing_x() {
        let mut p = Progress::new(10.0);
        assert!(p.feed("speed=1.52x"));
        assert_eq!(p.speed, Some(1.52));
        assert!(!p.feed("speed=N/A"));
        assert_eq!(p.speed, Some(1.52));
    }

    #[test]
    fn end_marks_finished_at_100() {
        let mut p = Progress::new(10.0);
        p.feed("out_time_us=9000000");
        assert!(p.feed("progress=end"));
        assert!(p.finished);
        assert_eq!(p.percent, 100.0);
    }

    #[test]
    fn percent_never_exceeds_100() {
        // 音频时长远超视频时长时会算出大于 100 的值。
        let mut p = Progress::new(5.0);
        p.feed("out_time_us=9000000");
        assert_eq!(p.percent, 100.0);
    }

    #[test]
    fn unknown_duration_keeps_percent_at_zero() {
        let mut p = Progress::new(0.0);
        p.feed("out_time_us=3000000");
        assert_eq!(p.percent, 0.0);
        assert!((p.processed_secs - 3.0).abs() < 1e-9);
    }

    #[test]
    fn parses_clock_format() {
        let mut p = Progress::new(10.0);
        p.feed("out_time=00:00:07.500000");
        assert!((p.processed_secs - 7.5).abs() < 1e-9);
        assert!((p.percent - 75.0).abs() < 1e-6);
    }

    #[test]
    fn ignores_non_kv_lines() {
        let mut p = Progress::new(10.0);
        assert!(!p.feed(""));
        assert!(!p.feed("frame=  120"));
        assert!(!p.feed("frame=120"));
        assert_eq!(p.frames, 120);
    }
}
