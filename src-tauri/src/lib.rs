//! MediaForge v2 —— ffmpeg 的可视化外壳。
//!
//! 队列与任务状态由后端持有，前端只是视图。理由：
//! ffmpeg 是子进程，前端刷新或重载不该丢掉正在跑的任务；
//! 而且参数构建与能力判断都在 Rust，前端再存一份必然漂移。
//!
//! 模块划分（纯函数在前，有 IO 的在后）：
//! * `formats`       格式表（71 个）—— 纯数据
//! * `args`          参数构建 —— 纯函数，不碰进程与文件系统
//! * `compat`        跨类型兼容矩阵 —— 纯函数
//! * `progress`      `-progress` 解析 —— 纯函数
//! * `caps`          能力清单解析 —— 纯函数
//! * `probe`         ffprobe 输出解析 —— 纯函数
//! * `subtitle_text` MicroDVD / SBV 写入器 —— 纯函数
//! * `output_path`   输出路径推导 —— 纯函数
//! * `paths`         定位随包 ffmpeg —— 有 IO
//! * `settings`      设置持久化 —— 有 IO
//! * `runner`        子进程驱动、进度采集、取消 —— 有 IO
//! * 本文件          命令、状态、事件 —— 装配层

pub mod args;
pub mod caps;
pub mod compat;
pub mod copy;
pub mod formats;
pub mod output_path;
pub mod params;
pub mod paths;
pub mod probe;
pub mod progress;
pub mod runner;
pub mod settings;
pub mod subtitle_text;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{Emitter, Manager, State};

use crate::formats::{Format, Kind};
use crate::paths::ToolPaths;
use crate::probe::Probe;
use crate::runner::Outcome;
use crate::settings::Settings;

// ---------- 对外数据结构 ----------

/// 一条任务的当前状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    /// 等待开始；也用于「上次跑完，可重跑」。
    Pending,
    Running,
    Done,
    Failed,
    Cancelled,
    /// 建任务时就判定为无意义组合（例如音频转视频），或目标格式缺能力。
    Incompatible,
}

/// 一条转换任务。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: u64,
    pub source_path: String,
    pub file_name: String,
    /// 源文件的媒体类型。
    pub source_kind: Kind,
    /// 源文件的一句话摘要，例如「1920×1080 · h264 · 30fps」。
    pub source_summary: String,
    pub duration_secs: f64,
    pub source_size: u64,
    /// 当前目标格式 id。
    pub target_format: String,
    pub status: TaskStatus,
    /// 0–100。
    pub percent: f64,
    /// 编码速度倍数。
    pub speed: Option<f64>,
    pub output_path: String,
    pub output_size: u64,
    /// 失败或不可兼容时的原因。
    pub message: String,
    /// 自动降级等提示。
    pub notice: String,
    /// 这条任务的参数细调。切换目标格式时会被裁剪成该格式认识的部分。
    pub tune: crate::params::Tune,
}

/// 给前端的完整快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub tasks: Vec<Task>,
    pub formats: Vec<FormatView>,
    pub capabilities: CapsView,
    pub settings: Settings,
    pub ffmpeg_found: bool,
    pub is_running: bool,
}

/// 格式的界面视图。`available` 为 false 时界面置灰。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatView {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: Kind,
    pub ext: &'static str,
    pub animated: bool,
    pub notes: &'static str,
    /// 该格式在当前 ffmpeg 构建里是否可用。
    pub available: bool,
    /// 不可用的原因（例如缺 muxer）。
    pub unavailable_reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapsView {
    pub encoder_count: usize,
    pub muxer_count: usize,
    pub ffmpeg_version: String,
}

/// 任务变化的推送载荷。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskUpdate {
    pub id: u64,
    pub status: TaskStatus,
    pub percent: f64,
    pub speed: Option<f64>,
    pub output_path: String,
    pub output_size: u64,
    pub message: String,
    pub notice: String,
}

// ---------- 应用状态 ----------

/// 队列与设置。用 `Arc<Mutex<_>>` 包起来，以便后台线程共享。
struct Inner {
    tasks: Vec<Task>,
    probes: HashMap<u64, Probe>,
    settings: Settings,
    settings_path: PathBuf,
    next_id: u64,
    cancel: Option<Arc<AtomicBool>>,
    running: bool,
}

/// Tauri 托管状态。
pub struct AppState {
    inner: Arc<Mutex<Inner>>,
    tools: Mutex<Option<ToolPaths>>,
    caps: Mutex<Option<caps::Caps>>,
    version: Mutex<String>,
}

impl AppState {
    fn new(settings_path: PathBuf, settings: Settings, tools: Option<ToolPaths>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                tasks: Vec::new(),
                probes: HashMap::new(),
                settings,
                settings_path,
                next_id: 1,
                cancel: None,
                running: false,
            })),
            tools: Mutex::new(tools),
            caps: Mutex::new(None),
            version: Mutex::new(String::new()),
        }
    }
}

/// 取锁；锁中毒时恢复继续用，避免一处 panic 让整个界面卡死。
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ---------- 纯逻辑 ----------

/// 判定某个格式在当前能力下是否可用。
pub fn format_availability(target: &Format, caps: &caps::Caps) -> (bool, String) {
    // 本程序自己写文本的格式不依赖 ffmpeg 能力。
    if matches!(
        target.pipeline,
        formats::Pipeline::MicroDvdText | formats::Pipeline::SbvText
    ) {
        return (true, String::new());
    }

    if !target.muxer.is_empty() && !caps.has_muxer(target.muxer) {
        return (false, format!("当前 ffmpeg 缺少 {} 复用器", target.muxer));
    }

    let no_encoder_declared = target.video_encoders.is_empty() && target.audio_encoders.is_empty();
    let has_encoder = no_encoder_declared
        || target
            .video_encoders
            .iter()
            .chain(target.audio_encoders.iter())
            .any(|name| caps.has_encoder(name));

    if !has_encoder {
        return (false, "当前 ffmpeg 缺少所需编码器".into());
    }

    (true, String::new())
}

/// 源摘要在任务列表里显示。
fn summarize(probe: &Probe) -> String {
    if let Some(v) = probe.video() {
        v.describe()
    } else if let Some(a) = probe.audio() {
        a.describe()
    } else if let Some(s) = probe.subtitle() {
        s.describe()
    } else {
        "无法识别的媒体".to_string()
    }
}

/// 按源类型挑一个合理的默认目标格式。
fn default_target_for(kind: Kind, caps: &caps::Caps) -> &'static str {
    let preferred: &[&str] = match kind {
        Kind::Video => &["mp4", "mkv", "webm"],
        Kind::Audio => &["mp3", "m4a", "flac"],
        Kind::Image => &["png", "jpg", "webp"],
        Kind::Subtitle => &["srt", "vtt", "ass"],
    };

    for id in preferred {
        if let Some(f) = formats::by_id(id)
            && format_availability(f, caps).0
        {
            return f.id;
        }
    }

    // 全部不可用时退回该类别的第一个格式，让用户看到并自行改。
    formats::FORMATS
        .iter()
        .find(|f| f.kind == kind)
        .map(|f| f.id)
        .unwrap_or("mp4")
}

/// 建一条任务。探测结果由调用方提供。
fn make_task(id: u64, source_path: String, probe: &Probe, caps: &caps::Caps) -> Task {
    let source_kind = probe.kind();
    let target_id = default_target_for(source_kind, caps);
    let file_name = Path::new(&source_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| source_path.clone());

    let (status, message) = if probe.streams.is_empty() {
        (TaskStatus::Incompatible, "探测不到任何媒体流".to_string())
    } else {
        (TaskStatus::Pending, String::new())
    };

    Task {
        id,
        source_path,
        file_name,
        source_kind,
        source_summary: summarize(probe),
        duration_secs: probe.duration_secs,
        source_size: probe.size_bytes,
        target_format: target_id.to_string(),
        status,
        percent: 0.0,
        speed: None,
        output_path: String::new(),
        output_size: 0,
        message,
        notice: String::new(),
        tune: crate::params::Tune::default(),
    }
}

/// 切换目标格式后重新判定兼容性。
pub fn revalidate(task: &mut Task, target: &Format, caps: &caps::Caps) {
    task.percent = 0.0;
    task.speed = None;
    task.output_path.clear();
    task.output_size = 0;
    task.notice.clear();

    let verdict = compat::judge(task.source_kind, target.kind);
    if !verdict.ok {
        task.status = TaskStatus::Incompatible;
        task.message = verdict.reason.to_string();
        return;
    }

    let (available, reason) = format_availability(target, caps);
    if !available {
        task.status = TaskStatus::Incompatible;
        task.message = reason;
        return;
    }

    // 上一轮跑完的任务允许重跑，状态回到等待。
    task.status = TaskStatus::Pending;
    task.message.clear();
}

// ---------- 文件收集 ----------

/// 展开用户给的一批路径：目录递归展开，文件直接收。
/// 只收已知媒体扩展名，避免把整个目录里的杂物塞进队列。
#[cfg(test)]
fn collect_media_paths(inputs: &[String], max_depth: usize) -> Vec<PathBuf> {
    collect_media_paths_excluding(inputs, max_depth, None)
}

/// 与 `collect_media_paths` 相同，但跳过 `exclude_dir` 及其子目录。
///
/// 用途：用户把输出目录放在源目录里（默认就是如此）时，
/// 再拖一次同一个文件夹会把上次的产物当成新的源文件加进队列，
/// 于是「转一次」变成「每次多几条」。排除输出目录即可避免。
fn collect_media_paths_excluding(
    inputs: &[String],
    max_depth: usize,
    exclude_dir: Option<&Path>,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();

    for input in inputs {
        let path = PathBuf::from(input);
        if path.is_file() {
            out.push(path);
        } else if path.is_dir() {
            collect_dir(&path, 0, max_depth, exclude_dir, &mut out);
        }
    }

    // 去重并保持顺序。
    let mut seen: Vec<String> = Vec::new();
    out.retain(|p| {
        let key = p.to_string_lossy().to_lowercase();
        if seen.contains(&key) {
            false
        } else {
            seen.push(key);
            true
        }
    });

    out
}

fn collect_dir(
    dir: &Path,
    depth: usize,
    max_depth: usize,
    exclude_dir: Option<&Path>,
    out: &mut Vec<PathBuf>,
) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    let mut files: Vec<PathBuf> = Vec::new();
    let mut dirs: Vec<PathBuf> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if is_excluded(&path, exclude_dir) {
                continue;
            }
            dirs.push(path);
        } else if path.is_file() && is_media_extension(&path) {
            files.push(path);
        }
    }

    files.sort();
    dirs.sort();
    out.extend(files);
    for sub in dirs {
        collect_dir(&sub, depth + 1, max_depth, exclude_dir, out);
    }
}

/// 目录是否等于（或是其子目录）被排除的目录。
fn is_excluded(path: &Path, exclude: Option<&Path>) -> bool {
    let Some(exclude) = exclude else {
        return false;
    };
    if exclude.as_os_str().is_empty() {
        return false;
    }
    let path_text = path.to_string_lossy().to_lowercase();
    let exclude_text = exclude.to_string_lossy().to_lowercase();
    path_text == exclude_text || path_text.starts_with(&format!("{exclude_text}\\"))
}

fn is_media_extension(path: &Path) -> bool {
    let Some(ext) = path.extension() else {
        return false;
    };
    let ext = ext.to_string_lossy().to_ascii_lowercase();
    KNOWN_EXTENSIONS.contains(&ext.as_str())
}

/// 已知媒体扩展名。目录扫描时用来过滤。
const KNOWN_EXTENSIONS: &[&str] = &[
    // 视频
    "mp4", "mkv", "webm", "mov", "avi", "wmv", "flv", "ts", "m2ts", "mts", "mpg", "mpeg", "vob",
    "3gp", "ogv", "nut", "ivf", "mxf", "dv", "m4v", "rmvb", "asf", "h264", "h265", "hevc", "gif",
    // 音频
    "mp3", "m4a", "aac", "flac", "wav", "aiff", "aif", "alac", "opus", "ogg", "wma", "ac3", "eac3",
    "dts", "tta", "wv", "ape", "amr", "mka", //
    // 图像
    "png", "jpg", "jpeg", "webp", "avif", "jxl", "tif", "tiff", "bmp", "tga", "ico", "pcx", "qoi",
    "exr", "hdr", "dpx", "jp2", "j2k", "jls", "pam", "pgm", "ppm", "pbm", "pfm", "sgi", "ras",
    "xbm", "wbmp", "fits", "apng", "heic", "heif", //
    // 字幕
    "srt", "ass", "ssa", "vtt", "ttml", "sub", "sbv", "lrc", "smi", "dfxp",
];

// ---------- Tauri 命令 ----------

/// 首次加载的完整快照。
#[tauri::command]
fn get_snapshot(state: State<'_, AppState>) -> Snapshot {
    let (tasks, settings, is_running) = {
        let inner = lock(&state.inner);
        (inner.tasks.clone(), inner.settings.clone(), inner.running)
    };

    let caps_guard = lock(&state.caps);
    let caps_known = caps_guard.is_some();
    let all_caps = caps_guard.clone().unwrap_or_default();
    drop(caps_guard);

    let caps_view = CapsView {
        encoder_count: all_caps.encoder_count(),
        muxer_count: all_caps.muxer_count(),
        ffmpeg_version: lock(&state.version).clone(),
    };

    let formats: Vec<FormatView> = formats::FORMATS
        .iter()
        .map(|f| {
            let (available, reason) = if caps_known {
                format_availability(f, &all_caps)
            } else {
                (true, String::new())
            };
            FormatView {
                id: f.id,
                name: f.name,
                kind: f.kind,
                ext: f.ext,
                animated: f.animated,
                notes: f.notes,
                available,
                unavailable_reason: reason,
            }
        })
        .collect();

    let ffmpeg_found = lock(&state.tools).is_some();

    Snapshot {
        tasks,
        formats,
        capabilities: caps_view,
        settings,
        ffmpeg_found,
        is_running,
    }
}

/// 加一批路径入队（文件或目录）。
#[tauri::command]
fn add_paths(paths: Vec<String>, state: State<'_, AppState>) -> Result<Vec<Task>, String> {
    let tools = lock(&state.tools)
        .clone()
        .ok_or("找不到 ffmpeg，无法探测媒体文件")?;
    let caps = lock(&state.caps).clone().unwrap_or_default();

    // 输出目录里的产物不算源文件。
    let output_dir = {
        let inner = lock(&state.inner);
        let dir = inner.settings.output_dir.clone();
        if dir.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(dir))
        }
    };

    let candidates = collect_media_paths_excluding(&paths, 4, output_dir.as_deref());
    if candidates.is_empty() {
        return Err("没有找到可识别的媒体文件".into());
    }

    let mut created: Vec<Task> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    for path in candidates {
        let path_text = path.to_string_lossy().to_string();

        let json = match runner::run_ffprobe(&tools.ffprobe, &path) {
            Ok(j) => j,
            Err(e) => {
                failures.push(format!("{}（{e}）", short_name(&path)));
                continue;
            }
        };

        let probe = match probe::parse(&json, &path_text) {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("{}（{e}）", short_name(&path)));
                continue;
            }
        };

        let task = {
            let mut inner = lock(&state.inner);
            let id = inner.next_id;
            inner.next_id += 1;
            inner.probes.insert(id, probe.clone());
            let mut task = make_task(id, path_text, &probe, &caps);
            inner.tasks.push(task.clone());
            task.status = task_status(&inner, id).unwrap_or(task.status);
            task
        };

        // 目标格式本身不可用时直接标出来，别让用户点了开始才失败。
        if let Some(target) = formats::by_id(&task.target_format) {
            let (available, reason) = format_availability(target, &caps);
            if !available {
                let mut inner = lock(&state.inner);
                if let Some(stored) = inner.tasks.iter_mut().find(|t| t.id == task.id) {
                    stored.status = TaskStatus::Incompatible;
                    stored.message = reason.clone();
                }
            }
        }

        // 回读最终落盘状态，保证返回值与队列一致。
        let final_task = {
            let inner = lock(&state.inner);
            inner.tasks.iter().find(|t| t.id == task.id).cloned()
        };
        if let Some(t) = final_task {
            created.push(t);
        }
    }

    if created.is_empty() {
        return Err(format!("没有可转换的文件：{}", failures.join("；")));
    }

    // 部分文件被跳过时，挂在最后一条任务上提示。
    if !failures.is_empty()
        && let Some(last) = created.last()
    {
        let id = last.id;
        let notice = format!("{} 个文件被跳过", failures.len());
        let mut inner = lock(&state.inner);
        if let Some(stored) = inner.tasks.iter_mut().find(|t| t.id == id) {
            stored.notice = notice.clone();
        }
        if let Some(returned) = created.last_mut() {
            returned.notice = notice;
        }
    }

    Ok(created)
}

/// 读一条任务的当前状态。
fn task_status(inner: &Inner, id: u64) -> Option<TaskStatus> {
    inner.tasks.iter().find(|t| t.id == id).map(|t| t.status)
}

fn short_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// 取某条任务可用的目标格式（已按兼容性过滤）。
#[tauri::command]
fn compatible_formats(task_id: u64, state: State<'_, AppState>) -> Result<Vec<FormatView>, String> {
    let source_kind = {
        let inner = lock(&state.inner);
        inner
            .tasks
            .iter()
            .find(|t| t.id == task_id)
            .map(|t| t.source_kind)
            .ok_or_else(|| format!("任务 {task_id} 不存在"))?
    };
    let caps = lock(&state.caps).clone().unwrap_or_default();

    Ok(formats::FORMATS
        .iter()
        .filter(|f| compat::is_supported(source_kind, f.kind))
        .map(|f| {
            let (available, reason) = format_availability(f, &caps);
            FormatView {
                id: f.id,
                name: f.name,
                kind: f.kind,
                ext: f.ext,
                animated: f.animated,
                notes: f.notes,
                available,
                unavailable_reason: reason,
            }
        })
        .collect())
}

/// 改某条任务的目标格式。
#[tauri::command]
fn set_target(task_id: u64, format_id: String, state: State<'_, AppState>) -> Result<Task, String> {
    let target = formats::by_id(&format_id).ok_or_else(|| format!("未知格式 {format_id}"))?;
    let caps = lock(&state.caps).clone().unwrap_or_default();

    let mut inner = lock(&state.inner);
    let task = inner
        .tasks
        .iter_mut()
        .find(|t| t.id == task_id)
        .ok_or_else(|| format!("任务 {task_id} 不存在"))?;

    if task.status == TaskStatus::Running {
        return Err("该任务正在转换，无法改目标格式".into());
    }

    task.target_format = format_id;
    revalidate(task, target, &caps);
    // 新格式可能不认识旧参数（例如从 MP4 切到 FLAC 时残留的视频编码器），
    // 切完立刻裁掉，避免面板显示不存在的选项。
    task.tune = task.tune.clone().sanitize(target, &caps);
    Ok(task.clone())
}

/// 某个格式的可调参数规格。界面据此动态渲染面板。
#[tauri::command]
fn param_specs(
    format_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<params::ParamSpec>, String> {
    let target = formats::by_id(&format_id).ok_or_else(|| format!("未知格式 {format_id}"))?;
    let caps = lock(&state.caps).clone().unwrap_or_default();
    Ok(params::specs_for(target, &caps))
}

/// 改某条任务的参数细调。返回裁剪后的任务，界面据此刷新面板。
#[tauri::command]
fn set_tune(task_id: u64, tune: params::Tune, state: State<'_, AppState>) -> Result<Task, String> {
    let caps = lock(&state.caps).clone().unwrap_or_default();
    let mut inner = lock(&state.inner);
    let task = inner
        .tasks
        .iter_mut()
        .find(|t| t.id == task_id)
        .ok_or_else(|| format!("任务 {task_id} 不存在"))?;

    if task.status == TaskStatus::Running {
        return Err("该任务正在转换，无法改参数".into());
    }

    let Some(target) = formats::by_id(&task.target_format) else {
        return Err("未知目标格式".into());
    };

    task.tune = tune.sanitize(target, &caps);
    // 参数变了，上一次的产物与提示都作废。
    task.output_path.clear();
    task.output_size = 0;
    task.notice.clear();
    if task.status != TaskStatus::Incompatible {
        task.status = TaskStatus::Pending;
    }
    Ok(task.clone())
}

/// 批量设置目标格式。只影响非运行中的任务。
#[tauri::command]
fn set_target_bulk(
    task_ids: Vec<u64>,
    format_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<Task>, String> {
    let target = formats::by_id(&format_id).ok_or_else(|| format!("未知格式 {format_id}"))?;
    let caps = lock(&state.caps).clone().unwrap_or_default();

    let mut inner = lock(&state.inner);
    let mut updated = Vec::new();

    for id in task_ids {
        let Some(task) = inner.tasks.iter_mut().find(|t| t.id == id) else {
            continue;
        };
        if task.status == TaskStatus::Running {
            continue;
        }
        task.target_format = format_id.clone();
        revalidate(task, target, &caps);
        task.tune = task.tune.clone().sanitize(target, &caps);
        updated.push(task.clone());
    }

    Ok(updated)
}

/// 移除任务。正在跑的不允许移除。
#[tauri::command]
fn remove_tasks(task_ids: Vec<u64>, state: State<'_, AppState>) -> Result<Vec<u64>, String> {
    let mut inner = lock(&state.inner);

    let running: Vec<u64> = inner
        .tasks
        .iter()
        .filter(|t| task_ids.contains(&t.id) && t.status == TaskStatus::Running)
        .map(|t| t.id)
        .collect();

    if !running.is_empty() {
        return Err(format!("任务 {running:?} 正在转换，请先取消"));
    }

    inner.tasks.retain(|t| !task_ids.contains(&t.id));
    for id in &task_ids {
        inner.probes.remove(id);
    }
    Ok(task_ids)
}

/// 清空全部任务。
#[tauri::command]
fn clear_tasks(state: State<'_, AppState>) -> Result<(), String> {
    let mut inner = lock(&state.inner);
    if inner.running {
        return Err("正在转换，请先取消".into());
    }
    inner.tasks.clear();
    inner.probes.clear();
    Ok(())
}

/// 保存设置。
#[tauri::command]
fn save_settings(new_settings: Settings, state: State<'_, AppState>) -> Result<Settings, String> {
    let path = {
        let mut inner = lock(&state.inner);
        inner.settings = new_settings.clone();
        inner.settings_path.clone()
    };

    new_settings
        .save(&path)
        .map_err(|e| format!("设置写入失败：{e}"))?;

    Ok(new_settings)
}

/// 开始转换。立刻返回，进度走事件推送。
#[tauri::command]
fn start(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let tools = lock(&state.tools).clone().ok_or("找不到 ffmpeg")?;
    let caps = lock(&state.caps).clone().unwrap_or_default();
    let cancel_flag = Arc::new(AtomicBool::new(false));

    let (batch, inner_arc) = {
        let mut inner = lock(&state.inner);
        if inner.running {
            return Err("已有转换在进行".into());
        }

        // 待办：等待中，或上一轮结束但可以重跑的。
        let ids: Vec<u64> = inner
            .tasks
            .iter()
            .filter(|t| {
                matches!(
                    t.status,
                    TaskStatus::Pending
                        | TaskStatus::Done
                        | TaskStatus::Failed
                        | TaskStatus::Cancelled
                )
            })
            .map(|t| t.id)
            .collect();

        if ids.is_empty() {
            return Err("没有可转换的任务".into());
        }

        for task in inner.tasks.iter_mut() {
            if ids.contains(&task.id) {
                task.status = TaskStatus::Pending;
                task.percent = 0.0;
                task.speed = None;
                task.message.clear();
            }
        }

        inner.running = true;
        inner.cancel = Some(cancel_flag.clone());
        (ids, state.inner.clone())
    };

    let app_handle = app.clone();
    std::thread::spawn(move || {
        run_batch(app_handle, inner_arc, tools, caps, batch, cancel_flag);
    });

    Ok(())
}

/// 请求取消当前批次。
#[tauri::command]
fn cancel(state: State<'_, AppState>) -> Result<(), String> {
    let inner = lock(&state.inner);
    match &inner.cancel {
        Some(flag) => {
            flag.store(true, Ordering::SeqCst);
            Ok(())
        }
        None => Err("当前没有正在进行的转换".into()),
    }
}

// ---------- 批次执行 ----------

fn run_batch(
    app: tauri::AppHandle,
    inner: Arc<Mutex<Inner>>,
    tools: ToolPaths,
    caps: caps::Caps,
    batch: Vec<u64>,
    cancel: Arc<AtomicBool>,
) {
    for id in batch {
        if cancel.load(Ordering::SeqCst) {
            mark(&inner, id, TaskStatus::Cancelled, "已取消");
            emit(&app, &inner, id);
            continue;
        }

        // 一次性取出这条任务需要的全部数据，避免持锁跑 ffmpeg。
        let Some((source_path, format_id, probe, options, output_dir, overwrite)) =
            prepare(&inner, id)
        else {
            mark(&inner, id, TaskStatus::Failed, "缺少任务数据");
            emit(&app, &inner, id);
            continue;
        };

        let Some(target) = formats::by_id(&format_id) else {
            mark(&inner, id, TaskStatus::Failed, "未知目标格式");
            emit(&app, &inner, id);
            continue;
        };

        let out_path = output_path::derive_output_path(
            Path::new(&source_path),
            Some(output_dir.as_str()),
            target.ext,
            overwrite,
        );

        let plan = match args::build_plan(
            &source_path,
            &out_path.to_string_lossy(),
            target,
            &options,
            &probe,
            &caps,
        ) {
            Ok(p) => p,
            Err(e) => {
                mark(&inner, id, TaskStatus::Failed, &e);
                emit(&app, &inner, id);
                continue;
            }
        };

        // 先切到运行中，界面好显示进度条。
        {
            let mut guard = lock(&inner);
            if let Some(task) = guard.tasks.iter_mut().find(|t| t.id == id) {
                task.status = TaskStatus::Running;
                task.percent = 0.0;
                task.output_path = plan.output_path.clone();
                if let Some(n) = &plan.notice {
                    task.notice = n.clone();
                }
            }
        }
        emit(&app, &inner, id);

        let outcome = match plan.direct_write {
            Some(kind) => write_subtitle_directly(&tools, &source_path, &plan.output_path, kind),
            None => {
                let app_cb = app.clone();
                let inner_cb = inner.clone();
                let progress_cb: runner::ProgressFn = Arc::new(move |percent, speed| {
                    {
                        let mut guard = lock(&inner_cb);
                        if let Some(task) = guard.tasks.iter_mut().find(|t| t.id == id) {
                            task.percent = percent;
                            task.speed = speed;
                        }
                    }
                    let _ = app_cb.emit(
                        "task-progress",
                        TaskUpdate {
                            id,
                            status: TaskStatus::Running,
                            percent,
                            speed,
                            output_path: String::new(),
                            output_size: 0,
                            message: String::new(),
                            notice: String::new(),
                        },
                    );
                });

                runner::run_ffmpeg(
                    &tools.ffmpeg,
                    &plan.args,
                    Path::new(&plan.output_path),
                    probe.duration_secs,
                    &cancel,
                    Some(progress_cb),
                )
            }
        };

        match outcome {
            Outcome::Done => {
                let size = std::fs::metadata(&plan.output_path)
                    .map(|m| m.len())
                    .unwrap_or(0);
                {
                    let mut guard = lock(&inner);
                    if let Some(task) = guard.tasks.iter_mut().find(|t| t.id == id) {
                        task.status = TaskStatus::Done;
                        task.percent = 100.0;
                        task.speed = None;
                        task.output_size = size;
                        task.message.clear();
                        if let Some(n) = &plan.notice {
                            task.notice = n.clone();
                        }
                    }
                }
            }
            Outcome::Cancelled => {
                mark(&inner, id, TaskStatus::Cancelled, "已取消");
            }
            Outcome::Failed(reason) => {
                mark(&inner, id, TaskStatus::Failed, &reason);
            }
        }
        emit(&app, &inner, id);
    }

    {
        let mut guard = lock(&inner);
        guard.running = false;
        guard.cancel = None;
    }

    let _ = app.emit("batch-finished", ());
}

/// 把一条任务执行所需的全部数据取出来（一次性持锁）。
#[allow(clippy::type_complexity)]
fn prepare(
    inner: &Arc<Mutex<Inner>>,
    id: u64,
) -> Option<(String, String, Probe, args::Options, String, bool)> {
    let guard = lock(inner);
    let task = guard.tasks.iter().find(|t| t.id == id)?;
    let probe = guard.probes.get(&id)?.clone();
    // 任务的细调覆盖全局默认值，两者合成后才交给参数构建。
    let mut options = guard.settings.to_options();
    options.tune = task.tune.clone();
    Some((
        task.source_path.clone(),
        task.target_format.clone(),
        probe,
        options,
        guard.settings.output_dir.clone(),
        guard.settings.overwrite_existing,
    ))
}

/// MicroDVD / SBV：先把源字幕转成 SRT 到标准输出，再自己写目标文件。
fn write_subtitle_directly(
    tools: &ToolPaths,
    source_path: &str,
    output_path: &str,
    kind: args::DirectWrite,
) -> Outcome {
    let args: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-loglevel".into(),
        "error".into(),
        "-i".into(),
        source_path.to_string(),
        "-map".into(),
        "0:s:0".into(),
        "-c:s".into(),
        "subrip".into(),
        "-f".into(),
        "srt".into(),
        "pipe:1".into(),
    ];

    let srt = match runner::run_capture_stdout(&tools.ffmpeg, &args) {
        Ok(s) => s,
        Err(e) => return Outcome::Failed(e),
    };

    let cues = subtitle_text::srt_to_cues(&srt);
    if cues.is_empty() {
        return Outcome::Failed("源字幕里没有可用条目".into());
    }

    let content = match kind {
        args::DirectWrite::MicroDvd => subtitle_text::to_microdvd(&cues, 25.0),
        args::DirectWrite::Sbv => subtitle_text::to_sbv(&cues),
    };

    match std::fs::write(output_path, content) {
        Ok(()) => Outcome::Done,
        Err(e) => Outcome::Failed(format!("写入失败：{e}")),
    }
}

/// 更新一条任务的最终状态。
fn mark(inner: &Arc<Mutex<Inner>>, id: u64, status: TaskStatus, message: &str) {
    let mut guard = lock(inner);
    if let Some(task) = guard.tasks.iter_mut().find(|t| t.id == id) {
        task.status = status;
        task.message = message.to_string();
        if status != TaskStatus::Done {
            task.speed = None;
        }
        // 取消或失败时进度条归零：停在半路的百分比会让人以为还在跑。
        if matches!(status, TaskStatus::Cancelled | TaskStatus::Failed) {
            task.percent = 0.0;
        }
    }
}

/// 把一条任务的最新状态推给前端。
fn emit(app: &tauri::AppHandle, inner: &Arc<Mutex<Inner>>, id: u64) {
    let update = {
        let guard = lock(inner);
        guard.tasks.iter().find(|t| t.id == id).map(|t| TaskUpdate {
            id: t.id,
            status: t.status,
            percent: t.percent,
            speed: t.speed,
            output_path: t.output_path.clone(),
            output_size: t.output_size,
            message: t.message.clone(),
            notice: t.notice.clone(),
        })
    };
    if let Some(u) = update {
        let _ = app.emit("task-update", u);
    }
}

// ---------- 启动 ----------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let config_dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            let settings_path = settings::settings_path(&config_dir);
            let loaded = Settings::load(&settings_path);

            let resource_dir = app.path().resource_dir().ok();
            let tools = ToolPaths::locate(resource_dir.as_deref());

            // 能力探测约 90 ms，在 setup 里同步做，窗口稍晚出现但首屏就有真实能力。
            let (caps_opt, version) = if let Some(t) = &tools {
                let caps = runner::probe_capabilities(&t.ffmpeg).ok();
                let version = runner::ffmpeg_version(&t.ffmpeg).unwrap_or_default();
                (caps, version)
            } else {
                (None, String::new())
            };

            let state = AppState::new(settings_path, loaded, tools);
            *lock(&state.caps) = caps_opt;
            *lock(&state.version) = version;

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            add_paths,
            compatible_formats,
            set_target,
            set_target_bulk,
            param_specs,
            set_tune,
            remove_tasks,
            clear_tasks,
            save_settings,
            start,
            cancel,
        ])
        .run(tauri::generate_context!())
        .expect("MediaForge 启动失败");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::{FORMATS, by_id};

    fn caps() -> caps::Caps {
        let enc = include_str!("../tests/fixtures/encoders_full.txt");
        let mux = include_str!("../tests/fixtures/muxers_full.txt");
        caps::Caps::from_output(enc, mux)
    }

    #[test]
    fn every_format_is_available_with_real_capabilities() {
        let c = caps();
        let unavailable: Vec<(&str, String)> = FORMATS
            .iter()
            .filter_map(|f| {
                let (ok, reason) = format_availability(f, &c);
                if ok { None } else { Some((f.id, reason)) }
            })
            .collect();
        assert!(
            unavailable.is_empty(),
            "这些格式在当前 ffmpeg 下不可用：{unavailable:?}"
        );
    }

    #[test]
    fn missing_muxer_marks_format_unavailable() {
        let empty = caps::Caps::default();
        let f = by_id("mp4").unwrap();
        let (ok, reason) = format_availability(f, &empty);
        assert!(!ok);
        assert!(reason.contains("复用器"), "{reason}");
    }

    #[test]
    fn direct_write_formats_ignore_capabilities() {
        let empty = caps::Caps::default();
        for id in ["microdvd", "sbv"] {
            let f = by_id(id).unwrap();
            let (ok, _) = format_availability(f, &empty);
            assert!(ok, "{id} 由本程序自己写，不该受能力清单影响");
        }
    }

    #[test]
    fn default_target_matches_source_kind() {
        let c = caps();
        assert_eq!(default_target_for(Kind::Video, &c), "mp4");
        assert_eq!(default_target_for(Kind::Audio, &c), "mp3");
        assert_eq!(default_target_for(Kind::Image, &c), "png");
        assert_eq!(default_target_for(Kind::Subtitle, &c), "srt");
    }

    fn sample_task(kind: Kind, status: TaskStatus) -> Task {
        Task {
            id: 1,
            source_path: "a.bin".into(),
            file_name: "a.bin".into(),
            source_kind: kind,
            source_summary: String::new(),
            duration_secs: 1.0,
            source_size: 1,
            target_format: "mp4".into(),
            status,
            percent: 0.0,
            speed: None,
            output_path: String::new(),
            output_size: 0,
            message: String::new(),
            notice: String::new(),
            tune: crate::params::Tune::default(),
        }
    }

    #[test]
    fn revalidate_rejects_audio_to_video() {
        let c = caps();
        let mut task = sample_task(Kind::Audio, TaskStatus::Pending);
        revalidate(&mut task, by_id("mp4").unwrap(), &c);
        assert_eq!(task.status, TaskStatus::Incompatible);
        assert!(task.message.contains("音频"), "{}", task.message);
    }

    #[test]
    fn revalidate_accepts_video_to_audio_and_resets_progress() {
        let c = caps();
        let mut task = sample_task(Kind::Video, TaskStatus::Done);
        task.percent = 100.0;
        task.speed = Some(2.0);

        revalidate(&mut task, by_id("mp3").unwrap(), &c);
        assert_eq!(
            task.status,
            TaskStatus::Pending,
            "跑完的任务改格式后应可重跑"
        );
        assert_eq!(task.percent, 0.0);
        assert_eq!(task.speed, None);
    }

    #[test]
    fn media_extension_filter_accepts_common_types() {
        for name in ["a.mp4", "b.MKV", "c.flac", "d.png", "e.srt"] {
            assert!(is_media_extension(Path::new(name)), "{name} 应被识别");
        }
        for name in ["a.txt", "b.exe", "c.docx", "d"] {
            assert!(!is_media_extension(Path::new(name)), "{name} 不应被识别");
        }
    }

    #[test]
    fn collect_expands_directory_recursively() {
        let root = std::env::temp_dir().join("mediaforge-collect-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("a.mp4"), b"x").unwrap();
        std::fs::write(root.join("ignore.txt"), b"x").unwrap();
        std::fs::write(root.join("sub/b.mp3"), b"x").unwrap();

        let found = collect_media_paths(&[root.to_string_lossy().to_string()], 4);
        assert_eq!(found.len(), 2, "应找到 2 个媒体文件：{found:?}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn collect_skips_the_output_directory() {
        // 默认布局下输出就在源目录里。若不过滤，第二次拖同一个文件夹
        // 会把上次的产物当成新源文件，队列每轮翻倍。
        let root = std::env::temp_dir().join("mediaforge-collect-exclude");
        let _ = std::fs::remove_dir_all(&root);
        let outdir = root.join("converted");
        std::fs::create_dir_all(&outdir).unwrap();
        std::fs::write(root.join("a.mp4"), b"x").unwrap();
        std::fs::write(outdir.join("a.mkv"), b"x").unwrap();
        std::fs::write(outdir.join("b.mp3"), b"x").unwrap();

        let all = collect_media_paths(&[root.to_string_lossy().to_string()], 4);
        assert_eq!(all.len(), 3, "不排除时应看到 3 个");

        let filtered = collect_media_paths_excluding(
            &[root.to_string_lossy().to_string()],
            4,
            Some(outdir.as_path()),
        );
        assert_eq!(
            filtered.len(),
            1,
            "排除输出目录后应只剩源文件：{filtered:?}"
        );
        assert!(filtered[0].ends_with("a.mp4"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn collect_deduplicates_repeated_inputs() {
        let file = std::env::temp_dir().join("mediaforge-dedup.mp4");
        std::fs::write(&file, b"x").unwrap();
        let text = file.to_string_lossy().to_string();

        let found = collect_media_paths(&[text.clone(), text], 4);
        assert_eq!(found.len(), 1);

        let _ = std::fs::remove_file(&file);
    }
}
