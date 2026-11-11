# MediaForge v2

Windows 桌面媒体格式转换器。ffmpeg 的可视化外壳，界面用 Fluent 2，外壳用 Tauri。

支持视频、音频、图像、字幕四类共 71 种目标格式。随包分发 ffmpeg，装完即可用，不需要另外安装任何东西。

## 从源码构建

前置：

- Rust 1.80+（MSVC 工具链）
- Node.js 20+
- Windows 11（WebView2 自带）或装了 WebView2 的 Windows 10

```bash
# 1. 取 ffmpeg（9 个文件，约 181 MB）
powershell -ExecutionPolicy Bypass -File scripts/fetch-ffmpeg.ps1

# 2. 装前端依赖
npm install

# 3. 开发运行
npm run tauri dev

# 4. 打包安装程序
npm run tauri build
```

产物：`src-tauri/target/release/bundle/nsis/MediaForge_2.0.0_x64-setup.exe`

`fetch-ffmpeg.ps1` 优先从本地已有的 ffmpeg 复制；找不到时按官方地址下载解压。
`vendor/` 不进版本库（181 MB），必须靠脚本准备。

## 实测体积

| 组成 | 大小 |
|---|---|
| release exe | 3.69 MB |
| 前端产物 | 231 KB（gzip 58 KB） |
| ffmpeg 9 个文件 | 180.6 MB |
| 安装包 | 53.4 MiB |
| 安装后占用 | 185 MB |

安装包与占用都落在预期区间（预期安装包约 45 MB、装完约 185 MB）。
exe 比基线 2.95 MB 多 0.74 MB，来自 dialog / window-state / opener 三个插件。

## 已验证

装 → 跑 → 转 → 卸全流程在本机执行过：

- 静默安装退出码 0，装到 `%LOCALAPPDATA%\MediaForge`，9 个 ffmpeg 文件齐全
- 启动后探测到 ffmpeg n9.0.2、233 个编码器、184 个复用器、71/71 格式可用
- mp4 → webm 转换成功，产物回读为 VP9 + Opus、854×480、3.008 秒
- 静默卸载退出码 0，安装目录清空、注册项移除、开始菜单无残留
- 设置保留在 `%APPDATA%\com.naimio.mediaforge\settings.json`（有意保留）

## 跑测试

```bash
# 全部测试（96 个纯单测 + 11 个真实 ffmpeg 端到端）
cargo test --manifest-path src-tauri/Cargo.toml

# 只跑纯单测（不碰进程，约 0.2 秒）
cargo test --manifest-path src-tauri/Cargo.toml --lib

# 只跑端到端（会真的执行转换，约 3 秒）
cargo test --manifest-path src-tauri/Cargo.toml --test e2e
```

端到端测试不允许跳过：找不到 ffmpeg 直接失败，不会静默通过。

## 项目结构

```
src-tauri/src/
  formats.rs        71 个格式定义（纯数据）
  args.rs           参数构建（纯函数，不碰进程与文件系统）
  compat.rs         跨类型兼容矩阵（纯函数）
  progress.rs       -progress 解析（纯函数）
  caps.rs           能力清单解析（纯函数）
  probe.rs          ffprobe 输出解析（纯函数）
  subtitle_text.rs  MicroDVD / SBV 写入器（纯函数）
  output_path.rs    输出路径推导（纯函数）
  paths.rs          定位随包 ffmpeg
  settings.rs       设置持久化
  runner.rs         子进程驱动、进度采集、取消
  lib.rs            命令、状态、事件（装配层）
src-tauri/tests/e2e.rs   11 个真实转换测试
src/                前端（Vanilla TS + Fluent 2）
  store.ts          单一状态容器
  api.ts            invoke / listen 封装
  fluent.ts         组件注册与主题
  views/            任务列表
```

## 设计要点

**后端持有队列状态。** 前端只是视图，命令返回快照，进度走事件推送。
ffmpeg 是子进程，前端刷新或重载不该丢掉正在跑的任务。

**参数构建是纯函数。** `args::build_plan(源路径, 输出路径, 目标格式, 选项, 探测结果, 能力清单)`
只产出参数数组，不启动进程、不读写文件。有一条测试读源码断言这一点，
因此 71 种格式的每种组合都能在单测里断言出完整参数表。

**能力探测决定可用性。** 启动时读 `-encoders` 与 `-muxers`，界面只呈现真实存在的编码器和复用器。
缺能力的格式标为不可用并给出原因，而不是让用户点了开始才失败。

**跨类型转换有明确边界。** 视频可以抽音频、可以截帧；音频不能变视频，字幕只能转字幕。
无意义组合在建任务时就标为不兼容。

**取消会清理残片。** ffmpeg 被强杀时磁盘上会留半成品（实测 2 秒素材取消后剩 48 字节）。
取消路径会删掉本次创建的目标文件。

## 与常见写法的差异

几处容易踩的地方，实现里按实际情况处理：

| 事项 | 常见写法 | 本项目 |
|---|---|---|
| Fluent 主题对象 | 从 `@fluentui/web-components` 导入 `webLightTheme` | 该包只导出 `setTheme`；主题对象来自 `@fluentui/tokens` |
| Fluent 组件注册 | `@fluentui/web-components/button.js`（README 写法） | exports 只有一级通配指向 `define.js`，必须写 `button/define.js` |
| MicroDVD | 交给 ffmpeg | 当前 ffmpeg 构建没有该编码器，由本程序直接写文本 |
| SBV | 用 srt 复用器 + `.sbv` 扩展名 | ffmpeg 没有 SBV 复用器；本程序按 SBV 语法自己写 |
| ICO | `-frames:v 1 -update 1` | 还必须缩放进 256×256 并 `format=rgba`，否则写 0 字节 |
| DV | 手写 `scale=720:480,fps=30000/1001` | 会 `ff_frame_thread_encoder_init failed`；用 `-target pal-dv` |
| MXF | 任意采样率 | 复用器只认 48 kHz，漏了报 `only 48khz is implemented` |
| DTS | `-c:a dca` | 是实验性编码器，必须加 `-strict -2` |
| 静态图识别 | 看流类型 | PNG 也是 `codec_type=video`，要按容器名判定 |
| 进度 `out_time_us` | 按整数解析 | 首帧输出 `N/A`，解析失败要跳过而不是中止 |

## 许可

本项目 MIT。

分发的 ffmpeg 为 GPL v3 构建（含 libx264 / libx265），许可证见 `vendor/ffmpeg/LICENSE.txt`。
源码地址：https://ffmpeg.org/download.html

ffmpeg 是 FFmpeg 项目的作品，本项目只是调用它。
