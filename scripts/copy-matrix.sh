# 容器 × 码流直拷矩阵实测
#
# 用途：`src-tauri/src/copy.rs` 里的兼容表必须来自实测，不能凭印象写。
# ffmpeg 升级后重跑本脚本，把结果贴回 copy.rs。
#
# 用法（git-bash）：
#   bash scripts/copy-matrix.sh
#
# 输出：TSV，每行「码流 / 容器 / 是否允许 -c copy」。
# 只记录产物非空且退出码为 0 的组合。

set -u

FF="${FF:-vendor/ffmpeg/bin/ffmpeg.exe}"
W="${TMPDIR:-/tmp}/mf-copy-matrix"

if [ ! -f "$FF" ]; then
  echo "找不到 ffmpeg：$FF（先跑 scripts/fetch-ffmpeg.ps1）" >&2
  exit 1
fi
# 先解析成绝对路径，后面要 cd 到临时目录，相对路径会失效。
FF="$(cd "$(dirname "$FF")" && pwd)/$(basename "$FF")"

rm -rf "$W"; mkdir -p "$W"; cd "$W" || exit 1

echo "== 造素材" >&2
$FF -hide_banner -loglevel error -y -f lavfi -i "sine=frequency=440:duration=1" \
    -ac 2 -ar 48000 base.wav

# 音频源：每种编码器一个真实文件。
# 必须用真实容器，不能经 nut 中转 —— nut 会丢 extradata，
# 造成 prores / dnxhd / eac3 之类被误判为不支持。
build_audio() {
  case $1 in
    mp3)      $FF -hide_banner -loglevel error -y -i base.wav -c:a libmp3lame -b:a 192k s.mp3 ;;
    aac)      $FF -hide_banner -loglevel error -y -i base.wav -c:a aac -b:a 192k s.m4a ;;
    alac)     $FF -hide_banner -loglevel error -y -i base.wav -c:a alac s_alac.m4a ;;
    ac3)      $FF -hide_banner -loglevel error -y -i base.wav -c:a ac3 -b:a 192k s.ac3 ;;
    eac3)     $FF -hide_banner -loglevel error -y -i base.wav -c:a eac3 -b:a 192k s.eac3 ;;
    dca)      $FF -hide_banner -loglevel error -y -i base.wav -c:a dca -strict -2 -b:a 768k s.dts ;;
    flac)     $FF -hide_banner -loglevel error -y -i base.wav -c:a flac s.flac ;;
    libopus)  $FF -hide_banner -loglevel error -y -i base.wav -c:a libopus -b:a 96k s.opus ;;
    libvorbis)$FF -hide_banner -loglevel error -y -i base.wav -c:a libvorbis -q:a 5 s.ogg ;;
    pcm_s16le)$FF -hide_banner -loglevel error -y -i base.wav -c:a pcm_s16le s16.wav ;;
    pcm_s24le)$FF -hide_banner -loglevel error -y -i base.wav -c:a pcm_s24le s24.wav ;;
    pcm_f32le)$FF -hide_banner -loglevel error -y -i base.wav -c:a pcm_f32le s32f.wav ;;
    wmav2)    $FF -hide_banner -loglevel error -y -i base.wav -c:a wmav2 -b:a 192k s.wma ;;
    mp2)      $FF -hide_banner -loglevel error -y -i base.wav -c:a mp2 -b:a 192k s.mp2 ;;
    tta)      $FF -hide_banner -loglevel error -y -i base.wav -c:a tta s.tta ;;
    wavpack)  $FF -hide_banner -loglevel error -y -i base.wav -c:a wavpack s.wv ;;
    truehd)   $FF -hide_banner -loglevel error -y -i base.wav -c:a truehd -strict -2 s.thd.mkv ;;
  esac
}

# 视频源：同上，每种编码器一个真实容器文件。
build_video() {
  local src="testsrc2=s=320x240:d=1:r=25"
  case $1 in
    h264)       $FF -hide_banner -loglevel error -y -f lavfi -i "$src" -c:v libx264 -pix_fmt yuv420p v.mp4 ;;
    hevc)       $FF -hide_banner -loglevel error -y -f lavfi -i "$src" -c:v libx265 -pix_fmt yuv420p v_h265.mp4 ;;
    mpeg4)      $FF -hide_banner -loglevel error -y -f lavfi -i "$src" -c:v mpeg4 v_mpeg4.avi ;;
    mpeg2video) $FF -hide_banner -loglevel error -y -f lavfi -i "$src" -c:v mpeg2video v_m2v.mpg ;;
    mpeg1video) $FF -hide_banner -loglevel error -y -f lavfi -i "$src" -c:v mpeg1video v_m1v.mpg ;;
    mjpeg)      $FF -hide_banner -loglevel error -y -f lavfi -i "$src" -c:v mjpeg -pix_fmt yuvj420p v_mjpeg.avi ;;
    vp8)        $FF -hide_banner -loglevel error -y -f lavfi -i "$src" -c:v libvpx -b:v 200k v_vp8.webm ;;
    vp9)        $FF -hide_banner -loglevel error -y -f lavfi -i "$src" -c:v libvpx-vp9 -b:v 200k v_vp9.webm ;;
    av1)        $FF -hide_banner -loglevel error -y -f lavfi -i "$src" -c:v libsvtav1 -b:v 200k v_av1.mkv ;;
    prores)     $FF -hide_banner -loglevel error -y -f lavfi -i "$src" -c:v prores -profile:v 3 v_prores.mov ;;
    dnxhd)      $FF -hide_banner -loglevel error -y -f lavfi -i "testsrc2=s=1920x1080:d=1:r=25" \
                  -c:v dnxhd -b:v 175M -pix_fmt yuv422p10le v_dnxhd.mxf ;;
    dvvideo)    $FF -hide_banner -loglevel error -y -f lavfi -i "$src" \
                  -c:v dvvideo -s 720x576 -pix_fmt yuv420p -r 25 v_dv.dv ;;
    ffv1)       $FF -hide_banner -loglevel error -y -f lavfi -i "$src" -c:v ffv1 v_ffv1.mkv ;;
    theora)     $FF -hide_banner -loglevel error -y -f lavfi -i "$src" -c:v libtheora -b:v 200k v_theora.ogv ;;
    wmv2)       $FF -hide_banner -loglevel error -y -f lavfi -i "$src" -c:v wmv2 v_wmv2.wmv ;;
    flv1)       $FF -hide_banner -loglevel error -y -f lavfi -i "$src" -c:v flv1 v_flv1.flv ;;
    h263)       $FF -hide_banner -loglevel error -y -f lavfi -i "$src" -c:v h263 -s 352x288 -pix_fmt yuv420p v_h263.3gp ;;
    rawvideo)   $FF -hide_banner -loglevel error -y -f lavfi -i "$src" -c:v rawvideo -pix_fmt yuv420p v_raw.nut ;;
    png)        $FF -hide_banner -loglevel error -y -f lavfi -i "$src" -c:v png v_png.mkv ;;
  esac
}

AUDIO_CODECS="mp3 aac alac ac3 eac3 dca flac libopus libvorbis pcm_s16le pcm_s24le pcm_f32le wmav2 mp2 tta wavpack truehd"
VIDEO_CODECS="h264 hevc mpeg4 mpeg2video mpeg1video mjpeg vp8 vp9 av1 prores dnxhd dvvideo ffv1 theora wmv2 flv1 h263 rawvideo png"
AUDIO_MUX="mp3 ipod adts flac wav aiff opus ogg asf ac3 eac3 dts tta wv matroska mp4 mpegts nut"
VIDEO_MUX="mp4 matroska webm mov avi asf flv mpegts mpeg vob 3gp ogg nut ivf mxf dv gif h264 hevc"
AUDIO_IN_VIDEO_MUX="mp4 mov avi asf flv mpegts mpeg vob 3gp ogg webm nut matroska mxf"

for c in $AUDIO_CODECS; do build_audio "$c" 2>/dev/null || echo "缺素材 a:$c" >&2; done
for c in $VIDEO_CODECS; do build_video "$c" 2>/dev/null || echo "缺素材 v:$c" >&2; done

audio_file() {
  case $1 in
    mp3) echo s.mp3 ;; aac) echo s.m4a ;; alac) echo s_alac.m4a ;;
    ac3) echo s.ac3 ;; eac3) echo s.eac3 ;; dca) echo s.dts ;;
    flac) echo s.flac ;; libopus) echo s.opus ;; libvorbis) echo s.ogg ;;
    pcm_s16le) echo s16.wav ;; pcm_s24le) echo s24.wav ;; pcm_f32le) echo s32f.wav ;;
    wmav2) echo s.wma ;; mp2) echo s.mp2 ;; tta) echo s.tta ;;
    wavpack) echo s.wv ;; truehd) echo s.thd.mkv ;;
  esac
}
video_file() {
  case $1 in
    h264) echo v.mp4 ;; hevc) echo v_h265.mp4 ;; mpeg4) echo v_mpeg4.avi ;;
    mpeg2video) echo v_m2v.mpg ;; mpeg1video) echo v_m1v.mpg ;; mjpeg) echo v_mjpeg.avi ;;
    vp8) echo v_vp8.webm ;; vp9) echo v_vp9.webm ;; av1) echo v_av1.mkv ;;
    prores) echo v_prores.mov ;; dnxhd) echo v_dnxhd.mxf ;; dvvideo) echo v_dv.dv ;;
    ffv1) echo v_ffv1.mkv ;; theora) echo v_theora.ogv ;; wmv2) echo v_wmv2.wmv ;;
    flv1) echo v_flv1.flv ;; h263) echo v_h263.3gp ;; rawvideo) echo v_raw.nut ;;
    png) echo v_png.mkv ;;
  esac
}

# scene 列不可省：同一个容器名在三个场景下含义不同
# （webm 当视频容器收 VP9，当音轨容器只收 opus/vorbis），
# 混在一张表里会互相覆盖，校验时得出错误结论。
echo -e "scene\tcodec\tmuxer\tok"
for c in $VIDEO_CODECS; do
  f=$(video_file "$c"); [ -f "$f" ] || continue
  for m in $VIDEO_MUX; do
    rm -f o.out
    if $FF -hide_banner -loglevel error -y -i "$f" -map 0:v:0 -c:v copy -f "$m" o.out 2>/dev/null && [ -s o.out ]
    then echo -e "video\t$c\t$m\t1"; else echo -e "video\t$c\t$m\t0"; fi
  done
done
for c in $AUDIO_CODECS; do
  f=$(audio_file "$c"); [ -f "$f" ] || continue
  for m in $AUDIO_MUX; do
    rm -f o.out
    if $FF -hide_banner -loglevel error -y -i "$f" -vn -map 0:a:0 -c:a copy -f "$m" o.out 2>/dev/null && [ -s o.out ]
    then echo -e "audio\t$c\t$m\t1"; else echo -e "audio\t$c\t$m\t0"; fi
  done
done
for c in $AUDIO_CODECS; do
  f=$(audio_file "$c"); [ -f "$f" ] || continue
  [ -f v.mp4 ] || continue
  for m in $AUDIO_IN_VIDEO_MUX; do
    rm -f o.out
    if $FF -hide_banner -loglevel error -y -i v.mp4 -i "$f" -map 0:v:0 -map 1:a:0 \
         -c copy -f "$m" o.out 2>/dev/null && [ -s o.out ]
    then echo -e "in-video\t$c\t$m\t1"; else echo -e "in-video\t$c\t$m\t0"; fi
  done
done
