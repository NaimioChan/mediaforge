# MediaForge

Windows 桌面媒体格式转换器。ffmpeg 的可视化外壳，界面用 Fluent 2，外壳用 Tauri 2。

支持视频、音频、图像、字幕四类共 71 种目标格式。ffmpeg 随包分发，装完即可用，
不需要另外安装任何东西。

- 队列式批量转换，可调并发数
- 每个目标格式有各自的参数面板（编码器、质量、分辨率、码率等），只列出当前 ffmpeg
  构建真实支持的选项
- 拖入文件夹时递归展开，可识别的媒体文件才入队
- 转换中途取消会删掉半成品，不留打不开的空文件
- 同一格式互转时自动避让源文件，不覆盖原件

## 下载

从 [Releases](https://github.com/NaimioChan/mediaforge/releases) 下载
`MediaForge_x.y.z_x64-setup.exe`。

安装包未做代码签名，首次运行 Windows SmartScreen 会拦截。点「更多信息」→「仍要运行」。

系统要求：Windows 10 1809 以上或 Windows 11，x64。WebView2 运行时由安装程序按需下载。

## 使用

1. 拖入文件或文件夹，或点「添加文件」
2. 在每条任务右侧选目标格式，选中任务后可在参数面板细调
3. 「输出目录」留空则输出到源文件同目录；也可以手输路径或点「选择」
4. 点「开始转换」

输出目录不跨会话记忆，每次启动都是空的，避免产物落到上次选过的地方。

## 许可

源码以 MIT 许可发布，见 [LICENSE.txt](LICENSE.txt)。

安装包内分发的 FFmpeg 为 GPL v3 构建（含 libx264 / libx265），许可证与
上游源码指引见 [vendor/ffmpeg/THIRD-PARTY.md](vendor/ffmpeg/THIRD-PARTY.md)。

本项目不链接 FFmpeg 的库，只以独立进程方式调用 `ffmpeg.exe` 与 `ffprobe.exe`。

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

产物：`src-tauri/target/release/bundle/nsis/MediaForge_<version>_x64-setup.exe`

`vendor/` 不进版本库（181 MB），必须靠脚本准备。脚本把 ffmpeg 版本钉在
FFmpeg-Builds 的 `autobuild-2026-11-10-12-39`，不要改成 `latest`：上游每天出构建，
内容一变，`copy.rs` 里实测出的直拷兼容表就可能失效。

## 跑测试

```bash
# 全部测试（152 个纯单测 + 11 个真实 ffmpeg 端到端 + 8 个全格式扫描）
cargo test --manifest-path src-tauri/Cargo.toml

# 只跑纯单测（不碰进程，约 0.2 秒）
cargo test --manifest-path src-tauri/Cargo.toml --lib

# 只跑端到端（会真的执行转换，约 2 秒）
cargo test --manifest-path src-tauri/Cargo.toml --test e2e

# 全格式扫描（71 个格式 × 重编码/直拷两种模式，约 5 秒）
cargo test --manifest-path src-tauri/Cargo.toml --test formats_sweep

# 前端类型检查
npx tsc --noEmit
```

端到端测试不允许跳过：找不到 ffmpeg 直接失败，不会静默通过。

## 项目结构

```
src-tauri/src/
  formats.rs        71 个格式定义（纯数据）
  args.rs           参数构建（纯函数，不碰进程与文件系统）
  compat.rs         跨类型兼容矩阵（纯函数）
  copy.rs           容器×码流直拷兼容表（纯数据，全部实测）
  params.rs         格式参数规格与任务级细调（纯函数）
  progress.rs       -progress 解析（纯函数）
  caps.rs           能力清单解析（纯函数）
  probe.rs          ffprobe 输出解析（纯函数）
  subtitle_text.rs  MicroDVD / SBV 写入器（纯函数）
  output_path.rs    输出路径推导（纯函数）
  version.rs        版本号比较（纯函数）
  paths.rs          定位随包 ffmpeg
  settings.rs       设置持久化
  runner.rs         子进程驱动、进度采集、取消
  lib.rs            命令、状态、事件（装配层）
src-tauri/tests/
  e2e.rs            11 个真实转换测试
  formats_sweep.rs  71 个格式 × 重编码/直拷扫描
src/
  store.ts          单一状态容器
  api.ts            invoke / listen 封装
  update-check.ts   GitHub Releases 更新检查
  fluent.ts         组件注册与主题
  views/            任务列表、参数面板
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

**直拷必须过兼容表。** 容器不收该码流时自动回退重编码并说明原因，不直接加 `-c copy`
让 ffmpeg 报错。表里每一项都来自 `scripts/copy-matrix.sh` 的实测。

## 实测体积

| 组成 | 大小 |
|---|---|
| release exe | 3.69 MB |
| 前端产物 | 232 KB（gzip 60 KB） |
| ffmpeg 9 个文件 | 180.6 MB |
| 安装包 | 53.5 MiB |
| 安装后占用 | 185 MB |

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
| wavpack | 直接编码 | 对输入帧长敏感，从 mp3 这类流编码报 `invalid block size` 且产出 0 字节，必须带 `-af asetnsamples=n=4096` |
| webm / ogg | 只看音轨够不够 | 要求音视频成套：H.264 视频 + Opus 音轨进 webm 仍报错，判定时必须同时看视频码流 |
| 音频码率 | 用全局设置值 | 必须按编码器与声道数夹紧（单声道 libopus 上限 256k、vorbis 192k），超出时 ffmpeg 拒绝打开编码器 |

## 贡献

见 [CONTRIBUTING.md](CONTRIBUTING.md)。改动前请先读 [AGENTS.md](AGENTS.md) 里的铁律。

## 版本与发布

见 [docs/RELEASING.md](docs/RELEASING.md)。变更记录见 [CHANGELOG.md](CHANGELOG.md)。
