# 打包便携版
#
# 便携版是一个免安装目录：exe 加 `ffmpeg/` 子目录，解压即用，不写注册表。
# 与安装包共用同一个 release exe，区别只在 ffmpeg 的位置：
# `paths.rs` 的第 2 条查找是 `<资源目录>/ffmpeg`，而 Tauri 对未安装的 exe
# 把资源目录解析为 exe 自身所在目录，所以把 ffmpeg 放在 exe 旁边即可命中，
# 不必设 `MEDIAFORGE_FFMPEG_DIR`。实测过：解压到临时目录直接双击，
# 界面显示 ffmpeg 版本与 233 个编码器，转换 mp3→ogg 产物可回读。
#
# 前提：先跑 `npm run tauri build`，release exe 已就位。
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File scripts/make-portable.ps1
#
# 产物：src-tauri/target/release/bundle/portable/MediaForge-<版本>-portable-x64.zip

$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$version = (Get-Content (Join-Path $root 'src-tauri/Cargo.toml') |
    Select-String -Pattern '^version = "(.+)"$').Matches[0].Groups[1].Value

$exe = Join-Path $root 'src-tauri/target/release/mediaforge-v2.exe'
if (-not (Test-Path $exe)) {
    throw "release exe 不存在，先跑 npm run tauri build：$exe"
}

$ffmpegBin = Join-Path $root 'vendor/ffmpeg/bin'
if (-not (Test-Path $ffmpegBin)) {
    throw "ffmpeg 未就绪，先跑 scripts/fetch-ffmpeg.ps1：$ffmpegBin"
}

$stage = Join-Path $root 'src-tauri/target/release/bundle/portable'
$dirName = "MediaForge-$version-portable-x64"
$stageDir = Join-Path $stage $dirName
$zipPath = Join-Path $stage "$dirName.zip"

if (Test-Path $stageDir) { Remove-Item $stageDir -Recurse -Force }
New-Item -ItemType Directory -Path (Join-Path $stageDir 'ffmpeg') -Force | Out-Null

Copy-Item $exe $stageDir
Copy-Item (Join-Path $ffmpegBin '*.exe') (Join-Path $stageDir 'ffmpeg')
Copy-Item (Join-Path $ffmpegBin '*.dll') (Join-Path $stageDir 'ffmpeg')
# 许可证必须随包分发，GPL 分发义务。
Copy-Item (Join-Path $root 'vendor/ffmpeg/LICENSE.txt') (Join-Path $stageDir 'ffmpeg')
Copy-Item (Join-Path $root 'vendor/ffmpeg/THIRD-PARTY.md') (Join-Path $stageDir 'ffmpeg')

if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
Compress-Archive -Path $stageDir -DestinationPath $zipPath -CompressionLevel Optimal

$sizeMb = [math]::Round((Get-Item $zipPath).Length / 1MB, 2)
Write-Host "portable bundle: $zipPath ($sizeMb MiB)"
