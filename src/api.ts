/**
 * 与 Rust 后端的唯一通道。所有 invoke / listen 都收在这里，
 * 便于统一处理错误与类型。
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { FormatView, Settings, Snapshot, Task, TaskUpdate } from "./types";

export const api = {
  getSnapshot: () => invoke<Snapshot>("get_snapshot"),

  addPaths: (paths: string[]) => invoke<Task[]>("add_paths", { paths }),

  compatibleFormats: (taskId: number) =>
    invoke<FormatView[]>("compatible_formats", { taskId }),

  setTarget: (taskId: number, formatId: string) =>
    invoke<Task>("set_target", { taskId, formatId }),

  setTargetBulk: (taskIds: number[], formatId: string) =>
    invoke<Task[]>("set_target_bulk", { taskIds, formatId }),

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
