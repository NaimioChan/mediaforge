# Agent 工作记录

新条目写在最上方。每条记录：时间 / 执行者 / 任务 / 改动 / 验证 / 遗留。

---

## 2026-11-12 16:20 · hermes · 更新 AGENTS.md 至当前状态

- 改：AGENTS.md
- 任务：进入维护阶段，把 AGENTS.md 与实现对齐，补上此前被拦截没写进去的三条铁律。
- 改动：
  - 铁律 10/11/12：更新检查静默、ffmpeg 版本钉死、许可证随包分发。
  - 修正过期数据：单测 140→152、ffmpeg 版本号写明、体积补实测值、
    `bundle.resources` 说明补两份许可证文件。
  - 删除失效条目「cargo fmt 会把 CRLF 改成 LF」：`.gitattributes` 已统一 LF，
    该问题不存在。换成 `.gitattributes` 与 ps1 的 BOM 约束。
  - 新增「分支模型」一节，指向 `docs/RELEASING.md`。
  - 模块导航补 `version.rs`、`api.ts`、`update-check.ts`。
  - 已知约束补两条实测踩到的：撞 WebView2 目录要连 `msedgewebview2.exe` 一起杀、
    CDP 端口在设了 `HTTP_PROXY` 时要用 `--noproxy`。
- 验证：`cargo test --lib` 152 passed（与文档数字一致）；rustc 1.99.0、
  tauri 2.12.5、fluent 组件 18 个均对照源码核实；grep 确认无凭据泄露。
  纯文档改动，未重跑 e2e 与打包。
- 遗留：无。

---

## 2026-11-12 15:55 · hermes · 许可证改英文、README 加截图、仓库转公开

- 改：LICENSE.txt, README.md, docs/screenshot.png（新）
- 任务：用户要求把 LICENSE 里的说明改成英文，README「下载」标题上方加一张运行截图，
  然后转公开。README 正文用户已在 GitHub 网页上改过，先 fetch 合并再动。
- 改动：
  - `LICENSE.txt` 的第三方组件说明（FFmpeg / Fluent / Tauri）全部改英文，
    MIT 正文与版权行不动。中文行只剩版权名 `非茗 (Naimio)`。
  - `README.md` 只在 `## 下载` 上方插入 `![MediaForge 界面](docs/screenshot.png)`，
    用户此前在网页上的措辞修改原样保留（`git diff e2ae8cc 8e81d8c` 确认过范围）。
  - `docs/screenshot.png`：安装版 + CDP 抓取，浅色主题。
- 验证：
  - 截图前先用 CDP 断言界面真实状态：rows=4、targets=[webm,flac,webp,ass]、
    panel 显示「WebM 参数」、版本按钮 v2.0.0、无溢出（1180x760 vs 1180x760）。
  - 转公开后匿名访问：README 200、截图 200、Release 安装包 200（56,104,579 字节）。
  - `gh api releases/latest` 确认 tag=v2.0.0、非 draft、非 prerelease。
- 遗留：
  - **GitHub API 匿名限流 60 次/小时（按 IP）**。本机走 Clash 代理，出口 IP 与其他用户共享，
    调试时被打到 `rate limit exceeded`。应用端是用户自己的 IP，正常够用；
    但若将来发现更新检查经常静默失败，可考虑加 `ETag` 缓存或改用 `releases` 列表接口。
  - `AGENTS.md` 仍是上一次被拦截的状态，两条铁律（更新检查静默、ffmpeg 版本钉死）未写入。
  - 截图是浅色主题（安装版默认跟随系统）。暗色主题未出图。

---

## 2026-11-12 13:35 · hermes · 更新检查、GPL 合规、开源文档

- 改：src-tauri/src/version.rs（新）, src/update-check.ts（新）, src-tauri/src/lib.rs,
  src/api.ts, src/types.ts, src/main.ts, index.html, src/styles.css,
  src-tauri/tauri.conf.json, scripts/fetch-ffmpeg.ps1, README.md,
  vendor/ffmpeg/THIRD-PARTY.md（新）, CHANGELOG.md（新）, CONTRIBUTING.md（新）,
  docs/RELEASING.md（新）, .github/ISSUE_TEMPLATE/（新）
- 任务：加更新检查（右下角版本按钮），补齐开源发布所需的许可证与文档。
- 改动要点：
  - 版本比较在 `version.rs`，按 semver 2.0 处理预发布标识，解析失败一律
    「不构成更新」。HTTP 走前端 fetch，不为一次 GET 引入 reqwest。
  - GPL 合规：安装包此前只列 9 个 ffmpeg 二进制，没附许可证。
    `bundle.resources` 补 `LICENSE.txt` 与 `THIRD-PARTY.md`；后者写明
    精确到 commit 的版本、构建变体、上游源码地址，以及本项目不链接 libav*。
  - `fetch-ffmpeg.ps1` 的 URL 从 `latest` 钉到 `autobuild-2026-11-10-12-39`。
- 验证：
  - 单测 152 全绿（新增 9 条版本比较）；e2e 11；formats_sweep 8；tsc 通过。
  - `npm run tauri build` 出 53.51 MiB 包；静默安装后 `ffmpeg/` 目录含
    `LICENSE.txt` 与 `THIRD-PARTY.md`。
  - 安装版 CDP 验：origin=`tauri.localhost`、版本按钮显示 v2.0.0、
    GitHub API 请求被 CSP 放行（404 因仓库未建，正是静默路径）。
  - 有更新路径另验：临时指向 ripgrep 仓库，按钮变「可更新 15.2.0」并带 has-update。
- 遗留：
  - **想改 `AGENTS.md` 但被受保护文件拦截**（审批超时）。要补两条铁律：
    ① 更新检查失败必须静默、版本比较解析失败不构成更新；
    ② ffmpeg 版本钉死在 `fetch-ffmpeg.ps1`，不得用 `latest`。
    现状：文档里没有这两条，代码已按此实现。
  - 安装包未做代码签名，README 已写明 SmartScreen 的处理方式。
  - GitHub 私有仓待建（用户要求先私有，review 文档后再转公开）。

---

## 2026-11-12 12:56 · hermes · 输出目录改为可手输且不跨会话记忆

- 改：index.html, src/main.ts, src-tauri/src/settings.rs, src-tauri/src/lib.rs, docs/DESIGN.md
- 任务：输出目录字段原本 `readonly` 只能靠对话框选，且会被记住。
  按用户选择，改为可手输、只在启动时为空（同一次使用中导入新文件不清空）。
- 改动：
  - `index.html` 去掉 `readonly`，提示改为「可直接输入或粘贴路径」。
  - `settings.rs` 新增 `normalize_output_dir`（去引号/空格/尾分隔符，`D:` 补 `D:\`）；
    `load` 与 `save` 都清空 `output_dir`，`settings.json` 里恒为空串。
    不能用 `serde(skip)`——它同时作用于 Tauri 命令反序列化，会把前端传的值一起丢掉。
  - `lib.rs` 的 `save_settings` 先规范化再 `create_dir_all`，建不出来当场报错。
  - `main.ts` 回车即提交；后端拒绝时输入框退回上次生效值；目录对话框带 `defaultPath`。
- 验证：单测 143 全绿（新增 4 条盯落盘与规范化）；端到端 11 全绿；
  全格式扫描 8 全绿；`npx tsc --noEmit` 与 `npm run build` 通过。
- 遗留：未做安装包界面的手工点验（用户要求打包不推送，界面验收由用户进行）。

---

## 2026-11-12 11:00 · hermes · 修掉码率上限，打包并安装验证

**任务**：用户问是否打包了。打包后按用户真实设置复测，发现新的失败。

**根因**：用户 `settings.json` 里 `audioBitrateKbps` 是 320，而 libopus 单声道
上限只有 256k。ffmpeg 报 `The bit rate 320000 bps is unsupported` 并拒绝打开
编码器，产物 0 字节。此前所有测试都用默认 192k，所以没暴露。

码率上限随编码器与声道数变化，实测结果：

| 编码器 | 单声道 | 立体声 |
|---|---|---|
| libopus | 256k | 512k |
| libvorbis | 192k | 384k |
| mp2 | 384k | 384k |
| libmp3lame / aac / ac3 / eac3 / wmav2 | 1536k 不设限 | 同左 |

**改动**：

- `args.rs` 新增 `effective_audio_bitrate`，按目标声道数夹紧码率。
  夹紧后写进 `notice`，避免用户误以为设置生效。
- 新增 4 个单测盯夹紧逻辑，全格式扫描加一条「320k 设置下所有音频格式都要成功」。

**验证**：

| 项目 | 结果 |
|---|---|
| 单测 | 140 个全绿 |
| 端到端 | 11 个全绿 |
| 全格式扫描 | 8 个全绿（含新增的 320k 场景） |
| 已安装包 · 音频 | 单声道 Battle.mp3 在 320k 设置下转 15 个格式，15/15 成功 |
| 已安装包 · 视频 | 转 11 个容器全成功，webm 与 ogv 明确提示码率已调整 |
| 产物核对 | 全部被 ffprobe 正确识别，码流/尺寸/时长无误 |

**过程中发现的两个坑**（已写进 AGENTS.md）：

1. **`cargo build --release` 的 exe 指向 devUrl。** 它没有注入前端资源，
   运行时去连 `localhost:1420`。手工复制到安装目录会得到空白窗口，
   不是打包界面。验证界面必须用 `npm run tauri build` 的安装包，
   origin 应为 `tauri.localhost`。这一条让我多花了两轮排查。
2. **同时开多个实例会撞 WebView2 用户数据目录**，
   报 `HRESULT(0x8007139F)`。测完要杀干净再启下一个。

**遗留**：

- `cargo fmt` 会把 Rust 文件的 CRLF 改成 LF，导致整个文件显示成改动。
  这次手工还原了三次行尾符。建议加 `.gitattributes` 固定行尾符，未做。
- 安装包已生成并安装验证，未推送远端的提交有两个。

---

## 2026-11-12 08:00 · hermes · 修复全格式转换失败，重做参数面板

**任务**：用户实测反馈两个问题：mp3 转其它格式除 mp3/wav/wma 外全部报错；
右侧参数面板形同虚设，只能调音频码率与分辨率。

**根因**：两个问题其实是一个成因链条。

1. 「码流直拷」是一个**全局开关**，用户不懂它的含义而打开（用户原话：
   「我不知道码流直拷是干什么的就开了」）。打开后所有任务都加 `-c:a copy`，
   把源码流原样塞进目标容器。
2. 每个复用器只认有限的几种码流。mp3 音轨能进 mp3 / wav / asf，
   进 flac / opus / ac3 / adts / ipod / aiff / ogg / tta / wv / eac3 / dts
   一律在写文件头时报错。于是「除了 mp3 wav wma，其他全部无法转换」。
3. 参数面板写死了两个控件（质量滑块、音频码率下拉），
   而它们只对部分编码器有意义，所以看起来「死的」。

**改动**：

后端

- 新增 `src-tauri/src/copy.rs` — 容器×码流直拷兼容表，**全部来自实测**。
  三个函数分别管纯视频、纯音频、视频容器里的音轨。
- 新增 `src-tauri/src/params.rs` — 参数规格表（`ParamSpec`）与任务级细调
  （`Tune`）。规格按目标格式动态生成，界面不需要认识具体格式。
- `args.rs` — 直拷改为「过兼容表才拷，否则自动回退重编码并在任务上说明原因」；
  新增码率模式、preset、帧率、采样率、声道、GIF 调色板参数；
  修掉 wavpack 缺 `asetnsamples` 导致产出 0 字节的问题。
- `settings.rs` — 删掉全局 `copyStreams` 字段，直拷不再是一个全局开关。
- `lib.rs` — `Task` 增加 `tune`；新增 `param_specs` / `set_tune` 两个命令；
  切换目标格式时自动裁剪该格式不认识的参数。

前端

- 新增 `src/views/param-panel.ts` — 按规格动态渲染控件，
  支持下拉 / 滑块 / 数字 / 开关四种形态与分组。
- 参数改为**任务级**：选中某条任务才能调，调的是那一条。
- 删掉 `index.html` 里写死的五个控件。
- 修 `empty-state` 的 `[hidden]` 被 `display: grid` 盖掉的老 bug
  （有任务时空提示仍然压在任务行上）。

**验证**：

| 项目 | 结果 |
|---|---|
| 纯单测 | 133 个全绿（原 96，新增 37） |
| 端到端 | 11 个全绿，真跑 ffmpeg 回读产物 |
| 全格式扫描 | 视频 20 + 音频 15 + 图像 29 + 字幕 7，直拷开/关各一轮，**全部成功**，无 0 字节产物 |
| 音频重编码矩阵 | 11 种源 × 15 个目标 = 165 组，除 wavpack（已修）外全通过 |
| 容器拷贝矩阵 | 视频 19 容器 × 20 码流、音频 18 容器 × 17 码流，逐组实测后固化成表 |
| 界面（CDP 真机） | mp3/mp4/png/srt 四类任务逐格式切换，面板控件与格式严格对应 |
| 真实转换 | mp3 → flac 设压缩级别 5，产物 265 KB（默认 8 是 255 KB），确认细调生效 |

**过程中发现的坑**：

1. **用 nut 当中间容器会造出假阴性。** 首次矩阵用 `-f nut` 做中转，
   nut 会丢 extradata，导致 prores / dnxhd / eac3 / tta 等编码器被误判为
   「无源文件」。必须用真实容器造每种编码器的源。
2. **wavpack 对输入帧长敏感。** 从 mp3 这类非 4096 帧长的流直接编码会报
   `invalid block size` 并产出 0 字节，必须加 `-af asetnsamples=n=4096`。
3. **`display: grid` 会盖掉 `[hidden]`。** 属性选择器的优先级低于类选择器，
   绝对定位的空状态必须显式写 `.x[hidden] { display: none }`。
4. **CDP 验证时点击任务是切换选中。** 已选中的任务再点一次会取消选中，
   脚本里要先读 `dataset.focused` 再决定点不点，否则会误判「面板变空」为 bug。

**遗留**：

- `embedBootstrapper` 仍因网络无法打包。
- 参数暂不持久化：切换任务后回到默认值，只有设置文件里的全局默认会留存。

---

## 2026-11-11 22:20 · hermes · 替换 logo 并完成安装验证

**任务**：用户提供品牌 logo（1024×1024 PNG），替换全部图标并重新交付。

**改动**：

- `npx tauri icon` 生成全套：`icon.ico` 含 16/24/32/48/64/256 六尺寸，
  另有 32/64/128/128@2x/icon.png 与 Windows Store 系列
- 删掉用不到的 `icons/ios/`、`icons/android/`、`icon.icns`（共 30 余个文件），
  同步从 `bundle.icon` 移除 `.icns` 引用
- `bundle.windows.nsis.installerIcon` / `uninstallerIcon` 指向 `icons/icon.ico`
- 应用内左上角品牌区：`index.html` 的 `<span>MF</span>` 换成 `<img src="/src/assets/logo.png">`，
  尺寸 24px，去掉原来给的圆角（logo 自带圆形底）

**过程中发现三个图标相关的坑**：

1. **换图标后必须清构建缓存。** `tauri-build` 的 `rerun-if-changed` 不覆盖 `icons/`。
   第一次换完直接打包，exe 里仍是 Tauri 默认图标（`resource.lib` 时间戳停在改图标之前）。
   清掉 `target/release/build/mediaforge-v2-*` 后重编才生效。
2. **安装程序图标是独立配置。** `bundle.icon` 不管 NSIS 安装器本身，
   不配 `installerIcon` 就是 NSIS 默认的绿色下载箭头。
3. **应用内 logo 不跟随图标集。** `tauri icon` 只处理系统级图标，
   前端要单独放一份到 `src/assets/logo.png`。

**验证**：

| 项目 | 结果 |
|---|---|
| exe 图标 | 从 exe 提取确认是新 logo |
| 安装程序图标 | 从 setup 提取确认是新 logo |
| 应用内 logo（dev） | 128×128 源，渲染 24×24，`complete && naturalWidth>0` |
| 应用内 logo（安装后） | `http://tauri.localhost/` 下同样正确 |
| 静默安装 | exit=0，185 MB，9 个 ffmpeg 文件 |
| 安装后转换 | mp4 → avi 成功，产物 mpeg4 + mp3、640×360、2.04 s |
| 静默卸载 | exit=0，目录/注册项/开始菜单全部清空 |

**遗留**：

- `embedBootstrapper` 仍因网络无法打包（详见下方条目）。
- 仓库已初始化并完成首次提交（`b123237`），未推送。

---

## 2026-11-11 21:20 · hermes · MediaForge v2 从零搭建

**任务**：按已确认的实施计划（`docs/plan-2026-11-11.md`）从零实现 Tauri 版媒体转换器。

**改动**：

Rust 后端（`src-tauri/src/`，12 个模块）

- `formats.rs` — 71 个格式定义，从 v1 `FormatCatalog.cs` 迁入并转成静态表
- `args.rs` — 参数构建器，纯函数；含 ICO / DV / MXF / GIF 四条专用管线
- `compat.rs` — 4×4 兼容矩阵（允许 6 组，拒绝 10 组）
- `progress.rs` — `-progress` 解析，处理 `N/A` 与 `out_time_ms` 实为微秒
- `caps.rs` — 能力清单解析，处理行首空格与表头行
- `probe.rs` — ffprobe 解析，按容器名区分静态图与视频
- `subtitle_text.rs` — MicroDVD / SBV 写入器（ffmpeg 产不出这两个格式）
- `output_path.rs` — 输出路径推导，不覆盖源文件
- `paths.rs` / `settings.rs` / `runner.rs` — 有 IO 的三层
- `lib.rs` — 命令、状态、事件

前端（`src/`）

- `store.ts` 单一状态容器，`api.ts` 类型化 invoke/listen，`fluent.ts` 18 个组件注册
- `views/task-list.ts` 按 id 增量更新，进度事件不重绘整表
- 深浅色跟随系统（`matchMedia`），参数面板、拖放、输出目录、命令行入队

测试

- 96 个纯单测（格式表完整性、能力解析、进度、兼容矩阵、参数构建、字幕写入器、路径推导）
- 11 个端到端（真跑 ffmpeg 并回读产物，含取消清理残片）

**验证**：

| 项目 | 结果 |
|---|---|
| `cargo test` | 96 + 11 全绿，纯单测 0.16 s，端到端 2.4 s |
| `cargo clippy --all-targets` | 0 警告 |
| `npx tsc --noEmit` | 0 错误 |
| `npm run build` | 231 KB / gzip 58 KB |
| 真机 `tauri dev` | 窗口正常，深色主题生效，`ffmpeg n9.0.2` / 233 编码器 / 184 复用器 |
| 拖放/添加 4 类文件 | 15 / 64 个兼容格式按源类型正确过滤 |
| 实时进度 | 18% → 35% → 60% → 76% → 90% → 完成，速度 6.7×→5.6× |
| 取消 | 35% 取消即停，无残片 |
| 产物回读 | flac / webp / matroska / microdvd 全部被 ffprobe 正确识别 |

**过程中发现并修掉的两个缺陷**：

1. 目录扫描把输出目录里的产物当新源文件（同一素材被加入 3 次）
2. 取消后进度条停在中断处的百分比

**与任务书不一致、已按实测调整的四处**：

1. `webLightTheme` 不在 `@fluentui/web-components`，在 `@fluentui/tokens`
2. MicroDVD 在当前 ffmpeg 构建里没有编码器，改为本程序直接写文本
3. SBV 在 ffmpeg 里没有复用器，v1 的「支持」是 SRT 内容存 `.sbv`，改为按 SBV 语法写
4. ICO / DV / MXF / DTS 四个格式需要专用参数，通用写法会产出 0 字节

**遗留**：

- `embedBootstrapper` 需要构建时从微软下载 bootstrapper，家宽 + 代理下容易断流
  （`io: unexpected end of file`）。已下载的副本放在 `src-tauri/webview2/`，
  但 Tauri CLI 不支持指定该路径，仍需网络。
- exe 3.69 MB，比基线 2.95 MB 多 0.74 MB，来源是 dialog / window-state / opener 三个插件。
