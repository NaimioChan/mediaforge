/**
 * Fluent 2 组件注册与主题。
 *
 * 两个必须记住的点：
 *
 * 1. 组件注册走 `@fluentui/web-components/<component>/define.js`。
 *    v3.2.1 的 package.json exports 只有一级通配（星号通配）指向 define.js，
 *    没有 `button.js` 这种写法（README 里写的那种）——按 README 写会解析失败。
 *
 * 2. 必须调用 `setTheme(...)`，否则组件没有 CSS 变量、完全不显样式。
 *    主题对象来自 `@fluentui/tokens`，不是 web-components 本身：
 *    web-components 只导出 `setTheme`，不导出 `webLightTheme`。
 */

import { setTheme } from "@fluentui/web-components/theme/index.js";
import { webLightTheme, webDarkTheme } from "@fluentui/tokens";

// 组件按需注册。这里锁定的 18 个覆盖界面全部控件。
import "@fluentui/web-components/button/define.js";
import "@fluentui/web-components/checkbox/define.js";
import "@fluentui/web-components/divider/define.js";
import "@fluentui/web-components/dropdown/define.js";
import "@fluentui/web-components/option/define.js";
import "@fluentui/web-components/progress-bar/define.js";
import "@fluentui/web-components/slider/define.js";
import "@fluentui/web-components/switch/define.js";
import "@fluentui/web-components/text-input/define.js";
import "@fluentui/web-components/dialog/define.js";
import "@fluentui/web-components/dialog-body/define.js";
import "@fluentui/web-components/badge/define.js";
import "@fluentui/web-components/message-bar/define.js";
import "@fluentui/web-components/tooltip/define.js";
import "@fluentui/web-components/spinner/define.js";
import "@fluentui/web-components/field/define.js";
import "@fluentui/web-components/label/define.js";
import "@fluentui/web-components/text/define.js";

/** 当前是否深色。 */
let darkQuery: MediaQueryList | null = null;

function applyTheme(dark: boolean): void {
  setTheme(dark ? webDarkTheme : webLightTheme);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

/**
 * 初始化主题：跟随系统深浅色，并监听变化。
 *
 * WebView2 会把 `prefers-color-scheme` 反映成系统设置，
 * 因此只需要一个 matchMedia 监听，不必额外走 Tauri 命令。
 */
export function initTheme(): void {
  darkQuery = window.matchMedia("(prefers-color-scheme: dark)");
  applyTheme(darkQuery.matches);

  darkQuery.addEventListener("change", (event) => {
    applyTheme(event.matches);
  });
}

export function isDark(): boolean {
  return darkQuery?.matches ?? false;
}
