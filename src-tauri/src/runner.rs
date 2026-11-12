//! ffmpeg 子进程驱动。
//!
//! 三件事：跑进程、采集 `-progress` 输出、支持中途取消并清理残片。
//!
//! 取消的实现是 `Child::kill()`。ffmpeg 被强杀时磁盘上会留下半成品
//! （实测：2 秒素材转码中卡在第 1.3 秒，kill 后留下 48 字节的 mp4），
//! 因此取消路径必须删掉本次创建的目标文件，否则用户会看到打不开的空文件。

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::progress::Progress;

/// 一次转换的结局。
#[derive(Debug, Clone)]
pub enum Outcome {
    Done,
    Cancelled,
    Failed(String),
}

/// 进度回调。`percent` 为 0–100，`speed` 为编码倍数。
pub type ProgressFn = Arc<dyn Fn(f64, Option<f64>) + Send + Sync>;

/// 跑一次 ffmpeg。
///
/// `args` 是参数构建器给出的数组（不含可执行文件与输出路径）；
/// 输出路径由这里追加到末尾。
pub fn run_ffmpeg(
    ffmpeg: &Path,
    args: &[String],
    output_path: &Path,
    total_secs: f64,
    cancel: &Arc<AtomicBool>,
    on_progress: Option<ProgressFn>,
) -> Outcome {
    // 取消是抢占式的：进程还没起来就先检查一次。
    if cancel.load(Ordering::SeqCst) {
        return Outcome::Cancelled;
    }

    let mut command = Command::new(ffmpeg);
    command
        .args(args)
        .arg(output_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    // Windows 下不弹出控制台窗口。
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => return Outcome::Failed(format!("无法启动 ffmpeg：{e}")),
    };

    // stderr 单独线程读完，否则管道写满会让进程卡死。
    let stderr_handle = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            let mut tail: Vec<String> = Vec::new();
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim().to_string();
                if trimmed.is_empty() {
                    continue;
                }
                tail.push(trimmed);
                if tail.len() > 40 {
                    tail.remove(0);
                }
            }
            tail
        })
    });

    let mut progress = Progress::new(total_secs);
    let mut cancelled_by_user = false;

    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            // 每个检查点都看一次取消标志，响应延迟约等于 stats_period。
            if cancel.load(Ordering::SeqCst) {
                cancelled_by_user = true;
                let _ = child.kill();
                break;
            }

            if progress.feed(&line)
                && let Some(cb) = &on_progress
            {
                cb(progress.percent, progress.speed);
            }
        }
    }

    let status = match child.wait() {
        Ok(s) => s,
        Err(e) => {
            cleanup_partial(output_path);
            return Outcome::Failed(format!("等待 ffmpeg 退出失败：{e}"));
        }
    };

    let stderr_tail = stderr_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();

    if cancelled_by_user {
        cleanup_partial(output_path);
        return Outcome::Cancelled;
    }

    if status.success() {
        // 极少数情况下 ffmpeg 以 0 退出却没有写出内容。
        if output_path.is_file() {
            let size = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);
            if size == 0 {
                cleanup_partial(output_path);
                return Outcome::Failed("ffmpeg 报告成功但输出文件为空".into());
            }
            return Outcome::Done;
        }
        return Outcome::Failed("ffmpeg 报告成功但没有生成文件".into());
    }

    // 失败：删掉残片，把 stderr 的最后几行作为原因。
    cleanup_partial(output_path);
    let code = status.code().unwrap_or(-1);
    let reason = if stderr_tail.is_empty() {
        format!("ffmpeg 退出码 {code}")
    } else {
        // 只保留最后 4 行，够定位问题又不至于淹没界面。
        let tail: Vec<String> = stderr_tail.iter().rev().take(4).rev().cloned().collect();
        format!("ffmpeg 退出码 {code}：{}", tail.join(" / "))
    };
    Outcome::Failed(reason)
}

/// 删掉被中断的产物。只删存在且非空判断过的路径。
fn cleanup_partial(path: &Path) {
    if path.is_file() {
        let _ = std::fs::remove_file(path);
    }
}

/// 跑一次 ffprobe，返回 JSON 文本。
pub fn run_ffprobe(ffprobe: &Path, path: &Path) -> Result<String, String> {
    let mut command = Command::new(ffprobe);
    command
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_streams",
            "-show_format",
        ])
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command
        .output()
        .map_err(|e| format!("无法启动 ffprobe：{e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe 失败：{}", err.trim()));
    }

    String::from_utf8(output.stdout).map_err(|e| format!("ffprobe 输出不是 UTF-8：{e}"))
}

/// 跑一次 ffmpeg 并把标准输出整段读回。用于「字幕转 SRT 再自己写文本」的场景。
pub fn run_capture_stdout(ffmpeg: &Path, args: &[String]) -> Result<String, String> {
    let mut command = Command::new(ffmpeg);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command
        .output()
        .map_err(|e| format!("无法启动 ffmpeg：{e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg 失败：{}", err.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// 探测一次能力清单（encoders + muxers）。
pub fn probe_capabilities(ffmpeg: &Path) -> Result<crate::caps::Caps, String> {
    let encoders = run_flag(ffmpeg, "-encoders")?;
    let muxers = run_flag(ffmpeg, "-muxers")?;
    Ok(crate::caps::Caps::from_output(&encoders, &muxers))
}

/// 取 ffmpeg 的版本行，例如 `ffmpeg version n9.0.2 ...`。
pub fn ffmpeg_version(ffmpeg: &Path) -> Result<String, String> {
    let text = run_flag(ffmpeg, "-version")?;
    text.lines()
        .next()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .ok_or_else(|| "ffmpeg -version 没有输出".to_string())
}

fn run_flag(ffmpeg: &Path, flag: &str) -> Result<String, String> {
    let mut command = Command::new(ffmpeg);
    command
        .arg("-hide_banner")
        .arg(flag)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command
        .output()
        .map_err(|e| format!("无法启动 ffmpeg {flag}：{e}"))?;

    // -encoders 与 -muxers 都往 stdout 写。
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// 杀掉一个仍在运行的子进程（供外部强停使用）。
pub fn terminate(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ffmpeg_path() -> PathBuf {
        crate::paths::ToolPaths::locate(None)
            .expect("需要 vendor/ffmpeg")
            .ffmpeg
    }

    #[test]
    fn probe_capabilities_returns_real_encoder_list() {
        let caps = probe_capabilities(&ffmpeg_path()).unwrap();
        // 真实构建有 200 个以上编码器；解析失败会得到 0，这条能抓住行首空格的坑。
        assert!(
            caps.encoder_count() > 200,
            "只解析出 {} 个编码器",
            caps.encoder_count()
        );
        assert!(caps.has_encoder("libx264"));
        assert!(caps.has_muxer("mp4"));
    }

    #[test]
    fn missing_binary_reports_error_not_panic() {
        let bogus = PathBuf::from("C:/definitely/not/here/ffprobe.exe");
        assert!(run_ffprobe(&bogus, Path::new("x.mp4")).is_err());
    }
}
