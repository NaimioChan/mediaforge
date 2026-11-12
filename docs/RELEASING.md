# 发布流程

面向维护者。这里规定版本号怎么走、分支怎么用、一次发布要做哪些事。

## 分支模型

单人维护的桌面应用，用两层分支就够：`main` 恒等于最近一次发布，
`dev` 是唯一的日常开发分支。

```
main    ──●──────────────●──────────────●──   （只有发布提交与热修）
           ╲            ╱ ╲            ╱
dev         ●──●──●──●─╯   ●──●──●──●─╯        （日常开发）
             v2.1.0          v2.2.0
```

**为什么 `main` 不直接开发。** 发布出去的东西必须能随时重建。
`main` 上任何一次提交都应该对应一个可用的版本，这样出问题时能立刻
`git checkout` 回去打包，不用猜「哪个提交是好的」。

**日常在 `dev` 上做。** 一个版本的所有改动（新格式、修 bug、文档）
都落在 `dev`，攒够一批再合并。`dev` 上允许存在半成品，允许测试是红的。

**发布时合并到 `main` 并打 tag。** 合并用一个提交，提交信息写清这版做了什么。
tag 打在 `main` 的发布提交上，不打在 `dev`。

**热修走 `main` 上的短分支。** 线上版本出现严重问题（产物损坏、启动失败）时，
从 tag 拉一个 `hotfix/x.y.z`，修完直接合 `main` 发版，再 `git merge main` 回 `dev`，
避免 `dev` 上已有的改动被这版带上。

### 什么时候不适用

- 只改文档、改 CI、改 `.gitignore`：直接提交 `main`，不需要走 `dev`。
  这类改动不进安装包，不需要版本号。
- 改动小到一次提交能做完且测试全绿：可以直接提交 `main` 再 `git merge main` 到 `dev`。
  但一旦开始有第二个提交，就应当用 `dev`。

### 不要做的事

- 不要在 `main` 上 amend 已推送的提交。用户已经下载了那个版本，
  提交历史改掉之后重建出来的东西和发出去的对不上。
- 不要给 `dev` 打 tag。tag 是发布标记，只出现在 `main`。
- 不要长期不合并。`dev` 领先 `main` 几十个提交之后，一次合并会很难验证。

## 版本号

语义化版本，三段：

| 变化 | 版本位 | 例子 |
|---|---|---|
| 修 bug，行为不变 | patch | 2.0.0 → 2.0.1 |
| 加格式、加参数、加功能（向后兼容） | minor | 2.0.1 → 2.1.0 |
| 破坏性变更 | major | 2.1.0 → 3.0.0 |

破坏性变更的判据：`settings.json` 结构不兼容、安装路径变化、
`identifier` 改动、已有格式的产物行为改变到用户会察觉。

`identifier`（`com.naimio.mediaforge`）一旦发布就不再改。改它等于让老用户
装出第二个应用，旧版本还留在机器上。

### 版本号要改的地方

改版本号时三处必须同步，漏一处会打出前后不一致的包：

1. `package.json` 的 `version`
2. `src-tauri/Cargo.toml` 的 `version`
3. `src-tauri/tauri.conf.json` 的 `version`

界面右下角的版本号取自 `Cargo.toml`（`env!("CARGO_PKG_VERSION")`），
安装包文件名取自 `tauri.conf.json`，两者不一致时安装包名和界面显示会打架。

## 一次发布的完整流程

```bash
# 1. 在 dev 上确认状态干净、测试全绿
git switch dev
git status -sb
cargo test --manifest-path src-tauri/Cargo.toml

# 2. 改三处版本号，更新 CHANGELOG
#    CHANGELOG 把「未发布」改成新版本号与日期

# 3. 提交版本号
git commit -am "chore: 发布 v2.1.0"

# 4. 合并到 main
git switch main
git merge --no-ff dev -m "merge: v2.1.0"

# 5. 打包并安装验证
npm run tauri build
# 安装 src-tauri/target/release/bundle/nsis/MediaForge_2.1.0_x64-setup.exe
# 启动 → 转一个文件 → 回读产物 → 卸载

# 6. 打 tag 并推送
git tag -a v2.1.0 -m "v2.1.0"
git push origin main --tags

# 7. 建 GitHub Release，上传安装包，正文从 CHANGELOG 抄
gh release create v2.1.0 \
  "src-tauri/target/release/bundle/nsis/MediaForge_2.1.0_x64-setup.exe" \
  --title "v2.1.0" --notes-file <(sed -n '/## \[2.1.0\]/,/^## \[/p' CHANGELOG.md)

# 8. dev 跟上 main
git switch dev
git merge main
git push origin dev
```

### 第 5 步不能省

`cargo build --release` 的 exe 指向 `devUrl`，没有注入前端资源，
复制到安装目录会得到空白窗口。验证必须用 `npm run tauri build` 出的 NSIS 包，
页面 origin 是 `tauri.localhost`。

装完点一遍的清单：

- 启动无报错，ffmpeg 版本与编码器数量正常
- 拖一个文件进去，转成 mp4 与 mp3，产物能被播放器打开
- 取消一个正在跑的任务，确认目录里没留 0 字节残片
- 右下角版本按钮显示新版本号
- 卸载后安装目录清空

## 发布说明写什么

从用户视角写，不写内部实现。三段：

1. 这一版能做什么新的，或修了什么用户能感觉到的问题
2. 已知问题
3. 升级说明（若设置结构变了、需要重新配置，写清）

「重构了参数面板」「升级依赖」这类内容不进发布说明，进 CHANGELOG 的
对应条目即可。

## ffmpeg 升级

换 ffmpeg 版本是一次单独的发布，不和其他改动混在一起。步骤：

1. 改 `scripts/fetch-ffmpeg.ps1` 里钉住的 autobuild tag
2. 删掉 `vendor/ffmpeg/bin/` 下的旧文件，重跑脚本
3. 按新版本改 `tauri.conf.json` 的 `bundle.resources`（DLL 名带版本号）
4. 更新 `vendor/ffmpeg/THIRD-PARTY.md` 的版本与源码地址
5. 重测直拷矩阵：`bash scripts/copy-matrix.sh` 然后
   `python scripts/check-copy-table.py <matrix.tsv>`，必须「严重 0 项」
6. 跑全部测试，特别是 `formats_sweep`
7. 装包实测一轮

第 5 步最容易漏。`copy.rs` 里的表是按具体版本实测出来的，
新版本的复用器接受范围可能变，不重测就会在某些组合上产出失败的文件。

## 不要做的自动化

**暂不接自动更新。** 自动更新需要一对签名密钥、一个静态清单、
以及发版时更新清单的流程。密钥丢失就再也推不了更新，清单地址挂掉
应用启动就报错。当前的做法是启动时查一次 GitHub Releases，
有新版本在右下角提示，用户自己去下载。

等用户量上来、更新频率变高之后再考虑接入 `tauri-plugin-updater`，
届时把密钥与清单流程单独写进本文档。
