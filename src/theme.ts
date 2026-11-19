/**
 * 配色主题：Windows 11 Fluent 深浅两套。
 *
 * 界面里所有颜色都来自 iX 的 `--theme-*` token，组件与我们自己的 CSS 读同一批
 * 变量，所以换主题只需要覆盖这一层变量，不存在第二套调色板要同步。
 *
 * 两个配色不是两百个 token，而是九个颜色：底色、前景中性色、强调色、次强调色、
 * 强调色上的文字，加上四个语义色。其余全部由它们推出来——中性色阶从底色到前景插值，
 * 悬浮与组件填充用前景的透明度，选中态用强调色的透明度。推导而不是逐条手写，
 * 是因为手写两百条 token 就有两百次把悬浮态写成与底色不搭的机会。
 *
 * 深浅色跟随系统（WebView2 把 Windows 应用主题反映成 `prefers-color-scheme`），
 * 界面不提供切换开关。
 *
 * 特异性说明：iX 自己的 token 块是
 * `[data-ix-theme=classic][data-ix-color-schema=dark]`（0,2,0）。
 * 这里的覆盖块写成 `html[data-ix-theme][data-ix-color-schema]`（0,2,1），
 * 与源文件顺序无关，恒胜。
 */

/** 一套配色由这九个颜色决定。 */
export interface Palette {
  /** 最深的表面：窗口底色。 */
  bg: string
  /** 色阶混向的中性色，也是所有文字色的基色。 */
  fg: string
  /** 交互强调色：主按钮、选中、焦点。 */
  accent: string
  /** 次强调色。iX 用它画控件的「活动」态（滑块thumb）。 */
  dynamic: string
  /** 画在强调色上的文字。 */
  contrast: string
  success: string
  warning: string
  alarm: string
  info: string
  neutral: string
}

// ------------------------------------------------------------------ colours

type Rgb = [number, number, number]

function parse(hex: string): Rgb {
  const clean = hex.replace("#", "")
  const full =
    clean.length === 3
      ? clean
          .split("")
          .map((c) => c + c)
          .join("")
      : clean
  return [
    Number.parseInt(full.slice(0, 2), 16),
    Number.parseInt(full.slice(2, 4), 16),
    Number.parseInt(full.slice(4, 6), 16),
  ]
}

function toHex([r, g, b]: Rgb): string {
  const part = (n: number) =>
    Math.round(Math.min(255, Math.max(0, n)))
      .toString(16)
      .padStart(2, "0")
  return `#${part(r)}${part(g)}${part(b)}`
}

/** 从 `a` 到 `b` 的 `t` 处。 */
function mix(a: string, b: string, t: number): string {
  const [ar, ag, ab] = parse(a)
  const [br, bg, bb] = parse(b)
  return toHex([ar + (br - ar) * t, ag + (bg - ag) * t, ab + (bb - ab) * t])
}

function alpha(color: string, a: number): string {
  const [r, g, b] = parse(color)
  return `rgba(${r},${g},${b},${a})`
}

/**
 * 中性色阶的默认落点，iX 的 `--theme-color-1` … `-8`。
 *
 * `-1` 是窗口底色，`-8` 是最暗的文字；中间几级是表面与边框。比例取自 iX
 * classic 深色方案的实际落点，这样推出来的色阶有同样的明暗节奏。
 */
const RAMP = [0, 0.16, 0.26, 0.35, 0.43, 0.5, 0.56, 0.74]

/**
 * Windows 的两套色阶比 `RAMP` 更平：Fluent 把窗口压在 `#202020` / `#f3f3f3`，
 * 卡片只比窗口亮几个百分点，而固定色阶会让 `--theme-color-2` 明显更浅。
 * 这里的步长是从 WinUI 的 base / layer / card 填充读出来的，不是估的。
 */
const RAMP_DARK = [0, 0.06, 0.11, 0.17, 0.25, 0.35, 0.46, 0.74]
const RAMP_LIGHT = [0, 0.055, 0.1, 0.16, 0.24, 0.33, 0.44, 0.74]

/** 把一套配色展开成 token 覆盖表。 */
function buildTokens(
  p: Palette,
  schema: "dark" | "light",
  rampSteps: number[] = RAMP,
): Record<string, string> {
  const ramp = rampSteps.map((t) => mix(p.bg, p.fg, t))
  const accentHover = mix(p.accent, "#ffffff", schema === "dark" ? 0.22 : 0.12)
  const accentActive = mix(p.accent, "#000000", 0.12)
  const dynamicHover = mix(p.dynamic, "#ffffff", schema === "dark" ? 0.2 : 0.12)
  const dynamicActive = mix(p.dynamic, "#000000", 0.1)
  const critical = mix(p.warning, p.alarm, 0.5)

  return {
    // --- 表面
    "--theme-color-0": "transparent",
    "--theme-color-1": ramp[0]!,
    "--theme-color-1--hover": ramp[2]!,
    "--theme-color-1--active": ramp[1]!,
    "--theme-color-2": ramp[1]!,
    "--theme-color-3": ramp[2]!,
    "--theme-color-4": ramp[3]!,
    "--theme-color-5": ramp[4]!,
    "--theme-color-6": ramp[5]!,
    "--theme-color-7": ramp[6]!,
    "--theme-color-8": ramp[7]!,

    // --- 文字，一律是同一个中性的透明度
    "--theme-color-std-text": alpha(p.fg, 0.94),
    "--theme-color-soft-text": alpha(p.fg, 0.78),
    "--theme-color-weak-text": alpha(p.fg, 0.58),
    "--theme-color-inv-std-text": alpha(p.bg, 0.92),
    "--theme-color-inv-soft-text": alpha(p.bg, 0.66),
    "--theme-color-inv-weak-text": alpha(p.bg, 0.42),
    "--theme-color-contrast-text": schema === "dark" ? "#ffffff" : p.bg,
    "--theme-color-contrast-bdr": alpha(p.fg, 0.85),

    // --- 边框
    "--theme-color-std-bdr": alpha(p.fg, 0.55),
    "--theme-color-soft-bdr": alpha(p.fg, 0.38),
    "--theme-color-weak-bdr": alpha(p.fg, 0.22),
    "--theme-color-x-weak-bdr": alpha(p.fg, 0.14),
    "--theme-color-hard-bdr": alpha(p.fg, 0.75),

    // --- 强调色的三种状态
    "--theme-color-primary": p.accent,
    "--theme-color-primary--hover": accentHover,
    "--theme-color-primary--active": accentActive,
    "--theme-color-primary--disabled": alpha(p.accent, 0.45),
    "--theme-color-primary--contrast": p.contrast,
    "--theme-color-dynamic": p.dynamic,
    "--theme-color-dynamic--hover": dynamicHover,
    "--theme-color-dynamic--active": dynamicActive,
    "--theme-color-dynamic-alt": p.dynamic,
    "--theme-color-dynamic-alt--hover": dynamicHover,
    "--theme-color-dynamic-alt--active": dynamicActive,
    "--theme-color-focus-bdr": p.dynamic,
    "--theme-color-secondary": alpha(p.accent, 0.14),
    "--theme-color-secondary--hover": alpha(p.accent, 0.22),
    "--theme-color-secondary--active": alpha(p.accent, 0.3),

    // --- ghost 填充：悬浮的按钮与选中的行
    "--theme-color-ghost": "transparent",
    "--theme-color-ghost--hover": alpha(p.fg, 0.1),
    "--theme-color-ghost--active": alpha(p.fg, 0.16),
    "--theme-color-ghost--selected": alpha(p.accent, 0.16),
    "--theme-color-ghost--selected-hover": alpha(p.accent, 0.22),
    "--theme-color-ghost--selected-active": alpha(p.accent, 0.28),
    "--theme-color-ghost-alt": alpha(p.fg, 0.04),
    "--theme-color-ghost-alt--hover": alpha(p.fg, 0.1),
    "--theme-color-ghost-alt--active": alpha(p.fg, 0.16),
    "--theme-color-ghost-alt--selected": alpha(p.accent, 0.14),
    "--theme-color-ghost-alt--selected-hover": alpha(p.accent, 0.2),
    "--theme-color-ghost-alt--selected-active": alpha(p.accent, 0.26),
    "--theme-color-ghost-primary--hover": alpha(p.accent, 0.14),
    "--theme-color-ghost-primary--active": alpha(p.accent, 0.2),

    // --- 组件填充：输入框底色、分隔线、禁用态
    "--theme-color-component-1": alpha(p.fg, 0.14),
    "--theme-color-component-1--hover": alpha(p.fg, 0.22),
    "--theme-color-component-1--active": alpha(p.fg, 0.26),
    "--theme-color-component-2": alpha(p.fg, 0.09),
    "--theme-color-component-3": alpha(p.fg, 0.24),
    "--theme-color-component-4": alpha(p.fg, 0.16),
    "--theme-color-component-5": alpha(p.fg, 0.34),
    "--theme-color-component-6": alpha(p.fg, 0.44),
    "--theme-color-component-7": alpha(p.accent, 0.16),
    "--theme-color-component-7--hover": alpha(p.accent, 0.24),
    "--theme-color-component-7--active": alpha(p.accent, 0.3),
    "--theme-color-component-8": mix(p.bg, p.fg, 0.1),
    "--theme-color-component-8--hover": mix(p.bg, p.fg, 0.15),
    "--theme-color-component-9": p.accent,
    "--theme-color-component-9--hover": accentHover,
    "--theme-color-component-9--active": accentActive,
    "--theme-color-component-9--disabled": alpha(p.accent, 0.3),
    "--theme-color-component-10": p.dynamic,
    "--theme-color-component-10--hover": dynamicHover,
    "--theme-color-component-10--active": dynamicActive,
    "--theme-color-component-10--disabled": alpha(p.dynamic, 0.3),
    "--theme-color-component-11": alpha(p.fg, 0.12),
    "--theme-color-component-error": mix(p.bg, p.alarm, 0.22),
    "--theme-color-component-info": mix(p.bg, p.info, 0.22),
    "--theme-color-component-warning": mix(p.bg, p.warning, 0.22),

    // --- 语义色。`critical` 是 iX 自己夹在 warning 与 alarm 之间的那级。
    "--theme-color-success": p.success,
    "--theme-color-success--hover": mix(p.success, "#ffffff", 0.18),
    "--theme-color-success--active": mix(p.success, "#000000", 0.12),
    "--theme-color-success--contrast": p.contrast,
    "--theme-color-success-40": alpha(p.success, 0.4),
    "--theme-color-warning": p.warning,
    "--theme-color-warning--hover": mix(p.warning, "#ffffff", 0.18),
    "--theme-color-warning--active": mix(p.warning, "#000000", 0.12),
    "--theme-color-warning--contrast": p.contrast,
    "--theme-color-warning-bdr": p.warning,
    "--theme-color-warning-text": p.warning,
    "--theme-color-warning-10": alpha(p.warning, 0.1),
    "--theme-color-warning-40": alpha(p.warning, 0.4),
    "--theme-color-alarm": p.alarm,
    "--theme-color-alarm--hover": mix(p.alarm, "#ffffff", 0.18),
    "--theme-color-alarm--active": mix(p.alarm, "#000000", 0.12),
    "--theme-color-alarm--contrast": p.contrast,
    "--theme-color-alarm-bdr": p.alarm,
    "--theme-color-alarm-text": mix(p.alarm, "#ffffff", 0.28),
    "--theme-color-alarm-10": alpha(p.alarm, 0.1),
    "--theme-color-alarm-40": alpha(p.alarm, 0.4),
    "--theme-color-critical": critical,
    "--theme-color-critical--hover": mix(critical, "#ffffff", 0.18),
    "--theme-color-critical--active": mix(critical, "#000000", 0.12),
    "--theme-color-critical--contrast": p.contrast,
    "--theme-color-critical-40": alpha(critical, 0.4),
    "--theme-color-info": p.info,
    "--theme-color-info--hover": mix(p.info, "#ffffff", 0.18),
    "--theme-color-info--active": mix(p.info, "#000000", 0.12),
    "--theme-color-info--contrast": p.contrast,
    "--theme-color-info-40": alpha(p.info, 0.4),
    "--theme-color-neutral": p.neutral,
    "--theme-color-neutral--hover": mix(p.neutral, "#ffffff", 0.12),
    "--theme-color-neutral--active": mix(p.neutral, "#000000", 0.1),
    "--theme-color-neutral--contrast": p.contrast,
    "--theme-color-neutral-40": alpha(p.neutral, 0.4),

    // --- 遮罩与投影
    "--theme-color-backdrop": schema === "dark" ? "rgba(0,0,0,0.62)" : "rgba(18,18,26,0.38)",
    "--theme-color-backdrop-3": schema === "dark" ? "rgba(0,0,0,0.8)" : "rgba(18,18,26,0.5)",
    "--theme-color-shadow-1": schema === "dark" ? "rgba(0,0,0,0.6)" : "rgba(20,20,30,0.18)",
    "--theme-color-shadow-2": schema === "dark" ? "#000000" : "rgba(20,20,30,0.22)",
    "--theme-color-shadow-3": schema === "dark" ? "rgba(0,0,0,0.6)" : "rgba(20,20,30,0.2)",

    // --- 滚动条跟着色阶走，深色主题里不该留浅灰条
    "--theme-scrollbar-thumb--background": alpha(p.fg, 0.26),
    "--theme-scrollbar-thumb--background--hover": alpha(p.fg, 0.42),
    "--theme-scrollbar-track--background": "transparent",
  }
}

// ------------------------------------------------------------------- palettes

/** Windows 11 深色。`AccentFillColorDefault` 的 Light2 色调，上面压近黑文字。 */
const WINDOWS_DARK: Palette = {
  bg: "#202020",
  fg: "#f2f2f2",
  accent: "#4cc2ff",
  dynamic: "#99ebff",
  contrast: "#0a1a24",
  success: "#6ccb5f",
  warning: "#fce100",
  alarm: "#ff99a4",
  info: "#4cc2ff",
  neutral: "#a0a0a0",
}

/** Windows 11 浅色。`AccentFillColorDefault` 的 Dark1 色调，上面压白字。 */
const WINDOWS_LIGHT: Palette = {
  bg: "#f3f3f3",
  fg: "#1a1a1a",
  accent: "#0067c0",
  dynamic: "#0078d4",
  contrast: "#ffffff",
  success: "#0f7b0f",
  warning: "#9d5d00",
  alarm: "#c42b1c",
  info: "#0067c0",
  neutral: "#5d5d5d",
}

const STYLE_ID = "mf-theme"

function tokensFor(schema: "dark" | "light"): Record<string, string> {
  return schema === "dark"
    ? buildTokens(WINDOWS_DARK, "dark", RAMP_DARK)
    : buildTokens(WINDOWS_LIGHT, "light", RAMP_LIGHT)
}

function declarations(tokens: Record<string, string>): string {
  return Object.entries(tokens)
    .map(([name, value]) => `${name}:${value};`)
    .join("")
}

/**
 * 把当前系统深浅色对应的 token 覆盖写进一个 `<style>` 元素。
 *
 * 替换而不是追加：两套配色的 token 块叠在一起时，胜出的那一份取决于
 * 插入顺序，与「当前是深色还是浅色」无关。浅色块的投影比深色软，
 * 混进去会让面板多出一圈黑边。
 */
function paint(schema: "dark" | "light"): void {
  const root = document.documentElement
  root.dataset.ixTheme = "classic"
  root.dataset.ixColorSchema = schema
  // `color-scheme` 管原生滚动条与文本光标，它们不读 CSS 变量。
  root.style.colorScheme = schema

  let style = document.getElementById(STYLE_ID) as HTMLStyleElement | null
  if (!style) {
    style = document.createElement("style")
    style.id = STYLE_ID
    document.head.appendChild(style)
  }
  style.textContent = `html[data-ix-theme][data-ix-color-schema]{${declarations(tokensFor(schema))}}`
}

/** 跟随系统深浅色，并订阅变化。由 `main.ts` 在启动最早期调一次。 */
export function initTheme(): void {
  const query = window.matchMedia("(prefers-color-scheme: dark)")
  paint(query.matches ? "dark" : "light")
  query.addEventListener("change", (event) => paint(event.matches ? "dark" : "light"))
}
