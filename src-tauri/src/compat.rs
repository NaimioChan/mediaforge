//! 跨类型转换的合法矩阵。
//!
//! 有些跨类型转换确实有用（视频抽音频、视频截帧），
//! 但音频转视频、字幕转图像只会产出空壳文件，必须在建任务时就拒绝，
//! 而不是让 ffmpeg 写一个 10 KB 的假文件出来。

use serde::Serialize;

use crate::formats::Kind;

/// 这次「源类型 → 目标类型」是否有意义。
pub fn is_supported(source: Kind, target: Kind) -> bool {
    match (source, target) {
        // 同类互转永远成立。
        (a, b) if a == b => true,

        // 视频可以抽音频轨，也可以截一帧出来。
        (Kind::Video, Kind::Audio | Kind::Image) => true,

        // 其余一律拒绝：音频不产视频、图像不产音频、字幕只能转字幕。
        _ => false,
    }
}

/// 被拒绝时给用户看的原因。
pub fn explain(source: Kind, target: Kind) -> &'static str {
    match (source, target) {
        (Kind::Audio, Kind::Video) => "音频无法转成视频",
        (Kind::Audio, Kind::Image) => "音频无法转成图像",
        (Kind::Audio, Kind::Subtitle) => "音频无法转成字幕",
        (Kind::Image, Kind::Video) => "静态图像无法转成视频；如需动图请选 GIF",
        (Kind::Image, Kind::Audio) => "图像没有音轨",
        (Kind::Image, Kind::Subtitle) => "图像无法转成字幕",
        (Kind::Subtitle, Kind::Video) => "字幕文件无法转成视频",
        (Kind::Subtitle, Kind::Audio) => "字幕文件没有音轨",
        (Kind::Subtitle, Kind::Image) => "字幕文件无法转成图像",
        (Kind::Video, Kind::Subtitle) => "视频无法转成字幕；如需字幕请先抽取字幕轨",
        _ => "",
    }
}

/// 一对源/目标类型的判定结果，直接序列化给界面。
#[derive(Debug, Clone, Serialize)]
pub struct Verdict {
    pub ok: bool,
    pub reason: &'static str,
}

pub fn judge(source: Kind, target: Kind) -> Verdict {
    let ok = is_supported(source, target);
    Verdict {
        ok,
        reason: if ok { "" } else { explain(source, target) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Kind; 4] = [Kind::Video, Kind::Audio, Kind::Image, Kind::Subtitle];

    #[test]
    fn same_kind_always_allowed() {
        for kind in ALL {
            assert!(is_supported(kind, kind), "{kind:?} 同类互转应允许");
        }
    }

    #[test]
    fn video_can_extract_audio_and_frame() {
        assert!(is_supported(Kind::Video, Kind::Audio));
        assert!(is_supported(Kind::Video, Kind::Image));
    }

    #[test]
    fn audio_cannot_become_video_or_image_or_subtitle() {
        assert!(!is_supported(Kind::Audio, Kind::Video));
        assert!(!is_supported(Kind::Audio, Kind::Image));
        assert!(!is_supported(Kind::Audio, Kind::Subtitle));
    }

    #[test]
    fn image_cannot_become_audio_video_subtitle() {
        assert!(!is_supported(Kind::Image, Kind::Audio));
        assert!(!is_supported(Kind::Image, Kind::Video));
        assert!(!is_supported(Kind::Image, Kind::Subtitle));
    }

    #[test]
    fn subtitle_only_converts_to_subtitle() {
        assert!(!is_supported(Kind::Subtitle, Kind::Video));
        assert!(!is_supported(Kind::Subtitle, Kind::Audio));
        assert!(!is_supported(Kind::Subtitle, Kind::Image));
    }

    #[test]
    fn full_matrix_has_exactly_ten_rejections() {
        let rejected: Vec<_> = ALL
            .iter()
            .flat_map(|s| ALL.iter().map(move |t| (*s, *t)))
            .filter(|(s, t)| !is_supported(*s, *t))
            .collect();
        // 16 组里允许 6 组：4 个同类 + 视频→音频 + 视频→图像。
        // 被拒 10 组，其中视频→字幕也在内（视频里的字幕轨要单独抽，不做格式转换）。
        assert_eq!(rejected.len(), 10, "应只有 10 组被拒：{rejected:?}");
    }

    #[test]
    fn every_rejection_has_a_readable_reason() {
        for s in ALL {
            for t in ALL {
                if !is_supported(s, t) {
                    let reason = explain(s, t);
                    assert!(!reason.is_empty(), "{s:?} → {t:?} 缺原因");
                }
            }
        }
    }
}
