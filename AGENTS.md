# MediaForge v2

Windows 媒体格式转换器。Tauri 2 + Rust + Vanilla TypeScript + Fluent 2，ffmpeg 随包分发。

v1 是 WPF + .NET 自包含（安装包 119 MB，装完 321 MB），已归档在 `../mediaforge`。
本仓库是重写版，体积目标是安装包约 45 MB。

## 技术栈

- 外壳：Tauri 2.12（WebView2）
- 后端：Rust 1.99，MSVC 工具链
- 前端：Vanilla TypeScript + Vite 8，无框架
- 组件：`@fluentui/web-components` 3.2.1（选 18 个按需注册）
- 引擎：ffmpeg n9.0.2 shared 版，9 个文件放在 `vendor/ffmpeg/bin/`
- 打包：Tauri NSIS，`embedBootstrapper`

## 常用命令

```bash
# 准备 ffmpeg（首次必做；181 MB，不进版本库）
powershell -ExecutionPolicy Bypass -File scripts/fetch-ffmpeg.ps1

npm install

# 开发（Vite 走 1420 端口，必须 HTTP，file:// 下 ES module 会被 CORS 拦）
npm run tauri dev

# 纯单测（140 个，约 0.2 秒，不碰进程）
cargo test --manifest-path src-tauri/Cargo.toml --lib

# 端到端（11 个，真跑 ffmpeg 并回读产物，约 2 秒）
cargo test --manifest-path src-tauri/Cargo.toml --test e2e

# 全格式扫描（8 个，71 个格式 × 重编码/直拷两种模式，约 5 秒）
cargo test --manifest-path src-tauri/Cargo.toml --test formats_sweep

# 全部测试
cargo test --manifest-path src-tauri/Cargo.toml

# 前端类型检查与构建
npx tsc --noEmit
npm run build

# 打包安装程序
npm run tauri build
```

## 铁律

1. **`args.rs` 保持纯函数。** 不得 `use std::process`、`use std::fs`、不得启动进程。
   有一条测试读源码断言这一点。所有格式组合必须能在单测里断言出完整参数表。
2. **端到端测试必须真跑。** 找不到 ffmpeg 就让测试失败，不允许静默跳过。
   静默通过等于没测。
3. **新增格式必须同时满足**：`formats.rs` 里有定义、`format_availability` 通过、
   `every_format_is_available_with_real_capabilities` 测试通过。
4. **不得静默产出空壳文件。** 跨类型转换先过 `compat::judge`；
   参数构建在缺流时返回 `Err` 而不是拼一条注定失败的参数表。
5. **取消要清理残片。** `runner::run_ffmpeg` 的中断路径必须删除本次创建的目标文件。
6. **界面颜色一律走 Fluent 的 CSS 变量**（`var(--colorNeutral…)`），不硬编码色值。
7. **文档与提交信息用中文。** 不口语化，不用第二人称。
8. **直拷必须过 `copy.rs` 的兼容表。** 容器不收该码流时自动回退重编码并说明原因，
   不得直接加 `-c copy` 让 ffmpeg 报错。表里的每一项都必须来自
   `scripts/copy-matrix.sh` 的实测，不得凭印象写。
9. **参数面板不写死控件。** 可调参数由 `params.rs` 按目标格式生成规格，
   界面按规格渲染。新增参数要同时改 `ParamSpec`、`Tune` 与 `args.rs` 的消费处。

## 模块导航

| 位置 | 职责 | 性质 |
|---|---|---|
| `src-tauri/src/formats.rs` | 71 个格式定义 | 纯数据 |
| `src-tauri/src/args.rs` | 参数构建 | 纯函数 |
| `src-tauri/src/compat.rs` | 跨类型兼容矩阵 | 纯函数 |
| `src-tauri/src/copy.rs` | 容器×码流直拷兼容表（全部实测） | 纯数据 |
| `src-tauri/src/params.rs` | 格式参数规格与任务级细调 | 纯函数 |
| `src-tauri/src/progress.rs` | `-progress` 解析 | 纯函数 |
| `src-tauri/src/caps.rs` | 能力清单解析 | 纯函数 |
| `src-tauri/src/probe.rs` | ffprobe 输出解析 | 纯函数 |
| `src-tauri/src/subtitle_text.rs` | MicroDVD / SBV 写入器 | 纯函数 |
| `src-tauri/src/output_path.rs` | 输出路径推导 | 纯函数 |
| `src-tauri/src/paths.rs` | 定位随包 ffmpeg | 有 IO |
| `src-tauri/src/settings.rs` | 设置读写 | 有 IO |
| `src-tauri/src/runner.rs` | 子进程、进度、取消 | 有 IO |
| `src-tauri/src/lib.rs` | 命令、状态、事件 | 装配层 |
| `src-tauri/tests/e2e.rs` | 11 个真实转换测试 | 端到端 |
| `src-tauri/tests/formats_sweep.rs` | 71 个格式 × 重编码/直拷扫描 | 端到端 |
| `src/store.ts` | 前端单一状态容器 | 视图 |
| `src/fluent.ts` | 组件注册与主题切换 | 视图 |
| `src/views/task-list.ts` | 任务列表（按 id 增量更新） | 视图 |
| `src/views/param-panel.ts` | 按规格动态渲染参数面板 | 视图 |
| `scripts/copy-matrix.sh` | 实测容器×码流直拷矩阵 | 工具 |
| `scripts/check-copy-table.py` | 校验 `copy.rs` 与实测是否一致 | 工具 |

更细的设计依据见 `docs/DESIGN.md`。

## 已知约束

- `vendor/` 在 `.gitignore` 里（181 MB）。新克隆后必须跑 `fetch-ffmpeg.ps1`。
- `fetch-ffmpeg.ps1` 必须带 UTF-8 BOM。PowerShell 5.1 读无 BOM 的 UTF-8
  会把中文注释解析错，报 `Try 语句缺少 Catch 或 Finally`。
- `src-tauri/tauri.conf.json` 的 `bundle.resources` 逐个列出 9 个文件。
  换 ffmpeg 版本时 DLL 名带版本号（`avcodec-63.dll` 等），必须同步改。
- **换图标后必须先清构建缓存再打包。** `tauri-build` 的 `rerun-if-changed` 不覆盖 `icons/`，
  直接 `tauri build` 会得到仍带旧图标的 exe。正确顺序：
  `npx tauri icon <图>` → `rm -rf src-tauri/target/release/build/mediaforge-v2-*` → `npm run tauri build`。
  验证要看从 exe 里提取的图标，不能只看 `icons/` 下的文件。
- **安装程序图标单独配置。** `bundle.icon` 只管 exe 与安装包内的资源，
  NSIS 安装程序本身的图标由 `bundle.windows.nsis.installerIcon` 决定，
  不配就是 NSIS 默认图标。
- **应用内左上角的品牌图标要单独放。** `npx tauri icon` 只生成系统级图标
  （exe / 任务栏 / 安装程序），不会碰前端。界面里的 logo 是
  `src/assets/logo.png`，需要手动从 `icons/128x128.png` 复制一份。
  换 logo 时这两处都要动。
- 开发期加 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223`
  可以用 CDP 连进 WebView2 调试。正式构建不要带这个环境变量。
- 打包首次会下载 NSIS 3.13 与 `nsis_tauri_utils`，以及 WebView2 bootstrapper
  （`go.microsoft.com/fwlink/p/?LinkId=2124703`）。三者都需要网络，且后者最大。
- **直拷兼容表换 ffmpeg 版本后必须重测。** 表里每一项来自
  `scripts/copy-matrix.sh` 的实测，用真实容器造每种编码器的源
  （`-f nut` 中转会丢 extradata，造出假阴性）。测完跑
  `python scripts/check-copy-table.py <matrix.tsv>` 校验，必须「严重 0 项」。
- **wavpack 对输入帧长敏感。** 从 mp3 这类非 4096 帧长的流直接编码会报
  `invalid block size` 并产出 0 字节，参数里必须带 `-af asetnsamples=n=4096`。
- **webm / ogg 要求音视频成套。** 只检查音轨够不够是不够的：
  H.264 视频 + Opus 音轨进 webm 仍会报错，判定时必须同时看视频码流。
- **`display: grid` 会盖掉 `[hidden]`。** 类选择器优先级高于属性选择器，
  绝对定位的空状态要显式写 `.x[hidden] { display: none }`。
- **音频码率必须按编码器与声道数夹紧。** 设置里的码率是全局值，但每个有损
  编码器的上限不同，libopus 与 libvorbis 的上限还随声道数变化（单声道
  libopus 上限 256k、立体声 512k；单声道 vorbis 192k、立体声 384k；mp2 384k）。
  超出时 ffmpeg 直接拒绝打开编码器，产物 0 字节，界面只显示「无法转换」。
  夹紧事实要写进 `notice`，否则用户以为设置生效了。
  实现在 `args.rs` 的 `effective_audio_bitrate`。
- **`cargo build --release` 的 exe 指向 devUrl，不能当安装包用。**
  它没有注入前端资源，运行时会去连 `localhost:1420`。手工把它复制到
  安装目录会得到「页面打不开」，而不是打包好的界面。验证界面必须用
  `npm run tauri build` 的安装包，页面 origin 应为 `tauri.localhost`。
- **同时开多个实例会撞 WebView2 用户数据目录。** 症状是
  `failed to create webview: HRESULT(0x8007139F)`。测完先杀干净
  `mediaforge-v2.exe` 再启动下一个，否则新实例起不来。
- **`cargo fmt` 会把 CRLF 改成 LF。** 本仓 Rust 文件是 CRLF、前端文件是 LF，
  直接 fmt 会让整个文件显示成改动，进而把无关文件混进提交。
  fmt 之前先确认工作区干净，fmt 之后用 `git diff --numstat` 核对规模，
  超出实际改动就按原行尾还原（`git show HEAD:<file>` 判断该文件原本的格式）。
