//! 输出路径推导。
//!
//! 单独成模块是因为它有明确的规则，且需要在测试里断言（不覆盖源文件、
//! 同名文件不死循环）。

use std::path::{Path, PathBuf};

/// 推导输出路径。
///
/// * `output_dir` 为空时用源文件所在目录。
/// * 结果不得等于源文件路径（从 mp4 转 mp4 且同目录时）。
/// * 目标已存在且 `overwrite` 为 false 时，追加 `-1`、`-2`…
pub fn derive_output_path(
    source: &Path,
    output_dir: Option<&str>,
    ext: &str,
    overwrite: bool,
) -> PathBuf {
    let dir = match output_dir {
        Some(d) if !d.trim().is_empty() => PathBuf::from(d),
        _ => source
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
    };

    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let stem = sanitize(&stem);

    let first = dir.join(format!("{stem}{ext}"));

    // 目标就是源文件本身时，必须换名，否则会覆盖用户的原件。
    if !same_path(&first, source) && (overwrite || !first.exists()) {
        return first;
    }

    for i in 1..1000 {
        let candidate = dir.join(format!("{stem}-{i}{ext}"));
        if same_path(&candidate, source) {
            continue;
        }
        if overwrite || !candidate.exists() {
            return candidate;
        }
    }

    // 1000 个同名文件还没找到空位，就用时间戳兜底。
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    dir.join(format!("{stem}-{stamp}{ext}"))
}

/// 去掉 Windows 文件名里不允许的字符。
fn sanitize(name: &str) -> String {
    const INVALID: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    let cleaned: String = name
        .chars()
        .map(|c| if INVALID.contains(&c) || (c as u32) < 32 { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim_end_matches(['.', ' ']).to_string();
    if trimmed.is_empty() {
        "output".to_string()
    } else {
        trimmed
    }
}

/// Windows 路径比较不区分大小写。
fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn uses_source_directory_when_output_dir_empty() {
        let source = PathBuf::from("C:/media/clip.mp4");
        let out = derive_output_path(&source, None, ".mkv", false);
        assert_eq!(out, PathBuf::from("C:/media/clip.mkv"));
    }

    #[test]
    fn honours_explicit_output_dir() {
        let source = PathBuf::from("C:/media/clip.mp4");
        let out = derive_output_path(&source, Some("D:/converted"), ".webm", false);
        assert_eq!(out, PathBuf::from("D:/converted/clip.webm"));
    }

    #[test]
    fn never_overwrites_the_source_file() {
        // mp4 → mp4 且输出目录就是源目录，必须换名而不是覆盖原件。
        let dir = std::env::temp_dir().join("mediaforge-outpath-same");
        let _ = std::fs::remove_dir_all(&dir);
        let source = dir.join("clip.mp4");
        touch(&source);

        let out = derive_output_path(&source, Some(dir.to_str().unwrap()), ".mp4", false);
        assert_ne!(
            out.to_string_lossy().to_lowercase(),
            source.to_string_lossy().to_lowercase()
        );
        assert!(out.to_string_lossy().ends_with("clip-1.mp4"), "{out:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn appends_counter_for_existing_files() {
        let dir = std::env::temp_dir().join("mediaforge-outpath-counter");
        let _ = std::fs::remove_dir_all(&dir);
        touch(&dir.join("a.mkv"));
        touch(&dir.join("a-1.mkv"));

        let source = dir.join("a.mp4");
        touch(&source);

        let out = derive_output_path(&source, Some(dir.to_str().unwrap()), ".mkv", false);
        assert!(out.to_string_lossy().ends_with("a-2.mkv"), "{out:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn overwrite_mode_reuses_the_same_name() {
        let dir = std::env::temp_dir().join("mediaforge-outpath-overwrite");
        let _ = std::fs::remove_dir_all(&dir);
        touch(&dir.join("a.mkv"));
        let source = dir.join("a.mp4");
        touch(&source);

        let out = derive_output_path(&source, Some(dir.to_str().unwrap()), ".mkv", true);
        assert!(out.to_string_lossy().ends_with("a.mkv"));
        assert!(!out.to_string_lossy().contains("-1"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sanitizes_invalid_characters() {
        let source = PathBuf::from("C:/media/we:ird*name?.mp4");
        let out = derive_output_path(&source, Some("D:/out"), ".mkv", false);
        let name = out.file_name().unwrap().to_string_lossy().to_string();
        assert!(!name.contains(':'));
        assert!(!name.contains('*'));
        assert!(!name.contains('?'));
    }
}
