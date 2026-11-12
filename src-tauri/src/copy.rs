//! 「容器 × 码流」直拷兼容表。
//!
//! 为什么需要这张表：`-c copy` 不重新编码，把源码流原样塞进目标容器。
//! 但每个复用器只认有限的几种码流，塞错了 ffmpeg 会在写文件头时直接报错，
//! 用户看到的是「无法转换」，实际原因只是容器不收这种码流。
//!
//! 典型例子：mp3 音轨能进 mp3 / wav / asf，进不了 flac / opus / ac3 / ipod。
//!
//! 本表全部来自实测：用每种编码器造真实源文件，逐个复用器跑 `-c copy`，
//! 只记录产物非空且退出码为 0 的组合。表里没有的组合一律当作不支持，
//! 让调用方回退到重编码 —— 宁可多花一点 CPU，也不要产出坏文件。
//!
//! 维护方式：ffmpeg 升级后重跑 `scripts/copy-matrix.sh`，把输出贴回本文件。

/// 视频码流能否直接拷进该复用器。
pub fn video_copy_ok(muxer: &str, codec: &str) -> bool {
    let allowed: &[&str] = match muxer {
        "3gp" => &["h263", "h264", "mpeg4"],
        "dv" => &["dvvideo"],
        "flv" => &["av1", "flv1", "h264", "hevc", "vp9"],
        "h264" => &["h264"],
        "hevc" => &["hevc"],
        "ivf" => &["av1", "vp8", "vp9"],
        "mov" => &[
            "dnxhd",
            "ffv1",
            "flv1",
            "h263",
            "h264",
            "hevc",
            "mjpeg",
            "mpeg1video",
            "mpeg2video",
            "mpeg4",
            "png",
            "prores",
            "rawvideo",
            "theora",
            "wmv2",
        ],
        "mp4" => &[
            "av1",
            "ffv1",
            "h264",
            "hevc",
            "mjpeg",
            "mpeg1video",
            "mpeg2video",
            "mpeg4",
            "png",
            "vp9",
        ],
        "mxf" => &["dnxhd", "ffv1", "h264", "mpeg2video", "prores"],
        "ogg" => &["theora", "vp8"],
        "webm" => &["av1", "vp8", "vp9"],
        // 这些容器几乎什么都收，但实测仍有例外：
        // asf / nut 不认 dnxhd，nut 也不认裸 mpeg2video，
        // 所以按实测逐项列出而不是一概放宽。
        "matroska" | "avi" | "mpeg" | "mpegts" | "vob" => &[
            "av1",
            "dnxhd",
            "dvvideo",
            "ffv1",
            "flv1",
            "h263",
            "h264",
            "hevc",
            "mjpeg",
            "mpeg1video",
            "mpeg2video",
            "mpeg4",
            "png",
            "prores",
            "rawvideo",
            "theora",
            "vp8",
            "vp9",
            "wmv2",
        ],
        "asf" => &[
            "av1",
            "dvvideo",
            "ffv1",
            "flv1",
            "h263",
            "h264",
            "hevc",
            "mjpeg",
            "mpeg1video",
            "mpeg2video",
            "mpeg4",
            "png",
            "prores",
            "rawvideo",
            "theora",
            "vp8",
            "vp9",
            "wmv2",
        ],
        "nut" => &[
            "av1",
            "dvvideo",
            "ffv1",
            "flv1",
            "h263",
            "h264",
            "hevc",
            "mjpeg",
            "mpeg1video",
            "mpeg4",
            "png",
            "prores",
            "rawvideo",
            "theora",
            "vp8",
            "vp9",
            "wmv2",
        ],
        // GIF 复用器只认 gif 编码器，而 gif 编码器必须重新调色板编码，
        // 直拷必然失败，所以这里当作不支持。
        _ => &[],
    };
    allowed.contains(&codec)
}

/// 纯音频容器：音频码流能否直接拷进去。
pub fn audio_copy_ok(muxer: &str, codec: &str) -> bool {
    let allowed: &[&str] = match muxer {
        "ac3" => &["ac3"],
        "adts" => &["aac"],
        "aiff" => &["pcm_f32le", "pcm_s16le", "pcm_s24le", "wmav2"],
        "asf" => &[
            "aac",
            "ac3",
            "dca",
            "eac3",
            "flac",
            "libvorbis",
            "mp2",
            "mp3",
            "pcm_f32le",
            "pcm_s16le",
            "pcm_s24le",
            "wmav2",
        ],
        "dts" => &["dca"],
        "eac3" => &["eac3"],
        "flac" => &["flac"],
        "ipod" => &["aac", "ac3", "alac"],
        "mp3" => &["mp3"],
        "ogg" | "opus" => &["flac", "libopus", "libvorbis"],
        "tta" => &["tta"],
        "wav" => &[
            "aac",
            "ac3",
            "dca",
            "eac3",
            "flac",
            "libvorbis",
            "mp2",
            "mp3",
            "pcm_f32le",
            "pcm_s16le",
            "pcm_s24le",
            "wmav2",
        ],
        "wv" => &["wavpack"],
        // 通用容器当纯音频容器用时同样宽容（-vn 后只剩音轨）。
        "matroska" | "mpegts" => &[
            "aac",
            "ac3",
            "alac",
            "dca",
            "eac3",
            "flac",
            "libopus",
            "libvorbis",
            "mp2",
            "mp3",
            "pcm_f32le",
            "pcm_s16le",
            "pcm_s24le",
            "truehd",
            "tta",
            "wavpack",
            "wmav2",
        ],
        "mp4" => &[
            "aac",
            "ac3",
            "alac",
            "dca",
            "eac3",
            "flac",
            "libopus",
            "libvorbis",
            "mp2",
            "mp3",
            "pcm_f32le",
            "pcm_s16le",
            "pcm_s24le",
        ],
        "nut" => &[
            "aac",
            "ac3",
            "alac",
            "dca",
            "eac3",
            "flac",
            "libopus",
            "libvorbis",
            "mp2",
            "mp3",
            "pcm_f32le",
            "pcm_s16le",
            "pcm_s24le",
            "wavpack",
            "wmav2",
        ],
        _ => &[],
    };
    allowed.contains(&codec)
}

/// 音轨与视频轨同时存在时，音轨能否拷进该容器。
///
/// 与纯音频容器不同：webm / ogg 对音轨的要求更窄，
/// 而 mpeg / vob / 3gp 这类老容器只收少数几种音轨。
///
/// webm / ogg 还必须检查视频轨：这两个复用器要求音视频都在自己的白名单里，
/// 只有音轨合格、视频是 H.264 时依然会报
/// 「Only VP8 or VP9 or AV1 video and Vorbis or Opus audio are supported」。
pub fn audio_in_video_copy_ok(muxer: &str, audio_codec: &str, video_codec: Option<&str>) -> bool {
    // 这两个容器音视频必须成套，先卡视频。
    if matches!(muxer, "webm" | "ogg") {
        let Some(video) = video_codec else {
            return false;
        };
        if !video_copy_ok(muxer, video) {
            return false;
        }
    }

    let allowed: &[&str] = match muxer {
        "3gp" => &["aac"],
        "flv" => &["aac", "ac3", "eac3", "flac", "libopus", "mp3"],
        "mov" => &[
            "aac",
            "ac3",
            "alac",
            "dca",
            "eac3",
            "libvorbis",
            "mp2",
            "mp3",
            "pcm_f32le",
            "pcm_s16le",
            "pcm_s24le",
            "wmav2",
        ],
        "mp4" => &[
            "aac",
            "ac3",
            "alac",
            "dca",
            "eac3",
            "flac",
            "libopus",
            "libvorbis",
            "mp2",
            "mp3",
            "pcm_f32le",
            "pcm_s16le",
            "pcm_s24le",
        ],
        "mpeg" | "vob" => &["ac3", "dca", "mp2", "mp3"],
        "mxf" => &["pcm_s16le", "pcm_s24le"],
        "webm" => &["libopus", "libvorbis"],
        "ogg" => &["flac", "libopus", "libvorbis"],
        "matroska" | "mpegts" => &[
            "aac",
            "ac3",
            "alac",
            "dca",
            "eac3",
            "flac",
            "libopus",
            "libvorbis",
            "mp2",
            "mp3",
            "pcm_f32le",
            "pcm_s16le",
            "pcm_s24le",
            "truehd",
            "tta",
            "wavpack",
            "wmav2",
        ],
        "avi" | "asf" => &[
            "aac",
            "ac3",
            "dca",
            "eac3",
            "flac",
            "libvorbis",
            "mp2",
            "mp3",
            "pcm_f32le",
            "pcm_s16le",
            "pcm_s24le",
            "wmav2",
        ],
        "nut" => &[
            "aac",
            "ac3",
            "alac",
            "dca",
            "eac3",
            "flac",
            "libopus",
            "libvorbis",
            "mp2",
            "mp3",
            "pcm_f32le",
            "pcm_s16le",
            "pcm_s24le",
            "wavpack",
            "wmav2",
        ],
        _ => &[],
    };
    allowed.contains(&audio_codec)
}

/// 该容器能不能装字幕轨。装不下的容器要显式丢弃字幕，否则 mux 报错。
pub fn keeps_subtitles(muxer: &str) -> bool {
    matches!(
        muxer,
        "matroska" | "mp4" | "mov" | "mpegts" | "nut" | "webm" | "mxf" | "avi"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp3_copies_into_mp3_but_not_flac() {
        assert!(audio_copy_ok("mp3", "mp3"));
        assert!(audio_copy_ok("wav", "mp3"));
        assert!(audio_copy_ok("asf", "mp3"));
        // 这正是用户遇到的故障：mp3 码流塞进这些容器全部报错。
        assert!(!audio_copy_ok("flac", "mp3"));
        assert!(!audio_copy_ok("opus", "mp3"));
        assert!(!audio_copy_ok("ac3", "mp3"));
        assert!(!audio_copy_ok("ipod", "mp3"));
        assert!(!audio_copy_ok("adts", "mp3"));
        assert!(!audio_copy_ok("wv", "mp3"));
    }

    #[test]
    fn aac_copies_into_aac_containers() {
        assert!(audio_copy_ok("adts", "aac"));
        assert!(audio_copy_ok("ipod", "aac"));
        assert!(!audio_copy_ok("flac", "aac"));
        assert!(!audio_copy_ok("mp3", "aac"));
    }

    #[test]
    fn video_containers_reject_unrelated_codecs() {
        assert!(video_copy_ok("mp4", "h264"));
        assert!(video_copy_ok("mp4", "hevc"));
        assert!(!video_copy_ok("mp4", "vp8"), "mp4 不收 VP8");
        assert!(!video_copy_ok("mp4", "theora"));
        assert!(video_copy_ok("webm", "vp9"));
        assert!(!video_copy_ok("webm", "h264"), "webm 只收 VP8/VP9/AV1");
        assert!(video_copy_ok("ivf", "av1"));
        assert!(!video_copy_ok("ivf", "h264"));
    }

    #[test]
    fn gif_never_accepts_stream_copy() {
        // GIF 必须重编码成调色板图像，直拷一律失败。
        for codec in ["h264", "gif", "png", "mpeg4"] {
            assert!(!video_copy_ok("gif", codec), "gif 不该接受 {codec}");
        }
    }

    #[test]
    fn raw_codec_muxers_are_strict() {
        assert!(video_copy_ok("h264", "h264"));
        assert!(!video_copy_ok("h264", "hevc"));
        assert!(video_copy_ok("hevc", "hevc"));
        assert!(!video_copy_ok("hevc", "h264"));
    }

    #[test]
    fn audio_in_video_muxers_are_stricter_than_pure_audio_ones() {
        // 纯音频 wav 能装 vorbis，但 mp4 里的音轨不能是 vorbis？实际上能。
        assert!(audio_in_video_copy_ok("mp4", "libvorbis", Some("h264")));
        // webm 只收 opus / vorbis。
        assert!(audio_in_video_copy_ok("webm", "libopus", Some("vp9")));
        assert!(!audio_in_video_copy_ok("webm", "aac", Some("vp9")));
        assert!(!audio_in_video_copy_ok("webm", "mp3", Some("vp9")));
        // mpeg/vob 只收 mp2 / mp3 / ac3 / dts。
        assert!(audio_in_video_copy_ok("mpeg", "mp2", Some("mpeg2video")));
        assert!(!audio_in_video_copy_ok("mpeg", "aac", Some("mpeg2video")));
    }

    #[test]
    fn mxf_only_takes_pcm() {
        assert!(audio_in_video_copy_ok(
            "mxf",
            "pcm_s24le",
            Some("mpeg2video")
        ));
        assert!(!audio_in_video_copy_ok("mxf", "aac", Some("mpeg2video")));
    }

    #[test]
    fn webm_requires_matching_video_even_when_audio_fits() {
        // 实测：h264 视频 + opus 音轨塞进 webm 会报
        // 「Only VP8 or VP9 or AV1 video and Vorbis or Opus audio are supported」。
        // 音轨合格不代表能拷，视频轨也必须合格。
        assert!(audio_in_video_copy_ok("webm", "libopus", Some("vp9")));
        assert!(!audio_in_video_copy_ok("webm", "libopus", Some("h264")));
        assert!(!audio_in_video_copy_ok("webm", "libopus", None));
    }

    #[test]
    fn ogg_requires_matching_video_even_when_audio_fits() {
        assert!(audio_in_video_copy_ok("ogg", "libvorbis", Some("theora")));
        assert!(!audio_in_video_copy_ok("ogg", "libvorbis", Some("h264")));
    }

    #[test]
    fn containers_that_accept_any_video_do_not_check_the_video_side() {
        // mp4 / matroska 之类音视频互不牵连，音轨合格就够。
        assert!(audio_in_video_copy_ok("mp4", "aac", Some("h264")));
        assert!(audio_in_video_copy_ok("matroska", "aac", Some("h264")));
    }

    #[test]
    fn unknown_muxer_never_copies() {
        assert!(!video_copy_ok("不存在的容器", "h264"));
        assert!(!audio_copy_ok("不存在的容器", "mp3"));
        assert!(!audio_in_video_copy_ok("不存在的容器", "aac", Some("h264")));
    }

    #[test]
    fn subtitle_capable_containers_are_listed() {
        assert!(keeps_subtitles("matroska"));
        assert!(keeps_subtitles("mp4"));
        assert!(!keeps_subtitles("mp3"));
        assert!(!keeps_subtitles("gif"));
    }
}
