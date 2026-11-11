/**
 * 单一状态容器。
 *
 * 后端是权威状态，这里只做视图侧的合并：
 * 命令返回的快照覆盖本地，事件按 id 定向更新。
 * 订阅者收到变更通知后自行决定重绘范围。
 */

import type { FormatView, Settings, Snapshot, Task, TaskUpdate } from "./types";

type Listener = () => void;

export interface State {
  tasks: Task[];
  formats: FormatView[];
  /** taskId → 该源可用的目标格式（按需加载）。 */
  compatible: Map<number, FormatView[]>;
  settings: Settings;
  encoderCount: number;
  muxerCount: number;
  ffmpegVersion: string;
  ffmpegFound: boolean;
  isRunning: boolean;
  /** 底栏一句话。 */
  status: string;
  /** 最近的错误，界面顶部展示。 */
  error: string;
}

const defaultSettings: Settings = {
  outputDir: "",
  overwriteExisting: false,
  concurrency: 1,
  quality: 80,
  audioBitrateKbps: 192,
  maxHeight: null,
  stripMetadata: false,
  copyStreams: false,
  lastInputDir: "",
};

let state: State = {
  tasks: [],
  formats: [],
  compatible: new Map(),
  settings: { ...defaultSettings },
  encoderCount: 0,
  muxerCount: 0,
  ffmpegVersion: "",
  ffmpegFound: false,
  isRunning: false,
  status: "就绪",
  error: "",
};

const listeners = new Set<Listener>();

function notify(): void {
  for (const listener of listeners) listener();
}

export const store = {
  get(): State {
    return state;
  },

  subscribe(listener: Listener): () => void {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },

  /** 用后端快照覆盖。 */
  applySnapshot(snapshot: Snapshot): void {
    state = {
      ...state,
      tasks: snapshot.tasks,
      formats: snapshot.formats,
      settings: snapshot.settings,
      encoderCount: snapshot.capabilities.encoderCount,
      muxerCount: snapshot.capabilities.muxerCount,
      ffmpegVersion: snapshot.capabilities.ffmpegVersion,
      ffmpegFound: snapshot.ffmpegFound,
      isRunning: snapshot.isRunning,
    };
    notify();
  },

  /** 追加新任务（去重，后端已经给过 id）。 */
  addTasks(tasks: Task[]): void {
    const existing = new Set(state.tasks.map((t) => t.id));
    const fresh = tasks.filter((t) => !existing.has(t.id));
    // 新任务默认排在最前，最近拖进来的先看到。
    state = { ...state, tasks: [...fresh, ...state.tasks] };
    notify();
  },

  /** 整条替换某个任务，或按 id 局部更新。 */
  patchTask(id: number, patch: Partial<Task>): void {
    let touched = false;
    const tasks = state.tasks.map((task) => {
      if (task.id !== id) return task;
      touched = true;
      return { ...task, ...patch };
    });
    if (!touched) return;
    state = { ...state, tasks };
    notify();
  },

  /** 处理事件推送。只更新变化的部分，避免整表重绘。 */
  applyUpdate(update: TaskUpdate): void {
    const index = state.tasks.findIndex((t) => t.id === update.id);
    if (index < 0) return;

    const current = state.tasks[index];
    const next: Task = {
      ...current,
      status: update.status,
      percent: update.percent,
      speed: update.speed,
      outputSize: update.outputSize,
      message: update.message,
    };
    if (update.outputPath) next.outputPath = update.outputPath;
    if (update.notice) next.notice = update.notice;

    const tasks = state.tasks.slice();
    tasks[index] = next;
    state = { ...state, tasks };
    notify();
  },

  /** 移除若干任务。 */
  removeTasks(ids: number[]): void {
    const set = new Set(ids);
    state = {
      ...state,
      tasks: state.tasks.filter((t) => !set.has(t.id)),
    };
    for (const id of ids) state.compatible.delete(id);
    notify();
  },

  clear(): void {
    state = { ...state, tasks: [], compatible: new Map() };
    notify();
  },

  /** 缓存某条任务的可用目标格式。 */
  setCompatible(taskId: number, formats: FormatView[]): void {
    state.compatible.set(taskId, formats);
    notify();
  },

  setSettings(settings: Settings): void {
    state = { ...state, settings };
    notify();
  },

  setRunning(running: boolean): void {
    state = { ...state, isRunning: running };
    notify();
  },

  setStatus(status: string): void {
    state = { ...state, status };
    notify();
  },

  setError(error: string): void {
    state = { ...state, error };
    notify();
  },
};

/** 格式 id → 展示名。 */
export function formatLabel(formats: FormatView[], id: string): string {
  const found = formats.find((f) => f.id === id);
  return found ? found.name : id;
}

/** 媒体类型的中文名。 */
export const kindLabel: Record<string, string> = {
  video: "视频",
  audio: "音频",
  image: "图像",
  subtitle: "字幕",
};
