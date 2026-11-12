# 参与开发

## 提交前

改动前先读 `AGENTS.md`。那里的铁律不是风格建议，多数是踩过之后写下的：
`args.rs` 保持纯函数、端到端测试必须真跑、取消要清理残片、
界面颜色一律走 Fluent 的 CSS 变量。

提交前跑一遍：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo test --manifest-path src-tauri/Cargo.toml --test e2e
npx tsc --noEmit
```

改动涉及格式表、参数构建或直拷表时，再跑：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test formats_sweep
```

## 报告问题

发 issue 时请带上三样东西，否则很难定位：

1. 源文件信息：`ffprobe -hide_banner <文件>` 的输出
2. 目标格式，以及界面上显示的失败原因
3. 安装包版本（右下角版本按钮上的数字）

ffmpeg 版本与能力清单也会影响判断，可以附上 `ffmpeg -version` 的输出。

## 代码约定

- 纯函数优先。能在单测里断言出完整结果的逻辑，不要挪进有 IO 的模块。
- 新增格式必须同时满足三件事：`formats.rs` 里有定义、`format_availability` 通过、
  `every_format_is_available_with_real_capabilities` 测试通过。
- 修 bug 要带一条能复现的测试。没有测试的修复会在下次以另一种形态回来。
- 界面颜色用 Fluent 的 CSS 变量（`var(--colorNeutral…)`），不硬编码色值。
- 文档与提交信息用中文，不口语化，不用第二人称。

## 提交信息

```
<type>(<scope>): <一句话说明>

<需要时补充为什么这么改>

Agent: human
Verify: <跑过的命令与结果>
```

`type` 取 `feat` / `fix` / `docs` / `refactor` / `test` / `chore`。
`Agent:` 标记执行者，人写 `human`，自动化工具写自己的名字。
`Verify:` 写真实跑过的验证，没验证写 `none`。

## 文档语言

本项目的文档、代码注释、提交信息全部用中文。代码标识符用英文。

## 许可

提交的代码按 MIT 许可授权。分发的 FFmpeg 是 GPL v3 构建，
合规说明见 `vendor/ffmpeg/THIRD-PARTY.md`。
