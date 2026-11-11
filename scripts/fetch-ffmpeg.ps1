# 取 ffmpeg（9 个文件，shared 版，约 181 MB）。
#
# 优先顺序：
#   1. -From 参数指定的目录（例如 v1 归档里的 tools/ffmpeg/bin）
#   2. 仓库同级的 mediaforge 目录（v1）
#   3. 从官方地址下载解压
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File scripts/fetch-ffmpeg.ps1
#   powershell -ExecutionPolicy Bypass -File scripts/fetch-ffmpeg.ps1 -From "D:\some\ffmpeg\bin"

param(
    [string]$From = "",
    [string]$Url = "https://github.com/FFmpeg-Builds/FFmpeg-Builds/releases/download/latest/ffmpeg-n9.0-latest-win64-gpl-shared-9.0.zip"
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$target = Join-Path $repoRoot "vendor\ffmpeg\bin"

# 需要的 9 个文件。DLL 名带版本号，换 ffmpeg 版本时要同步改。
$required = @(
    "ffmpeg.exe",
    "ffprobe.exe",
    "avcodec-63.dll",
    "avdevice-63.dll",
    "avfilter-12.dll",
    "avformat-63.dll",
    "avutil-61.dll",
    "swresample-7.dll",
    "swscale-10.dll"
)

function Test-Complete {
    param([string]$Dir)
    if (-not (Test-Path $Dir)) { return $false }
    foreach ($name in $required) {
        if (-not (Test-Path (Join-Path $Dir $name))) { return $false }
    }
    return $true
}

New-Item -ItemType Directory -Force -Path $target | Out-Null

Write-Host "目标目录：$target"

if (Test-Complete -Dir $target) {
    Write-Host "9 个文件已就位，跳过。" -ForegroundColor Green
    exit 0
}

# ---------- 1. 显式指定的来源 ----------
$candidates = @()
if ($From -ne "") { $candidates += $From }
$candidates += (Join-Path (Split-Path -Parent $repoRoot) "mediaforge\src\MediaForge.App\tools\ffmpeg\bin")

foreach ($source in $candidates) {
    if (Test-Complete -Dir $source) {
        Write-Host "从本地复制：$source"
        foreach ($name in $required) {
            Copy-Item -Force (Join-Path $source $name) (Join-Path $target $name)
        }
        Write-Host "完成。" -ForegroundColor Green
        exit 0
    }
}

# ---------- 2. 下载 ----------
Write-Host "本地未找到完整 ffmpeg，开始下载（约 180 MB）..."
Write-Host "来源：$Url"

$temp = Join-Path $env:TEMP ("mediaforge-ffmpeg-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $temp | Out-Null
$zip = Join-Path $temp "ffmpeg.zip"

try {
    # 走系统代理（如果设了 HTTPS_PROXY）
    Invoke-WebRequest -Uri $Url -OutFile $zip -UseBasicParsing
    Write-Host "解压..."
    Expand-Archive -Path $zip -DestinationPath $temp -Force

    # 官方 zip 里是 ffmpeg-<ver>-win64-gpl-shared-9.0\bin\*.exe|dll
    $found = Get-ChildItem -Path $temp -Recurse -Directory |
        Where-Object { $_.Name -eq "bin" } |
        Select-Object -First 1

    if (-not $found) {
        throw "解压后找不到 bin 目录"
    }

    foreach ($name in $required) {
        $source = Join-Path $found.FullName $name
        if (-not (Test-Path $source)) {
            throw "压缩包里缺少 $name"
        }
        Copy-Item -Force $source (Join-Path $target $name)
    }

    # LICENSE 一并带上
    $license = Get-ChildItem -Path $found.FullName -Filter "LICENSE*" | Select-Object -First 1
    if ($license) {
        Copy-Item -Force $license.FullName (Join-Path $repoRoot "vendor\ffmpeg\LICENSE.txt")
    }

    Write-Host "完成。" -ForegroundColor Green
}
finally {
    Remove-Item -Recurse -Force $temp -ErrorAction SilentlyContinue
}

if (-not (Test-Complete -Dir $target)) {
    throw "复制后仍缺文件，请检查 $target"
}
