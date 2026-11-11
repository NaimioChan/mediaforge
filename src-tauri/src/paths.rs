//! 定位随包分发的 ffmpeg / ffprobe。
//!
//! 查找顺序：
//! 1. 环境变量 `MEDIAFORGE_FFMPEG_DIR`（测试与调试用）。
//! 2. 打包后的资源目录 `<install>/ffmpeg/`（生产）。
//! 3. 仓库内的 `vendor/ffmpeg/bin/`（`tauri dev` 与 cargo test）。
//! 4. 系统 PATH（最后兜底）。
//!
//! 前三项都不命中时返回 None，界面据此显示「找不到 ffmpeg」。

use std::path::{Path, PathBuf};

/// 一对可执行文件的位置。
#[derive(Debug, Clone)]
pub struct ToolPaths {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl ToolPaths {
    fn from_dir(dir: &Path) -> Option<Self> {
        let ffmpeg = dir.join("ffmpeg.exe");
        let ffprobe = dir.join("ffprobe.exe");
        if ffmpeg.is_file() && ffprobe.is_file() {
            Some(Self { ffmpeg, ffprobe })
        } else {
            None
        }
    }

    /// 按优先级查找。
    ///
    /// `resource_dir` 是 Tauri 的资源目录；`None` 表示当前不在 Tauri 运行期
    /// （例如 cargo test）。
    pub fn locate(resource_dir: Option<&Path>) -> Option<Self> {
        // 1. 环境变量优先，便于测试注入与用户自定义。
        if let Ok(dir) = std::env::var("MEDIAFORGE_FFMPEG_DIR")
            && let Some(paths) = Self::from_dir(Path::new(&dir)) {
                return Some(paths);
            }

        // 2. 打包后的资源目录。
        if let Some(dir) = resource_dir
            && let Some(paths) = Self::from_dir(&dir.join("ffmpeg")) {
                return Some(paths);
            }

        // 3. 仓库内的 vendor 目录。开发期与 cargo test 都走这里。
        for candidate in repo_vendor_candidates() {
            if let Some(paths) = Self::from_dir(&candidate) {
                return Some(paths);
            }
        }

        // 4. PATH 兜底。
        if let (Some(ffmpeg), Some(ffprobe)) = (which("ffmpeg.exe"), which("ffprobe.exe")) {
            return Some(Self { ffmpeg, ffprobe });
        }

        None
    }
}

/// 从可执行文件所在目录往上找 `vendor/ffmpeg/bin`。
///
/// cargo test 的可执行文件在 `src-tauri/target/debug/deps/`，
/// `tauri dev` 的在 `src-tauri/target/debug/`，两者都向上若干层能找到仓库根。
fn repo_vendor_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();

    // 相对当前工作目录：cargo test 的 cwd 是 crate 根（src-tauri）。
    out.push(PathBuf::from("../vendor/ffmpeg/bin"));
    out.push(PathBuf::from("vendor/ffmpeg/bin"));

    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(Path::to_path_buf);
        // 最多向上 6 层，避免在异常布局下无限循环。
        for _ in 0..6 {
            let Some(current) = dir else { break };
            out.push(current.join("vendor/ffmpeg/bin"));
            out.push(current.join("../vendor/ffmpeg/bin"));
            out.push(current.join("../../vendor/ffmpeg/bin"));
            dir = current.parent().map(Path::to_path_buf);
        }
    }

    out
}

/// PATH 查找。Windows 下需要补 `.exe`。
fn which(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_vendored_ffmpeg_in_repo() {
        // 仓库里 vendor/ffmpeg/bin 已就位，从 src-tauri 运行 cargo test 应能命中。
        let found = ToolPaths::locate(None);
        assert!(
            found.is_some(),
            "未找到 ffmpeg；先运行 scripts/fetch-ffmpeg.ps1"
        );
        let paths = found.unwrap();
        assert!(paths.ffmpeg.is_file());
        assert!(paths.ffprobe.is_file());
    }

    #[test]
    fn from_dir_requires_both_binaries() {
        let temp = std::env::temp_dir().join("mediaforge-paths-test");
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();

        // 只有 ffmpeg，不应被接受。
        std::fs::write(temp.join("ffmpeg.exe"), b"x").unwrap();
        assert!(ToolPaths::from_dir(&temp).is_none());

        std::fs::write(temp.join("ffprobe.exe"), b"x").unwrap();
        assert!(ToolPaths::from_dir(&temp).is_some());

        let _ = std::fs::remove_dir_all(&temp);
    }
}
