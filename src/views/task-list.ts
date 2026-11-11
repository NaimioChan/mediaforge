/**
 * 任务列表渲染。
 *
 * 每条任务一行：文件名 + 源摘要 / 目标格式下拉 / 状态。
 * 重绘策略：整表重绘过于浪费（进度事件每秒几十次），
 * 因此按 id 维护 DOM 节点，事件只更新对应行的进度与状态文字。
 */

import type { FormatView, Task } from "../types";
import { formatLabel, kindLabel, store } from "../store";

interface Row {
  root: HTMLLIElement;
  /** 目标格式下拉。 */
  select: HTMLSelectElement;
  status: HTMLElement;
  progress: HTMLElement;
  progressFill: HTMLElement;
  meta: HTMLElement;
  remove: HTMLButtonElement;
}

const rows = new Map<number, Row>();

export interface RowCallbacks {
  onTargetChange: (taskId: number, formatId: string) => void;
  onRemove: (taskId: number) => void;
  onFocus: (taskId: number) => void;
}

let callbacks: RowCallbacks | null = null;

export function initTaskList(cb: RowCallbacks): void {
  callbacks = cb;
}

/** 全量同步：列表结构以 store 为准。 */
export function syncTaskList(): void {
  const list = document.getElementById("task-list");
  const empty = document.getElementById("empty-state");
  if (!list) return;

  const { tasks, formats } = store.get();

  if (empty) {
    empty.hidden = tasks.length > 0;
  }

  // 移除已经不存在的行。
  const alive = new Set(tasks.map((t) => t.id));
  for (const [id, row] of rows) {
    if (!alive.has(id)) {
      row.root.remove();
      rows.delete(id);
    }
  }

  // 按 store 顺序重排并补齐新行。
  for (const task of tasks) {
    let row = rows.get(task.id);
    if (!row) {
      row = createRow(task, formats);
      rows.set(task.id, row);
    }
    list.appendChild(row.root);
    updateRow(row, task, formats);
  }
}

function createRow(task: Task, formats: FormatView[]): Row {
  const root = document.createElement("li");
  root.className = "task";
  root.dataset.id = String(task.id);

  const main = document.createElement("div");
  main.className = "task-main";

  const name = document.createElement("div");
  name.className = "task-name";
  name.textContent = task.fileName;

  const meta = document.createElement("div");
  meta.className = "task-meta";

  main.append(name, meta);

  const controls = document.createElement("div");
  controls.className = "task-controls";

  const select = document.createElement("select");
  select.className = "select task-format";
  select.addEventListener("change", () => {
    callbacks?.onTargetChange(task.id, select.value);
  });

  const remove = document.createElement("button");
  remove.className = "icon-button";
  remove.title = "从队列移除";
  remove.textContent = "✕";
  remove.addEventListener("click", () => {
    callbacks?.onRemove(task.id);
  });

  controls.append(select, remove);

  const status = document.createElement("div");
  status.className = "task-status";

  const progress = document.createElement("div");
  progress.className = "task-progress";
  const progressFill = document.createElement("div");
  progressFill.className = "task-progress-fill";
  progress.appendChild(progressFill);

  const row: Row = { root, select, status, progress, progressFill, meta, remove };

  root.addEventListener("click", (event) => {
    // 点下拉或删除按钮不算选中。
    if (event.target instanceof HTMLSelectElement || event.target === remove) return;
    callbacks?.onFocus(task.id);
  });

  root.append(main, controls, status, progress);
  fillOptions(row, task, formats);
  return row;
}

/** 目标格式下拉：只列与源类型兼容的项，不可用的置灰。 */
function fillOptions(row: Row, task: Task, formats: FormatView[]): void {
  const compatible = store.get().compatible.get(task.id);
  const pool = compatible ?? formats.filter((f) => f.kind === task.sourceKind);

  const signature = pool.map((f) => `${f.id}:${f.available}`).join(",");
  if (row.select.dataset.signature === signature) {
    row.select.value = task.targetFormat;
    return;
  }

  row.select.textContent = "";
  for (const format of pool) {
    const option = document.createElement("option");
    option.value = format.id;
    option.textContent = `${format.name}  ${format.ext}`;
    if (!format.available) {
      option.disabled = true;
      option.textContent += "（不可用）";
      option.title = format.unavailableReason;
    }
    if (format.notes) option.title = format.notes;
    row.select.appendChild(option);
  }
  row.select.dataset.signature = signature;
  row.select.value = task.targetFormat;
}

function updateRow(row: Row, task: Task, formats: FormatView[]): void {
  row.root.dataset.status = task.status;
  fillOptions(row, task, formats);

  // 源摘要 + 源类型徽标 + 时长。
  const parts: string[] = [kindLabel[task.sourceKind] ?? task.sourceKind, task.sourceSummary];
  if (task.durationSecs > 0) parts.push(formatDuration(task.durationSecs));
  if (task.sourceSize > 0) parts.push(formatSize(task.sourceSize));
  row.meta.textContent = parts.filter(Boolean).join(" · ");

  row.status.textContent = statusText(task);
  row.status.title = task.message || task.notice;

  const showProgress = task.status === "running" || task.status === "done";
  row.progress.hidden = !showProgress;
  row.progressFill.style.width = `${Math.max(0, Math.min(100, task.percent))}%`;

  const locked = task.status === "running";
  row.select.disabled = locked;
  row.remove.disabled = locked;
}

function statusText(task: Task): string {
  switch (task.status) {
    case "pending":
      return task.notice ? `等待 · ${task.notice}` : "等待";
    case "running": {
      const speed = task.speed ? ` · ${task.speed.toFixed(2)}×` : "";
      return `转换中 ${task.percent.toFixed(0)}%${speed}`;
    }
    case "done": {
      const size = task.outputSize > 0 ? ` · ${formatSize(task.outputSize)}` : "";
      const notice = task.notice ? ` · ${task.notice}` : "";
      return `完成${size}${notice}`;
    }
    case "failed":
      return `失败 · ${task.message || "未知原因"}`;
    case "cancelled":
      return "已取消";
    case "incompatible":
      return `不兼容 · ${task.message}`;
    default:
      return task.status;
  }
}

export function formatSize(bytes: number): string {
  if (bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 100 || unit === 0 ? 0 : 1)} ${units[unit]}`;
}

export function formatDuration(seconds: number): string {
  const total = Math.round(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return `${m}:${String(s).padStart(2, "0")}`;
}

/** 当前选中的任务 id，用于「只改选中项」。 */
let focusedId: number | null = null;

export function setFocused(id: number | null): void {
  focusedId = id;
  for (const [rowId, row] of rows) {
    row.root.dataset.focused = String(rowId === id);
  }
}

export function getFocused(): number | null {
  return focusedId;
}

export { formatLabel };
