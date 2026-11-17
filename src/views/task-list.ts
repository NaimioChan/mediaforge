/**
 * 任务列表渲染。
 *
 * 每条任务一行：文件名 + 源摘要 / 目标格式下拉 / 状态。
 * 重绘策略：整表重绘过于浪费（进度事件每秒几十次），
 * 因此按 id 维护 DOM 节点，事件只更新对应行的进度与状态文字。
 *
 * 控件是 iX 的 web components。它们的事件是 `CustomEvent`，值在 `detail` 上，
 * 且要 `addEventListener` 挂（`on:valueChange` 是 Solid 的写法，原生 DOM 没有）。
 */

import { ICON } from "../icons";
import type { FormatView, Task } from "../types";
import { formatLabel, kindLabel, store } from "../store";

/** iX 的 `ix-select` 宿主元素。属性在运行时赋值，不需要类型声明文件。 */
interface IxSelect extends HTMLElement {
  value: string;
  disabled: boolean;
  label: string;
  allowClear: boolean;
}

interface IxProgress extends HTMLElement {
  value: number;
  status: string;
  type: string;
  size: string;
}

interface IxIconButton extends HTMLElement {
  icon: string;
  variant: string;
  size: string;
  disabled: boolean;
  title: string;
}

interface Row {
  root: HTMLLIElement;
  /** 目标格式下拉。 */
  select: IxSelect;
  status: HTMLElement;
  progress: IxProgress;
  meta: HTMLElement;
  remove: IxIconButton;
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
      row = createRow(task);
      rows.set(task.id, row);
    }
    list.appendChild(row.root);
    updateRow(row, task, formats);
  }
}

function createRow(task: Task): Row {
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

  const select = document.createElement("ix-select") as IxSelect;
  select.className = "task-format";
  // iX 的 select 自带清空按钮与下拉箭头，这里只留下拉。
  select.allowClear = false;
  // iX 的文案是英文常量（下拉列表表头、无匹配项、占位）。不覆盖的话
  // 展开列表时会在顶部露出「Select an option」。
  const i18n = select as unknown as Record<string, string>;
  i18n.i18nSelectListHeader = "目标格式";
  i18n.i18nNoMatches = "无匹配项";
  i18n.i18nPlaceholder = "选择目标格式";
  select.addEventListener("valueChange", (event) => {
    const value = (event as CustomEvent<string | string[]>).detail;
    const id = Array.isArray(value) ? value[0] : value;
    if (id) callbacks?.onTargetChange(task.id, id);
  });

  const remove = document.createElement("ix-icon-button") as IxIconButton;
  remove.icon = ICON.trash;
  remove.variant = "tertiary";
  remove.size = "16";
  remove.title = "从队列移除";
  remove.setAttribute("aria-label", "从队列移除");
  remove.addEventListener("click", () => {
    callbacks?.onRemove(task.id);
  });

  controls.append(select, remove);

  const status = document.createElement("div");
  status.className = "task-status";

  // 线性进度条，xs 是 2px 高，正好是原来那条细线的厚度。
  const progress = document.createElement("ix-progress-indicator") as IxProgress;
  progress.className = "task-progress";
  progress.type = "linear";
  progress.size = "xs";
  progress.value = 0;

  const row: Row = { root, select, status, progress, meta, remove };

  root.addEventListener("click", (event) => {
    // 点下拉或删除按钮不算选中。
    const target = event.target as Node;
    if (select.contains(target) || remove.contains(target)) return;
    callbacks?.onFocus(task.id);
  });

  root.append(main, controls, status, progress);
  return row;
}

/** 目标格式下拉：只列与源类型兼容的项，不可用的置灰。 */
function fillOptions(row: Row, task: Task, formats: FormatView[]): void {
  const compatible = store.get().compatible.get(task.id);
  const pool = compatible ?? formats.filter((f) => f.kind === task.sourceKind);

  const signature = pool.map((f) => `${f.id}:${f.available}`).join(",");
  if (row.select.dataset.signature !== signature) {
    // `ix-select` 的选项是 `ix-select-item` 子元素，不是 `<option>`。
    // 重建整批选项时先清空，再一次性挂上去。
    row.select.textContent = "";
    for (const format of pool) {
      const item = document.createElement("ix-select-item");
      item.setAttribute("value", format.id);
      item.setAttribute(
        "label",
        format.available ? `${format.name}  ${format.ext}` : `${format.name}  ${format.ext}（不可用）`,
      );
      if (!format.available) {
        item.setAttribute("disabled", "");
        item.title = format.unavailableReason;
      } else if (format.notes) {
        item.title = format.notes;
      }
      row.select.appendChild(item);
    }
    row.select.dataset.signature = signature;
  }

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
  row.progress.value = Math.max(0, Math.min(100, task.percent));
  row.progress.status = task.status === "done" ? "success" : "default";

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
