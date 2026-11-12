/**
 * 入口：装配界面、接收事件、驱动 store。
 */

import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";

import { api, errorText, onBatchFinished, onTaskProgress, onTaskUpdate } from "./api";
import { initTheme } from "./fluent";
import { store } from "./store";
import { checkForUpdate, initUpdateButton } from "./update-check";
import {
  getFocused,
  initTaskList,
  setFocused,
  syncTaskList,
} from "./views/task-list";
import {
  bindPanel,
  initPanelEvents,
  invalidateSpecs,
  refreshPanelFor,
} from "./views/param-panel";
import type { Settings } from "./types";

initTheme();

// ---------- 渲染订阅 ----------

function render(): void {
  const state = store.get();
  syncTaskList();

  const statusFfmpeg = document.getElementById("status-ffmpeg");
  const statusCaps = document.getElementById("status-caps");
  const statusSummary = document.getElementById("status-summary");
  const statusProgress = document.getElementById("status-progress");
  const btnStart = document.getElementById("btn-start") as HTMLButtonElement | null;
  const btnCancel = document.getElementById("btn-cancel") as HTMLButtonElement | null;
  const btnClear = document.getElementById("btn-clear") as HTMLButtonElement | null;

  if (statusFfmpeg) {
    statusFfmpeg.textContent = state.ffmpegFound
      ? state.ffmpegVersion || "ffmpeg 已就绪"
      : "找不到 ffmpeg";
    statusFfmpeg.style.color = state.ffmpegFound
      ? ""
      : "var(--colorPaletteRedForeground1, #c50f1f)";
  }

  if (statusCaps) {
    statusCaps.textContent = state.encoderCount
      ? `${state.encoderCount} 个编码器 · ${state.muxerCount} 个复用器`
      : "";
  }

  if (statusSummary) {
    const running = state.tasks.filter((t) => t.status === "running").length;
    const done = state.tasks.filter((t) => t.status === "done").length;
    const failed = state.tasks.filter((t) => t.status === "failed").length;

    if (state.tasks.length === 0) {
      statusSummary.textContent = state.status;
    } else {
      const parts = [`${state.tasks.length} 个任务`];
      if (done) parts.push(`完成 ${done}`);
      if (failed) parts.push(`失败 ${failed}`);
      if (running) parts.push(`进行中 ${running}`);
      statusSummary.textContent = parts.join(" · ");
    }
  }

  if (statusProgress) {
    const running = state.tasks.find((t) => t.status === "running");
    statusProgress.textContent = running
      ? `${running.fileName} ${running.percent.toFixed(0)}%`
      : "";
  }

  if (btnStart) {
    const canStart =
      !state.isRunning &&
      state.ffmpegFound &&
      state.tasks.some((t) =>
        ["pending", "done", "failed", "cancelled"].includes(t.status),
      );
    btnStart.disabled = !canStart;
  }
  if (btnCancel) btnCancel.disabled = !state.isRunning;
  if (btnClear) btnClear.disabled = state.isRunning || state.tasks.length === 0;
}

store.subscribe(render);

// ---------- 拖放 ----------

function initDragAndDrop(): void {
  const overlay = document.getElementById("drop-overlay");
  const win = getCurrentWindow();

  // WebView2 里 HTML5 drop 事件拿不到文件路径（安全限制），
  // 必须用 Tauri 的窗口级拖放事件。
  win.onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === "enter" || payload.type === "over") {
      if (overlay) overlay.hidden = false;
      return;
    }
    if (overlay) overlay.hidden = true;

    if (payload.type === "drop" && payload.paths.length > 0) {
      void addPaths(payload.paths);
    }
  });
}

async function addPaths(paths: string[]): Promise<void> {
  try {
    store.setError("");
    const tasks = await api.addPaths(paths);
    store.addTasks(tasks);
    store.setStatus(`已加入 ${tasks.length} 个文件`);
  } catch (error) {
    store.setError(errorText(error));
  }
}

// ---------- 交互 ----------

function initTaskListCallbacks(): void {
  initTaskList({
    onTargetChange: (taskId, formatId) => {
      void (async () => {
        try {
          const task = await api.setTarget(taskId, formatId);
          store.patchTask(taskId, task);
          // 目标格式变了，面板的规格也变了。
          invalidateSpecs(formatId);
          if (getFocused() === taskId) {
            await refreshPanelFor(task);
          }
        } catch (error) {
          store.setError(errorText(error));
        }
      })();
    },
    onRemove: (taskId) => {
      void (async () => {
        try {
          const removed = await api.removeTasks([taskId]);
          store.removeTasks(removed);
          if (getFocused() === taskId) {
            setFocused(null);
            await bindPanel(null);
          }
        } catch (error) {
          store.setError(errorText(error));
        }
      })();
    },
    onFocus: (taskId) => {
      const next = getFocused() === taskId ? null : taskId;
      setFocused(next);
      const task = next === null ? null : (store.get().tasks.find((t) => t.id === next) ?? null);
      void bindPanel(task);
    },
  });
}

function initToolbar(): void {
  const btnAddFiles = document.getElementById("btn-add-files");
  const btnAddFolder = document.getElementById("btn-add-folder");
  const btnStart = document.getElementById("btn-start");
  const btnCancel = document.getElementById("btn-cancel");
  const btnClear = document.getElementById("btn-clear");
  const btnPickDir = document.getElementById("btn-pick-dir");

  btnAddFiles?.addEventListener("click", () => {
    void (async () => {
      try {
        const picked = await open({
          multiple: true,
          title: "选择媒体文件",
          filters: [
            {
              name: "媒体文件",
              extensions: [
                "mp4", "mkv", "webm", "mov", "avi", "wmv", "flv", "ts", "m2ts", "mpg",
                "mpeg", "vob", "3gp", "ogv", "mxf", "dv", "gif",
                "mp3", "m4a", "aac", "flac", "wav", "aiff", "opus", "ogg", "wma", "ac3",
                "eac3", "dts", "tta", "wv",
                "png", "jpg", "jpeg", "webp", "avif", "jxl", "tif", "tiff", "bmp", "tga",
                "ico", "exr", "hdr", "dpx", "jp2", "qoi",
                "srt", "ass", "ssa", "vtt", "ttml", "sub", "sbv", "lrc",
              ],
            },
            { name: "全部文件", extensions: ["*"] },
          ],
        });
        if (!picked) return;
        const paths = Array.isArray(picked) ? picked : [picked];
        if (paths.length > 0) await addPaths(paths);
      } catch (error) {
        store.setError(errorText(error));
      }
    })();
  });

  btnAddFolder?.addEventListener("click", () => {
    void (async () => {
      try {
        const picked = await open({ directory: true, multiple: false, title: "选择文件夹" });
        if (typeof picked === "string") await addPaths([picked]);
      } catch (error) {
        store.setError(errorText(error));
      }
    })();
  });

  btnStart?.addEventListener("click", () => {
    void (async () => {
      try {
        store.setError("");
        store.setRunning(true);
        await api.start();
      } catch (error) {
        store.setRunning(false);
        store.setError(errorText(error));
      }
    })();
  });

  btnCancel?.addEventListener("click", () => {
    void (async () => {
      try {
        await api.cancel();
        store.setStatus("正在取消…");
      } catch (error) {
        store.setError(errorText(error));
      }
    })();
  });

  btnClear?.addEventListener("click", () => {
    void (async () => {
      try {
        await api.clearTasks();
        store.clear();
      } catch (error) {
        store.setError(errorText(error));
      }
    })();
  });

  btnPickDir?.addEventListener("click", () => {
    void (async () => {
      try {
        const current = store.get().settings.outputDir;
        const picked = await open({
          directory: true,
          multiple: false,
          title: "选择输出目录",
          defaultPath: current || undefined,
        });
        if (typeof picked === "string") {
          await persistSettings({ outputDir: picked });
        }
      } catch (error) {
        store.setError(errorText(error));
      }
    })();
  });
}

// ---------- 参数面板 ----------

async function persistSettings(patch: Partial<Settings>): Promise<void> {
  const merged: Settings = { ...store.get().settings, ...patch };
  try {
    const saved = await api.saveSettings(merged);
    store.setSettings(saved);
    syncPanelFromSettings();
  } catch (error) {
    store.setError(errorText(error));
    // 后端拒绝（例如目录不存在）时，把输入框退回上次生效的值，
    // 免得界面上留着一个不会被采用的路径。
    syncPanelFromSettings();
  }
}

function syncPanelFromSettings(): void {
  const { settings } = store.get();

  const dir = document.getElementById("output-dir") as HTMLInputElement | null;
  if (dir) {
    dir.value = settings.outputDir;
    dir.title = settings.outputDir || "与源文件同目录";
  }
}

function initPanelSection(): void {
  initPanelEvents((taskId, tune) => {
    // 参数改动后归零该行状态；后端已把不可用字段裁掉。
    const task = store.get().tasks.find((t) => t.id === taskId);
    if (task) store.patchTask(taskId, { tune });
  });
}

function initPanel(): void {
  const dir = document.getElementById("output-dir") as HTMLInputElement | null;
  if (!dir) return;

  // 手输路径：离开输入框或按回车时提交。清空即回到「与源文件同目录」。
  dir.addEventListener("change", () => {
    void persistSettings({ outputDir: dir.value });
  });

  // 回车立刻提交，不必等失焦。
  dir.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      dir.blur();
    }
  });
}

// ---------- 错误条 ----------

function showError(message: string): void {
  if (!message) return;

  let bar = document.getElementById("error-bar");
  if (!bar) {
    bar = document.createElement("div");
    bar.id = "error-bar";
    bar.className = "error-bar";
    document.getElementById("app")?.prepend(bar);
  }
  bar.textContent = message;
  bar.hidden = false;

  window.clearTimeout(errorTimer);
  errorTimer = window.setTimeout(() => {
    bar.hidden = true;
    store.setError("");
  }, 6000);
}

let errorTimer = 0;

// ---------- 启动 ----------

async function boot(): Promise<void> {
  initTaskListCallbacks();
  initToolbar();
  initPanel();
  initPanelSection();

  try {
    const snapshot = await api.getSnapshot();
    store.applySnapshot(snapshot);
    syncPanelFromSettings();

    // 版本按钮：先按当前版本渲染，再后台查一次更新。
    // 检查失败或仓库未公开都不在界面上留痕迹。
    initUpdateButton(snapshot.appVersion);
    void checkForUpdate(snapshot.appVersion);

    if (!snapshot.ffmpegFound) {
      store.setError("找不到 ffmpeg。请确认安装目录下存在 ffmpeg 子目录。");
    }
  } catch (error) {
    store.setError(errorText(error));
  }

  initDragAndDrop();

  await onTaskUpdate((update) => store.applyUpdate(update));
  await onTaskProgress((update) => {
    // 进度事件只更新数字，不重绘整表。
    store.applyUpdate(update);
  });
  await onBatchFinished(() => {
    store.setRunning(false);
    store.setStatus("就绪");
  });

  // 首次进入就同步一次，让空状态与按钮禁用正确。
  render();
  syncPanelFromSettings();
  await bindPanel(null);
}

// 错误条接到 store 上。
let lastError = "";
store.subscribe(() => {
  const { error } = store.get();
  if (error && error !== lastError) {
    lastError = error;
    showError(error);
  }
  if (!error) lastError = "";
});

void boot();
