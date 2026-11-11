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
    /// 输出目录；空表示「源文件旁边的 converted 子目录」。
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
    pub copy_streams: bool,
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
            copy_streams: false,
            last_input_dir: String::new(),
        }
    }
}

impl Settings {
    /// 转成参数构建器要的形态。
    pub fn to_options(&self) -> Options {
        Options {
            quality: self.quality.min(100),
            audio_bitrate_kbps: Some(self.audio_bitrate_kbps),
            max_width: None,
            max_height: self.max_height,
            copy_streams: self.copy_streams,
            strip_metadata: self.strip_metadata,
            video_encoder_override: None,
            audio_encoder_override: None,
            gif_fps: 12,
            gif_max_width: 480,
        }
    }

    /// 从磁盘读。文件不存在或解析失败都返回默认值。
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// 写回磁盘。失败只记日志，不打断用户操作。
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, text)
    }
}

/// 设置文件的固定位置。
pub fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join("settings.json")
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
        assert_eq!(back.output_dir, "E:/converted");
        assert_eq!(back.quality, 30);

        let _ = std::fs::remove_dir_all(&dir);
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
