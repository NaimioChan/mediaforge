//! 设置持久化。
//!
//! 存成 `<app_config_dir>/settings.json`。读失败一律回退默认值，
//! 因为设置损坏不应该让程序打不开。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::args::Options;

/// 用户设置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// 输出目录；空表示「与源文件同目录」。
    ///
    /// 只在本次会话内有效：`load` 与 `save` 都不让这个值进 `settings.json`，
    /// 每次启动都回到「与源文件同目录」。记住输出目录会让用户忘记这次
    /// 到底输出到哪，写错位置比重新选一次代价大。
    /// 不能用 `serde(skip)`：那会连前端传来的值一起丢掉。
    pub output_dir: String,
    /// 与上次相同的目标格式，不覆盖已有文件时自增编号。
    pub overwrite_existing: bool,
    /// 同时执行的任务数。
    pub concurrency: u8,
    /// 参数默认值。
    pub quality: u8,
    pub audio_bitrate_kbps: u32,
    pub max_height: Option<u32>,
    pub strip_metadata: bool,
    /// 记住上次选的目录（对话框起点）。
    pub last_input_dir: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_dir: String::new(),
            overwrite_existing: false,
            concurrency: 1,
            quality: 80,
            audio_bitrate_kbps: 192,
            max_height: None,
            strip_metadata: false,
            last_input_dir: String::new(),
        }
    }
}

impl Settings {
    /// 转成参数构建器要的形态。
    ///
    /// 注意 `copy_streams` 恒为 false：直拷是任务级的意图，
    /// 由每条任务的参数面板决定，不再是一个全局开关。
    /// 全局开关曾经让「mp3 转 flac」这类正常操作全部失败。
    pub fn to_options(&self) -> Options {
        Options {
            quality: self.quality.min(100),
            audio_bitrate_kbps: Some(self.audio_bitrate_kbps),
            max_width: None,
            max_height: self.max_height,
            copy_streams: false,
            strip_metadata: self.strip_metadata,
            video_encoder_override: None,
            audio_encoder_override: None,
            gif_fps: 12,
            gif_max_width: 480,
            tune: crate::params::Tune::default(),
        }
    }

    /// 从磁盘读。文件不存在或解析失败都返回默认值。
    ///
    /// 磁盘上若有 `outputDir` 也丢弃：输出目录不跨会话记忆。
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let mut settings: Settings = serde_json::from_str(&text).unwrap_or_default();
        settings.output_dir.clear();
        settings
    }

    /// 写回磁盘。失败只记日志，不打断用户操作。
    ///
    /// 写之前把输出目录清掉，`settings.json` 里不留这个值。
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut persisted = self.clone();
        persisted.output_dir.clear();
        let text = serde_json::to_string_pretty(&persisted)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, text)
    }
}

/// 设置文件的固定位置。
pub fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join("settings.json")
}

/// 规范化手输的输出目录。
///
/// 处理三件实际会遇到的事：资源管理器「复制为路径」会带一对双引号；
/// 手打时容易多出首尾空格；结尾常带多余的分隔符。
/// 空字符串保持为空，表示「与源文件同目录」。
pub fn normalize_output_dir(raw: &str) -> String {
    let trimmed = raw.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
        })
        .unwrap_or(trimmed)
        .trim();

    let stripped = unquoted.trim_end_matches(['\\', '/']);
    if stripped.is_empty() {
        return String::new();
    }
    // `D:` 是盘符相对路径，补回分隔符才是盘符根；否则会被当成非法路径。
    if stripped.ends_with(':') {
        return format!("{stripped}\\");
    }
    stripped.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let s = Settings::default();
        assert_eq!(s.quality, 80);
        assert_eq!(s.concurrency, 1);
        assert!(s.output_dir.is_empty());
    }

    #[test]
    fn roundtrips_through_json() {
        let s = Settings {
            output_dir: "D:/out".into(),
            quality: 55,
            max_height: Some(1080),
            strip_metadata: true,
            ..Default::default()
        };
        let text = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&text).unwrap();
        // 内存里与前端交换时字段是完整的，只在落盘层清空。
        assert_eq!(back.output_dir, "D:/out");
        assert_eq!(back.quality, 55);
        assert_eq!(back.max_height, Some(1080));
        assert!(back.strip_metadata);
    }

    #[test]
    fn missing_file_yields_defaults() {
        let path = std::env::temp_dir().join("mediaforge-nonexistent-settings.json");
        let _ = std::fs::remove_file(&path);
        let s = Settings::load(&path);
        assert_eq!(s.quality, 80);
    }

    #[test]
    fn corrupt_file_yields_defaults() {
        let dir = std::env::temp_dir().join("mediaforge-settings-corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, "{ this is not json").unwrap();
        // 损坏的设置不该让程序打不开。
        let s = Settings::load(&path);
        assert_eq!(s.quality, 80);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saves_and_reloads() {
        let dir = std::env::temp_dir().join("mediaforge-settings-save");
        let _ = std::fs::remove_dir_all(&dir);
        let path = settings_path(&dir);

        let s = Settings {
            output_dir: "E:/converted".into(),
            quality: 30,
            ..Default::default()
        };
        s.save(&path).unwrap();

        let back = Settings::load(&path);
        assert!(back.output_dir.is_empty(), "输出目录不该被记住");
        assert_eq!(back.quality, 30);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn output_dir_never_reaches_disk() {
        // 输出目录只在本次会话内有效：重启后必须为空，回到「与源文件同目录」。
        let dir = std::env::temp_dir().join("mediaforge-settings-no-outputdir");
        let _ = std::fs::remove_dir_all(&dir);
        let path = settings_path(&dir);

        let s = Settings {
            output_dir: "D:/somewhere".into(),
            ..Default::default()
        };
        s.save(&path).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let on_disk: Settings = serde_json::from_str(&text).unwrap();
        assert!(on_disk.output_dir.is_empty(), "落盘值必须为空：{text}");
        assert!(!text.contains("D:/somewhere"), "路径不该写进文件：{text}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_output_dir_in_old_settings_file_is_ignored() {
        // 旧版本写下的 outputDir 字段读进来必须丢掉，不能沿用。
        let dir = std::env::temp_dir().join("mediaforge-settings-stale-outputdir");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, r#"{"outputDir": "D:/old", "quality": 42}"#).unwrap();

        let s = Settings::load(&path);
        assert!(s.output_dir.is_empty(), "旧文件里的输出目录不该被沿用");
        assert_eq!(s.quality, 42);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn normalize_strips_quotes_spaces_and_trailing_separators() {
        // 资源管理器「复制为路径」会带引号，手打常带尾部分隔符。
        assert_eq!(normalize_output_dir(r#""D:\out""#), r"D:\out");
        assert_eq!(normalize_output_dir("  D:/out/  "), "D:/out");
        assert_eq!(normalize_output_dir("D:\\out\\\\"), "D:\\out");
        assert_eq!(normalize_output_dir(""), "");
        assert_eq!(normalize_output_dir("   "), "");
        // 盘符根要补回分隔符，否则会被当成盘符相对路径。
        assert_eq!(normalize_output_dir("D:"), "D:\\");
        assert_eq!(normalize_output_dir("D:/"), "D:\\");
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let json = r#"{"quality": 42, "someFutureField": true}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.quality, 42);
    }

    #[test]
    fn options_mapping_clamps_quality() {
        let s = Settings {
            quality: 200,
            ..Default::default()
        };
        assert_eq!(s.to_options().quality, 100);
    }
}
