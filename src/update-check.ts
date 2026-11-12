/**
 * 更新检查。
 *
 * 只有一件事：查 GitHub 上最新的 Release，比当前版本新就在右下角按钮上提示。
 * 检查失败一律静默——网络不通、仓库还没公开、API 限流都不该打扰用户。
 *
 * HTTP 放在前端而不是 Rust：Tauri 的 reqwest 会给 release 体积加 1 MB 以上，
 * 而这里只需要一次 GET。版本号比较放在 Rust（`version.rs`），因为那是纯逻辑，
 * 必须能单测。
 */

import { openUrl } from "@tauri-apps/plugin-opener";

import { api } from "./api";

/** 仓库与 API 地址。仓库改名或迁移时只改这里。 */
const RELEASES_LATEST =
  "https://api.github.com/repos/NaimioChan/mediaforge/releases/latest";

/** 一次检查的结果。`url` 为空表示没有可跳转的发布页。 */
interface UpdateFound {
  version: string;
  url: string;
}

let pending: Promise<void> | null = null;
let found: UpdateFound | null = null;

function button(): HTMLElement | null {
  return document.getElementById("btn-version");
}

function render(current: string): void {
  const el = button();
  if (!el) return;

  if (found) {
    el.textContent = `可更新 ${found.version}`;
    el.classList.add("has-update");
    el.setAttribute("title", `当前 ${current}，点击查看 ${found.version}`);
  } else {
    el.textContent = `v${current}`;
    el.classList.remove("has-update");
    el.setAttribute("title", "点击检查更新");
  }
}

/** 查一次最新 Release。任何异常都返回 null，调用方不需要处理错误。 */
async function fetchLatest(current: string): Promise<UpdateFound | null> {
  const response = await fetch(RELEASES_LATEST, {
    headers: { Accept: "application/vnd.github+json" },
  });
  if (!response.ok) return null;

  const body = (await response.json()) as {
    tag_name?: unknown;
    html_url?: unknown;
    draft?: unknown;
    prerelease?: unknown;
  };
  if (body.draft === true || body.prerelease === true) return null;
  if (typeof body.tag_name !== "string") return null;

  const newer = await api.isVersionNewer(body.tag_name, current);
  if (!newer) return null;

  return {
    version: body.tag_name.replace(/^v/i, ""),
    url: typeof body.html_url === "string" ? body.html_url : "",
  };
}

/** 检查更新。重复调用复用同一次请求。 */
export function checkForUpdate(current: string): Promise<void> {
  if (pending) return pending;

  pending = (async () => {
    try {
      found = await fetchLatest(current);
    } catch {
      // 静默：断网、仓库未公开、限流都不该在界面上留下痕迹。
      found = null;
    }
    render(current);
    pending = null;
  })();

  return pending;
}

/** 装上按钮的点击行为：有更新就打开发布页，否则重新检查一次。 */
export function initUpdateButton(current: string): void {
  const el = button();
  if (!el) return;

  render(current);

  el.addEventListener("click", () => {
    void (async () => {
      if (found?.url) {
        try {
          await openUrl(found.url);
        } catch {
          // 打不开浏览器时不做提示：地址已经在按钮的 title 里。
        }
        return;
      }
      await checkForUpdate(current);
    })();
  });
}
