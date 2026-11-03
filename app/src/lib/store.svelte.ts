import { listen } from "@tauri-apps/api/event";
import { api, errorText, type DeepLinkImport, type Traffic, type View } from "./api";

export type Screen =
  | "home"
  | "nodes"
  | "settings"
  | "profiles"
  | "add"
  | "rules"
  | "advanced"
  | "logs";

/** Where each screen's back button leads. */
export const PARENT: Record<Screen, Screen> = {
  home: "home",
  nodes: "home",
  settings: "home",
  profiles: "home",
  add: "profiles",
  rules: "settings",
  advanced: "settings",
  logs: "settings",
};

export type Theme = "system" | "light" | "dark";

class AppStore {
  theme = $state<Theme>((localStorage.getItem("theme") as Theme | null) ?? "system");
  view = $state<View | null>(null);
  traffic = $state<Traffic>({ up: 0, down: 0 });
  screen = $state<Screen>("home");
  /** Direction of the last navigation, for the slide transition. */
  forward = $state(true);
  toast = $state<{ text: string; tone: "error" | "info" } | null>(null);
  #toastTimer: ReturnType<typeof setTimeout> | undefined;

  get activeProfile() {
    return this.view?.profiles.find((p) => p.active) ?? null;
  }

  async init() {
    await listen<View>("view", (event) => {
      const previous = this.view?.phase;
      this.view = event.payload;
      if (event.payload.phase !== "connected") this.traffic = { up: 0, down: 0 };
      if (event.payload.error && previous !== event.payload.phase) {
        this.showToast(event.payload.error, "error");
      }
    });
    await listen<Traffic>("traffic", (event) => {
      this.traffic = event.payload;
    });
    await listen<DeepLinkImport>("deep-link-import", () => this.#takeImport());
    this.view = await api.getView();
    await this.#takeImport();
  }

  /** A `clash://install-config` link waiting for the user's confirmation. */
  pendingImport = $state<DeepLinkImport | null>(null);

  async #takeImport() {
    const link = await api.takeDeepLink();
    if (link) this.pendingImport = link;
  }

  setTheme(theme: Theme) {
    this.theme = theme;
    localStorage.setItem("theme", theme);
    applyTheme(theme);
  }

  go(screen: Screen) {
    this.forward = screen !== "home";
    this.screen = screen;
  }

  back(to: Screen = PARENT[this.screen]) {
    this.forward = false;
    this.screen = to;
  }

  showToast(text: string, tone: "error" | "info" = "info") {
    clearTimeout(this.#toastTimer);
    this.toast = { text, tone };
    this.#toastTimer = setTimeout(() => (this.toast = null), tone === "error" ? 6000 : 2500);
  }

  /** Run a command; surface failures as a toast. Returns null on failure. */
  async run<T>(action: () => Promise<T>): Promise<T | null> {
    try {
      return await action();
    } catch (error) {
      this.showToast(errorText(error), "error");
      return null;
    }
  }
}

function applyTheme(theme: Theme) {
  if (theme === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
}

export const app = new AppStore();
applyTheme(app.theme);
