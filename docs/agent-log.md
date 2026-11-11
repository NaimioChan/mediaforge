# Agent 工作记录

新条目写在最上方。每条记录：时间 / 执行者 / 任务 / 改动 / 验证 / 遗留。

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
