# 设计说明

这份文档记录实现里的判断依据。结论来自对真实 ffmpeg 的实测，不是推断。

## 一、状态为什么放后端

队列、任务状态、编码器能力、设置全部由 Rust 持有，前端只是视图。

理由有两条：

1. ffmpeg 是子进程。前端刷新、重载、崩溃都不该让正在跑的任务消失。
   实测过：页面重载后 `get_snapshot` 仍返回完整队列，正在跑的任务继续推进。
2. 参数构建与能力判断都在 Rust。前端再存一份状态必然与后端漂移，
   而漂移表现为「界面说能做、点了却失败」，这种 bug 最难查。

命令返回变更后的快照，进度通过 `task-progress` / `task-update` 事件推送。

## 二、参数构建为什么必须是纯函数

`args::build_plan(源路径, 输出路径, 目标格式, 选项, 探测结果, 能力清单) -> Plan`

不启动进程、不读写文件、不读环境变量。收益：

- 71 种格式 × 若干参数组合，全部能在单测里断言出完整参数数组
- 端到端测试只需验证「参数真的能跑通」，不必覆盖每个组合

约束靠一条测试守住：

```rust
#[test]
fn source_has_no_process_or_filesystem_access() {
    let production = include_str!("args.rs").split("#[cfg(test)]").next().unwrap();
    // 断言 production 部分不含 std::process / std::fs / Command::new
}
```

## 三、几个必须绕开的 ffmpeg 行为

这些都是实测出来的，不是从文档读来的。

### 3.1 能力清单行首有空格

```
Encoders:
 V..... = Video            <- 表头
 ------
 V....D libx264   H.264 ... <- 数据行，行首一个空格
```

不先 `trim_start` 就分词，第一个字段是空串，第二列错位，解析出 0 个编码器。
表头行的第二列是 `=`，也要按名字里含 `=` 排除。

实测当前构建：233 个编码器、184 个复用器，解析耗时约 45 ms。

### 3.2 `out_time_ms` 的单位是微秒

名字叫 ms，实际是 µs。按毫秒算会把 3 秒转成 50 分钟。

### 3.3 进度首帧是 `N/A`

```
out_time_us=N/A
out_time_us=N/A
out_time_us=633333
```

C# 里 `long.TryParse` 失败返回 false，静默跳过。
Rust 里 `"N/A".parse::<i64>()` 返回 `Err`，若把 `Err` 当异常上抛，
任务会在开始 0.3 秒时假失败。`progress.rs` 明确把解析失败当作「本行无信息」。

### 3.4 `-map 0:a:0?` 的问号

问号表示「有则映射」。实测无声视频：

- `-map 0:a:0?` → 退出码 0
- `-map 0:a:0` → `Error parsing options for output file: Invalid argument`

### 3.5 取消会留下残片

40 秒素材在 1.3 秒处 kill，磁盘留下 48 字节的 mp4。
`runner.rs` 的取消路径会删掉本次创建的目标文件。

### 3.6 静态图在 ffprobe 里是「视频」

PNG 的 ffprobe 输出是 `codec_type: video`、容器名 `png_pipe`。
只看流类型会把一张图判成视频，于是「图像→图像」变成「视频→图像」，
还会显示 0.04 秒的假时长。

`probe.rs::kind()` 先看容器名：

- 图像类容器 + 单帧 → 图像
- 图像类容器 + 多帧（GIF/APNG/WebP）→ 视频（这样 GIF→MP4 才可用）
- 其余容器有视频轨 → 视频

## 四、四个需要专用管线的原因

| 格式 | 通用写法为什么失败 | 正确写法 |
|---|---|---|
| ICO | 源大于 256 时 0 字节；`scale=-1:256` 也 0 字节 | `scale=w=256:h=256:force_original_aspect_ratio=decrease,format=rgba` + `-frames:v 1 -update 1` |
| DV | 手写 `scale=720:480,fps=30000/1001` → `ff_frame_thread_encoder_init failed`，加 `-threads 1` 无效 | `-target pal-dv`，让 ffmpeg 自己约束 |
| MXF | mpeg2video 不补采样率 → `mxf: only 48khz is implemented` | dnxhd + `-ar 48000 -ac 2` + `pcm_s24le` |
| DTS | 裸 `-c:a dca` → 0 字节 | `dca` 是实验性编码器，必须 `-strict -2`，还要 `-ar 48000 -ac 2` |

## 五、两个格式为什么要自己写

### MicroDVD

```
$ ffmpeg -h encoder=microdvd
Codec 'microdvd' is known to FFmpeg, but no encoders for it are available.
```

复用器存在，编码器不存在。试过 10 种字幕编码器 × 有无 `-r` × `-c copy`，
全部 `Output file does not contain any stream`。这是工具链缺口。

### SBV

ffmpeg 没有 SBV 复用器。v1 的做法是用 srt 复用器 + `.sbv` 扩展名，
产出的是 SRT 语法（`00:00:00,500 --> 00:00:01,500`），
而 SBV 是 `0:00:00.500,0:00:01.500`。标成 SBV 不成立。

### 实现方式

两者都先从任意字幕转成 SRT（走 ffmpeg 写到 stdout），再解析成 `Cue` 列表，
最后按目标语法写出。`subtitle_text.rs` 全是纯函数，有 9 条单测。

实测验证：写出的 `.sub` 被 ffprobe 识别为 `microdvd` 编码器，
`.sbv` 被识别为 `subviewer`；MicroDVD 转回 SRT 时间只差一帧（帧率取整）。

## 六、跨类型兼容矩阵

16 组里允许 6 组，拒绝 10 组：

```
视频 → 视频 / 音频 / 图像     允许
音频 → 音频                   允许
图像 → 图像                   允许
字幕 → 字幕                   允许
其余 10 组拒绝，每一条都有中文原因
```

视频 → 字幕也在拒绝之列：视频里的字幕轨要单独抽取，不做格式转换。
用户若需要字幕，应当从字幕文件转。

## 七、输出目录互相污染

默认输出目录是源文件旁边。用户拖一整个文件夹时，第二次拖会把上次的产物
当成新源文件，队列每轮翻倍。实测复现过：同一个 40 秒素材被加入 3 次。

`collect_media_paths_excluding` 接受输出目录并跳过它。
有一条测试守着这个行为。

### 输出目录不跨会话记忆

输出目录字段只在本次会话内有效。`Settings::load` 与 `Settings::save`
都把这个值清空，`settings.json` 里恒为空字符串，下次启动回到「与源文件同目录」。

理由：记住输出目录会让用户忘记这一次到底输出到哪，而写错位置的代价
比重新选一次大。字段本身仍参与前后端数据交换，因此不能用 `serde(skip)`，
否则前端传上来的路径也会被一起丢掉。

目录可能由用户手输，`settings::normalize_output_dir` 负责去掉
「复制为路径」带出来的引号、首尾空格与尾部分隔符，并把 `D:` 补成 `D:\`。
`save_settings` 随后用 `create_dir_all` 建出目录，建不出来就当场报错，
不把失败留到转换阶段。

## 八、Siemens iX 的四个陷阱

界面用 `@siemens/ix` 5.3 的 web components。布局与交互不变，控件与配色换掉。

### 8.1 CSS 与元素定义是两件事

`@siemens/ix/dist/siemens-ix/siemens-ix.css`（213 KB）里既有组件样式，
也有 classic 深浅两套主题 token 块。但组件本身是 web components：
只引 CSS 不注册元素，页面上是一堆无样式的空标签。
元素定义要显式注册，见下一条。

### 8.2 不要用 `@siemens/ix/loader`

`defineCustomElements()` 把全部 150 多个组件一次性注册进来，
前端产物从 232 KB 涨到 2.2 MB（gzip 317 KB）。而且它注册的是
`ix-icon` 之外的全部元素——`ix-icon` 属于另一个包。

改为逐个 import `@siemens/ix/components/ix-<name>.js`，这些模块
**只导出 `defineCustomElement` 而不自己调用**，必须显式调一遍。
按实际用到的 11 个组件 + 3 个 shadow 内依赖注册后，产物 676 KB
（其中 iX 的 CSS 独占 213 KB），gzip 121 KB。

### 8.3 `ix-icon` 的图标必须给 data URL，不能用名字

`ix-icon` 拿到名字时不是查注册表，而是拼出 `svg/<name>.svg` 去 fetch，
那个目录不在打包产物里，于是每个按名字给的图标都 fetch 失败，
回退成 iX 的错误符号（一个带叉的方块）。

图标文件放在 `src/assets/icons/`（从 `@siemens/ix-icons/svg/` 复制，MIT），
`icons.ts` 以 `?raw` 导入并拼成 `data:image/svg+xml;utf8,<原始 SVG>`。
两个细节缺一不可：

- **不要 `encodeURIComponent`。** iX 内部只调 `decodeURI`，它不还原 `%3D`
  这类保留字符。编码过的 SVG 会被解析成属性字面量为 `width%3d"512px"`
  的 `<svg>`，每条 path 丢掉 `d`，渲染出来是一个空框——看起来像
  「图标没加载」，实际是「图标解析坏了」。
- **去掉 `<?xml …?>` 声明。** iX 用 `DOMParser` 按 `text/html` 解析，
  XML 声明在那里是 bogus comment，而这些图标文件大多以它开头。

另外，data URL 内含引号，写进 HTML 属性会被解析器截断，
所以静态 HTML 里的图标由 `main.ts` 的 `applyIcons()` 用属性赋值挂上去。

### 8.4 别在 `html` 上写 `font-size`

iX 的所有尺寸都是 `rem`（控件高 2rem、圆角 0.125rem），前提是根字号 16px。
在 `html` 上写 `font-size: 0.875rem` 会把整条 rem 标度缩到 87.5%，
按钮变成 28px 而不是 32px——看起来只是「有点小」，实际是标度错了。

字号写在 `body` 上，且选择器要写 `html body`：iX 自己的样式表里有一条
`body` 规则把字体链固定成 `Siemens Sans, Siemens Sans, Arial, Helvetica`，
同特异性下后加载的胜出，中文会退到 Arial。加 `html` 前缀抬到 (0,0,2)
才能让中文回退链生效。

### 8.5 控件事件的语义与原生控件不同

- `ix-select` 的选项是 `ix-select-item` 子元素（不是 `<option>`），
  选中后抛 `valueChange`，值在 `event.detail`，多选时是数组。
- `ix-slider` 的 `valueChange` 在拖动过程中持续抛，必须自己防抖，
  否则一次拖动写几十次设置。
- `ix-number-input` 每敲一个字符抛 `valueChange`；要「失焦或回车才提交」
  得监听 `ixChange`。清空输入框要 `allowEmptyValueChange`，
  否则抛 0 而不是「不限制」。
- `ix-toggle` 自带 "On"/"Off" 文案，中文界面要加 `hide-text`。

## 九、体积构成（实测）

| 组成 | 大小 |
|---|---|
| Tauri release exe | 3.69 MB |
| 前端产物 | 231 KB（gzip 58 KB） |
| ffmpeg 9 个文件 | 180.6 MB |
| 安装包（NSIS） | 53.41 MiB |
| 安装后占用 | 185 MB |

基线里 exe 是 2.95 MB，本实现多了 0.74 MB，来自 dialog / window-state / opener 三个插件。
其中 dialog 必需（选文件与目录），window-state 对应「窗口尺寸持久化」需求，
opener 只用来「在资源管理器中显示」。若要压回 2.95 MB，去掉 opener 即可。

安装后占用 185 MB 与预期完全一致（ffmpeg 180.6 MB + exe 3.69 MB + 前端资源）。

## 十、WebView2 安装模式的实际取舍

需求写的是 `embedBootstrapper`（+1.8 MB，离线可用）。实际打包时这条路走不通：

```
Downloading https://go.microsoft.com/fwlink/p/?LinkId=2124703
failed to bundle project: `io: unexpected end of file`
```

Tauri CLI 在打包阶段自己去微软下载 bootstrapper，走的是它内部的 HTTP 客户端，
不读 `HTTPS_PROXY`。本机网络到微软 CDN 会中途断流。
手动 `curl` 同一个 URL 能拿到 1.80 MB 的完整文件，但 `NsisConfig` 没有
「指定 bootstrapper 路径」的字段（schema 里 `embedBootstrapper` 只有 `silent`），
所以无法把手工下载的副本喂给 CLI。

**当前配置是 `downloadBootstrapper`**：安装包小 1.8 MB，安装时由 NSIS 自己联网拉
WebView2。Windows 11 自带 WebView2，实际不会触发下载；只有 Win10 才会。

### 换图标后必须清构建缓存

`tauri-build` 的 `rerun-if-changed` 不覆盖 `icons/` 目录。改完图标直接 `tauri build`
会得到一个仍然带着旧图标的 exe（实测：新 `icon.ico` 生成于 21:48，
exe 里的图标仍是 Tauri 默认图标，因为 `resource.lib` 停留在 21:19）。

正确顺序：

```bash
npx tauri icon path/to/logo.png
rm -rf src-tauri/target/release/build/mediaforge-v2-*
npm run tauri build
```

验证方式：从 exe 里提取图标看一眼，别只看 `icons/` 下的文件对不对。

要改回离线模式，在能稳定访问微软 CDN 的网络下把配置改回：

```json
"webviewInstallMode": { "type": "embedBootstrapper", "silent": true }
```

再跑一次 `npm run tauri build` 即可，代码不需要改动。
