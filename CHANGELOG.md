# 变更记录

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循[语义化版本](https://semver.org/lang/zh-CN/)。

## [未发布]

## [2.0.0] - 2026-11-12

首个公开发布版本。

### 新增

- 71 种目标格式，覆盖视频、音频、图像、字幕四类
- 队列式批量转换，可调并发数
- 按目标格式动态生成的参数面板，只列出当前 ffmpeg 构建真实支持的选项
- 容器×码流直拷兼容表，容器不收该码流时自动回退重编码并说明原因
- 音频码率按编码器与声道数自动夹紧，夹紧事实写进界面提示
- 右下角版本按钮，启动时检查 GitHub Releases，有新版本时提示
- 输出目录可手输，支持「复制为路径」带出的引号与尾部分隔符

### 设计约束

- `args.rs` 为纯函数，不启动进程、不读写文件；有一条测试读源码断言
- 端到端测试真跑 ffmpeg 并回读产物，找不到 ffmpeg 直接失败，不静默跳过
- 取消转换时删除本次创建的目标文件，不留半成品
- 输出目录不跨会话记忆，每次启动回到「与源文件同目录」

[未发布]: https://github.com/NaimioChan/mediaforge/compare/v2.0.0...HEAD
[2.0.0]: https://github.com/NaimioChan/mediaforge/releases/tag/v2.0.0
