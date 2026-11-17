/**
 * 图标。
 *
 * 图标文件是 `@siemens/ix-icons` 的副本（MIT，Siemens AG），放在
 * `src/assets/icons/`，随包分发。复制而不是按名字引用，有两个原因：
 *
 * 1. `ix-icon` 拿到名字时不是查注册表，而是拼出 `svg/<name>.svg` 去 fetch。
 *    那个目录不在打包产物里，于是每个按名字给的图标都 fetch 失败，
 *    回退成 iX 的错误符号（一个带叉的方块）。
 * 2. 仓库里的文件满足离线要求：任何时刻都不发网络请求，也不依赖某个包的
 *    内部资源布局；换图标时 diff 里看得见。
 *
 * **data URL 里的 SVG 必须是原始文本，不能 `encodeURIComponent`。**
 * iX 内部只调 `decodeURI`（不是 `decodeURIComponent`），它不还原 `%3D`
 * 这类保留字符，于是编码过的 SVG 会被解析成属性字面量为 `width%3d"512px"`
 * 的 `<svg>`，每条 path 丢掉 `d`，渲染出来是一个空框。
 * iX 自己的图标 data URL 也是原始文本，原因就在这里。
 * 这些文件里不含 `#` 与 `%`，原始 URL 是安全的。
 *
 * 加图标：从 `node_modules/@siemens/ix-icons/svg/<name>.svg` 复制到
 * `src/assets/icons/`，在下面 import。名字没登记是编译错误，而不是一个空白方块。
 */

import add from "./assets/icons/add.svg?raw";
import closeSmall from "./assets/icons/close-small.svg?raw";
import error from "./assets/icons/error.svg?raw";
import folderOpen from "./assets/icons/folder-open.svg?raw";
import play from "./assets/icons/play.svg?raw";
import refresh from "./assets/icons/refresh.svg?raw";
import stop from "./assets/icons/stop.svg?raw";
import trashcan from "./assets/icons/trashcan.svg?raw";
import upload from "./assets/icons/upload.svg?raw";

/**
 * `ix-icon` 接受名字或 data URL，`data:image/svg+xml` 前缀是让它就地解析
 * 而不是去 fetch 的开关。
 *
 * XML 声明要去掉：iX 用 `DOMParser` 按 `text/html` 解析，`<?xml …?>` 在那里
 * 不是声明而是 bogus comment，而这些图标文件大多以它开头。
 */
const asDataUrl = (svg: string): string =>
  `data:image/svg+xml;utf8,${svg.replace(/<\?xml[^>]*\?>/g, "").trim()}`;

/** 界面用到的图标。 */
export const ICON = {
  add: asDataUrl(add),
  addFolder: asDataUrl(folderOpen),
  play: asDataUrl(play),
  stop: asDataUrl(stop),
  trash: asDataUrl(trashcan),
  upload: asDataUrl(upload),
  close: asDataUrl(closeSmall),
  error: asDataUrl(error),
  refresh: asDataUrl(refresh),
} as const;

export type IconName = (typeof ICON)[keyof typeof ICON];
