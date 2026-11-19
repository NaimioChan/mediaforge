/**
 * Siemens iX 组件注册与主题。
 *
 * 三个必须记住的点：
 *
 * 1. **CSS 与元素定义是两件事。** `@siemens/ix/dist/siemens-ix/siemens-ix.css`
 *    里既有组件样式也有两套主题 token 块；但组件本身是 web components，
 *    不调 `defineCustomElements()` 页面上只有一堆无样式的空标签。
 *
 * 2. **按需注册，不要用 `@siemens/ix/loader`。** 那个 loader 把全部 150 多个
 *    组件一次性注册进来，前端产物从 232 KB 涨到 2.2 MB。每个
 *    `@siemens/ix/components/ix-<name>.js` 都导出 `defineCustomElement`，
 *    只注册自己（连带它内部引用的兄弟组件），下面列出界面真正用到的那些。
 *
 * 3. **`ix-icon` 不在 `@siemens/ix` 里。** 它是独立包 `@siemens/ix-icons`
 *    的元素，必须单独注册，否则 `ix-icon` 不被升级，图标位置只有一个高度为 0
 *    的空元素——看起来像「图标没显示」，实际是「元素没定义」。
 *
 * 主题由 `<html>` 上的两个属性决定：`data-ix-theme="classic"` 打开 iX 的 token
 * 块，`data-ix-color-schema` 决定深还是浅。具体色值由 `theme.ts` 按 Windows 11
 * Fluent 的配色算出覆盖块写进 `<head>`，深浅色跟随系统——WebView2 会把 Windows
 * 的应用主题反映成 `prefers-color-scheme`，不必自己读注册表。
 * `index.html` 的 head 里有一段内联脚本先定 schema 与底色，避免首帧闪成 classic 的蓝紫。
 */

import "@siemens/ix/dist/siemens-ix/siemens-ix.css";
import { defineCustomElements as defineIcons } from "@siemens/ix-icons/loader";

// 每个 import 只为副作用：注册对应的自定义元素。
// `components/ix-<name>.js` 只导出 `defineCustomElement`，**不会自己调**，
// 所以要显式调一遍；`customElements.get` 的判重由 iX 自己做了，
// 同一元素被多个兄弟组件连带注册时不会重复定义。
import { defineCustomElement as defineButton } from "@siemens/ix/components/ix-button.js";
import { defineCustomElement as defineDivider } from "@siemens/ix/components/ix-divider.js";
import { defineCustomElement as defineEmptyState } from "@siemens/ix/components/ix-empty-state.js";
import { defineCustomElement as defineIconButton } from "@siemens/ix/components/ix-icon-button.js";
import { defineCustomElement as defineInput } from "@siemens/ix/components/ix-input.js";
import { defineCustomElement as defineNumberInput } from "@siemens/ix/components/ix-number-input.js";
import { defineCustomElement as defineProgressIndicator } from "@siemens/ix/components/ix-progress-indicator.js";
import { defineCustomElement as defineSelect } from "@siemens/ix/components/ix-select.js";
import { defineCustomElement as defineSelectItem } from "@siemens/ix/components/ix-select-item.js";
import { defineCustomElement as defineSlider } from "@siemens/ix/components/ix-slider.js";
import { defineCustomElement as defineToggle } from "@siemens/ix/components/ix-toggle.js";

// 上面这些组件的 shadow DOM 里会用到下面几个，必须一并注册，
// 否则它们出现在 shadow root 里时是不升级的空标签。
import { defineCustomElement as defineSpinner } from "@siemens/ix/components/ix-spinner.js";
import { defineCustomElement as defineTooltip } from "@siemens/ix/components/ix-tooltip.js";
import { defineCustomElement as defineTypography } from "@siemens/ix/components/ix-typography.js";

for (const define of [
  defineButton,
  defineDivider,
  defineEmptyState,
  defineIconButton,
  defineInput,
  defineNumberInput,
  defineProgressIndicator,
  defineSelect,
  defineSelectItem,
  defineSlider,
  defineToggle,
  defineSpinner,
  defineTooltip,
  defineTypography,
]) {
  define();
}

void defineIcons();
