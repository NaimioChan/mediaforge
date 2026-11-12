/**
 * 右侧参数面板。
 *
 * 面板不写死控件，而是按当前目标格式向后端要「参数规格」再渲染。
 * 这样 MP4 能调编码器与 preset、FLAC 能调压缩级别、GIF 能调颜色数，
 * 界面本身不需要认识任何具体格式。
 *
 * 参数是任务级的：选中某条任务后调面板，改的是那一条任务的 tune。
 * 没有选中任务时面板显示提示，而不是给出一个不生效的全局设置。
 */

import { api, errorText } from "../api";
import { store } from "../store";
import type { ParamSpec, Task, Tune } from "../types";

type SpecsListener = (taskId: number, tune: Tune) => void;

/** formatId → 规格缓存。格式数量有限，进程内缓存即可。 */
const specCache = new Map<string, ParamSpec[]>();

/** 当前面板绑定的任务 id。 */
let boundTaskId: number | null = null;
/** 规格对应的格式 id，用于判断是否需要重新拉取。 */
let currentFormatId = "";

let onChange: SpecsListener | null = null;

/** 挂上「参数改动后」的回调。与 `bindPanel` 分开，避免两件事挤在一个入口。 */
export function initPanelEvents(cb: SpecsListener): void {
  onChange = cb;
}

/** 面板当前绑定的任务。 */
export function panelTask(): number | null {
  return boundTaskId;
}

/**
 * 绑定到某条任务并重绘。
 *
 * 同一格式内切任务只需要换值，不必重新请求规格；
 * 换格式才重新拉一次。
 */
export async function bindPanel(task: Task | null): Promise<void> {
  const root = document.getElementById("panel-body");
  if (!root) return;

  if (!task) {
    boundTaskId = null;
    currentFormatId = "";
    renderEmpty(root, "选中一条任务后可调参数");
    return;
  }

  boundTaskId = task.id;

  let specs = specCache.get(task.targetFormat);
  if (!specs) {
    try {
      specs = await api.paramSpecs(task.targetFormat);
      specCache.set(task.targetFormat, specs);
    } catch (error) {
      renderEmpty(root, `无法读取参数：${errorText(error)}`);
      return;
    }
  }

  // 请求期间用户可能已切到别的任务，回退避免覆盖。
  if (boundTaskId !== task.id) return;

  currentFormatId = task.targetFormat;

  if (specs.length === 0) {
    renderEmpty(root, "该格式没有可调参数");
    return;
  }

  renderSpecs(root, specs, task);
}

/** 目标格式变了，清掉该任务的规格缓存并重绘。 */
export function invalidateSpecs(formatId: string): void {
  specCache.delete(formatId);
}

function renderEmpty(root: HTMLElement, text: string): void {
  root.textContent = "";
  const p = document.createElement("p");
  p.className = "panel-empty";
  p.textContent = text;
  root.appendChild(p);
}

function renderSpecs(root: HTMLElement, specs: ParamSpec[], task: Task): void {
  root.textContent = "";

  // 标题行：当前格式名 + 重置。
  const head = document.createElement("div");
  head.className = "panel-head";

  const title = document.createElement("span");
  title.className = "panel-target";
  const format = store.get().formats.find((f) => f.id === task.targetFormat);
  title.textContent = format ? `${format.name} 参数` : "转换参数";

  const reset = document.createElement("fluent-button");
  reset.setAttribute("size", "small");
  reset.setAttribute("appearance", "subtle");
  reset.textContent = "恢复默认";
  reset.addEventListener("click", () => {
    void commit(task, {}, /* replace */ true);
  });

  head.append(title, reset);
  root.appendChild(head);

  // 按组渲染，保持规格给出的顺序。
  const groups: string[] = [];
  for (const spec of specs) {
    if (!groups.includes(spec.group)) groups.push(spec.group);
  }

  for (const group of groups) {
    const section = document.createElement("section");
    section.className = "param-group";

    const label = document.createElement("h3");
    label.className = "param-group-title";
    label.textContent = group;
    section.appendChild(label);

    for (const spec of specs.filter((s) => s.group === group)) {
      section.appendChild(renderField(spec, task));
    }

    root.appendChild(section);
  }
}

function renderField(spec: ParamSpec, task: Task): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "field";

  const current = valueOf(spec, task.tune);

  if (spec.kind === "toggle") {
    wrap.classList.add("field-row");

    const label = document.createElement("label");
    label.className = "field-label";
    label.textContent = spec.label;
    label.htmlFor = `param-${spec.id}`;

    const input = document.createElement("input");
    input.type = "checkbox";
    input.id = `param-${spec.id}`;
    input.checked = current === "1";
    input.addEventListener("change", () => {
      void commit(task, { [spec.id]: input.checked });
    });

    wrap.append(label, input);
    appendHelp(wrap, spec.help);
    return wrap;
  }

  const label = document.createElement("label");
  label.className = "field-label";
  label.htmlFor = `param-${spec.id}`;

  const text = document.createElement("span");
  text.textContent = spec.label;

  const valueTag = document.createElement("span");
  valueTag.className = "field-value";
  label.append(text, valueTag);

  if (spec.kind === "select") {
    if (spec.kind === "select") valueTag.textContent = "";
    const select = document.createElement("select");
    select.className = "select";
    select.id = `param-${spec.id}`;

    for (const choice of spec.choices) {
      const option = document.createElement("option");
      option.value = choice.value;
      option.textContent = choice.label;
      select.appendChild(option);
    }
    select.value = current === "" ? (spec.choices[0]?.value ?? "") : current;
    select.addEventListener("change", () => {
      void commit(task, { [spec.id]: select.value === "" ? null : select.value });
    });

    wrap.append(label, select);
    appendHelp(wrap, spec.help);
    return wrap;
  }

  if (spec.kind === "slider") {
    valueTag.textContent = current === "" ? spec.default : current;

    const slider = document.createElement("input");
    slider.type = "range";
    slider.id = `param-${spec.id}`;
    slider.min = String(spec.min ?? 0);
    slider.max = String(spec.max ?? 100);
    slider.step = String(spec.step ?? 1);
    slider.value = current === "" ? spec.default : current;

    // 拖动时只更新数字，松手才落盘，避免一次拖动写几十次。
    slider.addEventListener("input", () => {
      valueTag.textContent = slider.value;
    });
    slider.addEventListener("change", () => {
      void commit(task, { [spec.id]: Number(slider.value) });
    });

    wrap.append(label, slider);
    appendHelp(wrap, spec.help);
    return wrap;
  }

  // number：留空表示不限制。
  const input = document.createElement("input");
  input.type = "number";
  input.id = `param-${spec.id}`;
  input.className = "text-input";
  input.value = current;
  input.placeholder = `留空${spec.unit ? `（${spec.unit}）` : "不限"}`;
  if (spec.unit) input.title = spec.unit;

  // 失焦或回车才提交，边打字边提交会打断输入。
  const submit = () => {
    const raw = input.value.trim();
    void commit(task, { [spec.id]: raw === "" ? null : Number(raw) });
  };
  input.addEventListener("change", submit);
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      submit();
    }
  });

  wrap.append(label, input);
  appendHelp(wrap, spec.help);
  return wrap;
}

function appendHelp(parent: HTMLElement, help: string): void {
  if (!help) return;
  const p = document.createElement("p");
  p.className = "field-help";
  p.textContent = help;
  parent.appendChild(p);
}

/** 当前值：任务里设了就显示，否则用规格给的默认值。 */
function valueOf(spec: ParamSpec, tune: Tune): string {
  const raw = tune[spec.id];
  if (raw === undefined || raw === null) {
    return spec.kind === "number" ? "" : spec.default;
  }
  if (typeof raw === "boolean") return raw ? "1" : "0";
  return String(raw);
}

/**
 * 提交一条或多条参数。
 *
 * `replace` 为 true 时以空对象为基础（恢复默认），
 * 否则在现有 tune 上合并。后端会裁剪掉该格式不认识的字段。
 */
async function commit(task: Task, patch: Tune, replace = false): Promise<void> {
  if (task.status === "running") {
    store.setError("任务正在转换，无法改参数");
    return;
  }

  const merged: Tune = replace ? { ...patch } : { ...task.tune, ...patch };

  try {
    const updated = await api.setTune(task.id, merged);
    store.patchTask(task.id, updated);
    onChange?.(updated.id, updated.tune);
    // 值可能被后端夹到合法范围，重绘一次让界面与真实值一致。
    await bindPanel(updated);
  } catch (error) {
    store.setError(errorText(error));
  }
}

/** 外部改了目标格式后调用，重绘面板。 */
export async function refreshPanelFor(task: Task | null): Promise<void> {
  if (!task) {
    await bindPanel(null);
    return;
  }
  if (task.targetFormat !== currentFormatId) {
    specCache.delete(task.targetFormat);
  }
  await bindPanel(task);
}
