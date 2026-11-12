# 第三方组件与许可证

本项目源码以 MIT 许可发布（见仓库根目录 `LICENSE.txt`）。
安装包内分发了一个 FFmpeg 可执行文件，它有自己的许可证，与 MIT 不同。

## FFmpeg

| 项目 | 内容 |
|---|---|
| 版本 | n9.0.2-48-gd4070a14ce |
| 构建 | FFmpeg-Builds 的 `autobuild-2026-11-10-12-39` |
| 变体 | win64-gpl-shared-9.0（GPL，动态链接） |
| 许可证 | GNU General Public License v3.0 |
| 许可证全文 | 安装目录下 `ffmpeg/LICENSE.txt` |
| 上游源码 | https://github.com/FFmpeg/FFmpeg/tree/gd4070a14ce |
| 构建脚本 | https://github.com/FFmpeg-Builds/FFmpeg-Builds/tree/master |
| 构建配置 | 见下 |

构建时的 `--enable-gpl --enable-version3` 及关键外部库：

```
--enable-gpl --enable-version3
--enable-libx264 --enable-libx265 --enable-libxvid
--enable-libvpx --enable-libaom --enable-libsvtav1 --enable-libdav1d
--enable-libmp3lame --enable-libopus --enable-libvorbis --enable-libtheora
--enable-libass --enable-libfreetype --enable-libfontconfig
```

完整的 `configuration:` 行由 `ffmpeg -version` 输出，随安装包一并提供。

## 与 GPL 的关系

本项目**不链接** FFmpeg 的任何库（libavcodec、libavformat 等均未参与链接），
只通过 `std::process::Command` 以独立进程方式调用 `ffmpeg.exe` 与 `ffprobe.exe`，
通过命令行参数传入、通过标准输出与文件读取结果。因此本项目的源代码
不构成 FFmpeg 的衍生作品，可以独立以 MIT 许可发布。

分发的 FFmpeg 二进制本身仍受 GPL v3 约束。相应地，本项目提供：

1. 完整的 GPL v3 许可证文本（`ffmpeg/LICENSE.txt`，随安装包分发）；
2. 上述精确到 commit 的上游源码地址与构建脚本地址，任何收到本安装包的人
   都可以据此重建出完全相同的 ffmpeg 二进制；
3. 该二进制未经修改，直接取自上游构建产物。

## 关于 GPL 与 MIT 并存的常见疑问

「分发 GPL 程序是否要求整个应用 GPL」取决于是否构成衍生作品。以独立进程调用
命令行工具的聚合（aggregation）在 FFmpeg 项目自身与业界通行理解中不构成衍生，
这也是本项目采用的方式。若将来改为链接 libav* 库，整个项目就必须改为 GPL 兼容许可。
