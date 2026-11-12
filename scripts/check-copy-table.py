#!/usr/bin/env python
"""对照实测矩阵与 copy.rs，找出表里与实测不符的条目。

用法（先跑 copy-matrix.sh 生成 TSV）：

    bash scripts/copy-matrix.sh > matrix.tsv
    python scripts/check-copy-table.py matrix.tsv

两类不一致：

* 表说允许、实测失败 —— 会让用户转换报错，是严重问题，退出码非 0；
* 表没写、实测成功 —— 只是少了一次直拷机会，记为提示。

容器名在三个场景下含义不同（webm 当视频容器收 VP9，
当音轨容器只收 opus/vorbis），所以 TSV 必须带 scene 列。
"""
import collections
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
COPY_RS = ROOT / "src-tauri" / "src" / "copy.rs"

VIDEO_MUX = {
    "mp4", "matroska", "webm", "mov", "avi", "asf", "flv", "mpegts",
    "mpeg", "vob", "3gp", "ogg", "nut", "ivf", "mxf", "dv", "gif",
    "h264", "hevc",
}
AUDIO_MUX = {
    "mp3", "ipod", "adts", "flac", "wav", "aiff", "opus", "ogg",
    "asf", "ac3", "eac3", "dts", "tta", "wv",
    # 通用容器在 audio 场景里当纯音频容器用（-vn 后只剩音轨）。
    "matroska", "mp4", "mpegts", "nut",
}
IN_VIDEO_MUX = {
    "mp4", "mov", "avi", "asf", "flv", "mpegts", "mpeg", "vob",
    "3gp", "ogg", "webm", "nut", "matroska", "mxf",
}

AUDIO_CODECS = {
    "mp3", "aac", "alac", "ac3", "eac3", "dca", "flac", "libopus",
    "libvorbis", "pcm_s16le", "pcm_s24le", "pcm_f32le", "wmav2",
    "mp2", "tta", "wavpack", "truehd",
}
VIDEO_CODECS = {
    "h264", "hevc", "mpeg4", "mpeg2video", "mpeg1video", "mjpeg", "vp8",
    "vp9", "av1", "prores", "dnxhd", "dvvideo", "ffv1", "theora",
    "wmv2", "flv1", "h263", "rawvideo", "png",
}


def load_matrix(path):
    """TSV → {scene: {muxer: 允许的码流集合}}。

    容器键必须先建出来再填成员：只建「有成功组合」的容器，
    会让「所有组合都失败」的容器被误判成「脚本没测」。
    """
    table = collections.defaultdict(lambda: collections.defaultdict(set))
    with open(path, encoding="utf-8") as f:
        header = next(f).rstrip("\n").split("\t")
        if header[:4] != ["scene", "codec", "muxer", "ok"]:
            print(f"表头不对：{header}（需要 scene/codec/muxer/ok）")
            raise SystemExit(2)
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) != 4:
                continue
            scene, codec, muxer, ok = parts
            bucket = table[scene][muxer]  # 先建立键
            if ok == "1":
                bucket.add(codec)
    return table


def load_code_tables():
    """从 copy.rs 解析三个函数里的 match 分支。"""
    text = COPY_RS.read_text(encoding="utf-8")
    tables = {}
    for fn in ("video_copy_ok", "audio_copy_ok", "audio_in_video_copy_ok"):
        start = text.index(f"pub fn {fn}")
        end = text.index("\n}", start)
        body = text[start:end]

        table = collections.defaultdict(set)
        # 分支形如 "mp4" => &[...] 或 "a" | "b" | "c" => &[...]
        for match in re.finditer(
            r'((?:"[^"]+"\s*(?:\|\s*)?)+)\s*=>\s*&\[([^\]]*)\]',
            body,
        ):
            keys = re.findall(r'"([^"]+)"', match.group(1))
            codecs = re.findall(r'"([^"]+)"', match.group(2))
            for key in keys:
                table[key].update(codecs)
        tables[fn] = table
    return tables


def main():
    if len(sys.argv) < 2:
        print("用法: check-copy-table.py <matrix.tsv>")
        return 2

    measured = load_matrix(sys.argv[1])
    code = load_code_tables()

    errors = 0
    warnings = 0
    manual = 0
    unchecked = 0

    for fn, scene, muxers, codecs in (
        ("video_copy_ok", "video", VIDEO_MUX, VIDEO_CODECS),
        ("audio_copy_ok", "audio", AUDIO_MUX, AUDIO_CODECS),
        ("audio_in_video_copy_ok", "in-video", IN_VIDEO_MUX, AUDIO_CODECS),
    ):
        table = code[fn]
        seen = measured.get(scene, {})
        for muxer in sorted(muxers):
            if muxer not in seen:
                print(f"未测  {fn}: 场景 {scene} 里没有 {muxer} 的任何一行")
                unchecked += 1
                continue

            # 这些「实测失败」是测试原料本身的限制，不是容器限制：
            # * webm / ogg 的音视频必须成套，矩阵固定用 H.264 视频源；
            # * mpeg2video 从裸 mpg 抽出来缺时间戳，换个有容器的源就能拷。
            needs_matching_video = muxer in ("webm", "ogg")

            for codec in sorted(codecs):
                timestamp_sensitive = codec == "mpeg2video"
                allowed = codec in table.get(muxer, set())
                works = codec in seen[muxer]

                if allowed and not works:
                    if needs_matching_video:
                        print(
                            f"核对  {fn}: 表说 {muxer} 可直拷 {codec}，"
                            f"但矩阵用的是 H.264 视频源，需人工确认视频配套"
                        )
                        manual += 1
                    elif timestamp_sensitive:
                        print(
                            f"核对  {fn}: 表说 {muxer} 可直拷 {codec}，"
                            f"但矩阵的 mpeg2video 源是裸流缺时间戳，需人工确认"
                        )
                        manual += 1
                    else:
                        print(f"严重  {fn}: 表说 {muxer} 可直拷 {codec}，实测失败")
                        errors += 1
                elif works and not allowed:
                    print(f"提示  {fn}: 实测 {muxer} 可直拷 {codec}，表里没写")
                    warnings += 1

    print(
        f"\n严重 {errors} 项，提示 {warnings} 项，"
        f"待核对 {manual} 项，未测 {unchecked} 项"
    )
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
