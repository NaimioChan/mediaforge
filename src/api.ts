/**
 * 与 Rust 后端的唯一通道。所有 invoke / listen 都收在这里，
 * 便于统一处理错误与类型。
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { FormatView, ParamSpec, Settings, Snapshot, Task, TaskUpdate, Tune } from "./types";

export const api = {
  getSnapshot: () => invoke<Snapshot>("get_snapshot"),

  addPaths: (paths: string[]) => invoke<Task[]>("add_paths", { paths }),

  compatibleFormats: (taskId: number) =>
    invoke<FormatView[]>("compatible_formats", { taskId }),

  setTarget: (taskId: number, formatId: string) =>
    invoke<Task>("set_target", { taskId, formatId }),

  setTargetBulk: (taskIds: number[], formatId: string) =>
    invoke<Task[]>("set_target_bulk", { taskIds, formatId }),

  /** 某个格式的可调参数规格，界面据此渲染面板。 */
  paramSpecs: (formatId: string) => invoke<ParamSpec[]>("param_specs", { formatId }),

  /** 提交任务级参数细调，返回裁剪后的任务。 */
  setTune: (taskId: number, tune: Tune) => invoke<Task>("set_tune", { taskId, tune }),

  removeTasks: (taskIds: number[]) => invoke<number[]>("remove_tasks", { taskIds }),

  clearTasks: () => invoke<void>("clear_tasks"),

  saveSettings: (settings: Settings) =>
    invoke<Settings>("save_settings", { newSettings: settings }),

  start: () => invoke<void>("start"),

  cancel: () => invoke<void>("cancel"),
};

/** 订阅任务更新。返回取消订阅函数。 */
export function onTaskUpdate(handler: (update: TaskUpdate) => void): Promise<UnlistenFn> {
  return listen<TaskUpdate>("task-update", (event) => handler(event.payload));
}

/** 订阅进度更新（频率高于 task-update）。 */
export function onTaskProgress(handler: (update: TaskUpdate) => void): Promise<UnlistenFn> {
  return listen<TaskUpdate>("task-progress", (event) => handler(event.payload));
}

/** 订阅整批完成。 */
export function onBatchFinished(handler: () => void): Promise<UnlistenFn> {
  return listen("batch-finished", () => handler());
}

/** 把后端抛出的错误转成可读文本。 */
export function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}
