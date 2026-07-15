(() => {
  "use strict";

  const GLOBAL_NAME = "__TAURI_PLUGIN_DECORATION__";
  const VERSION = 1;
  const STYLESHEET_SELECTOR =
    'link[data-tauri-plugin-decoration-stylesheet="v1"]';
  const READY_PROPERTY = "--tauri-plugin-decoration-ready";
  const ACK_COMMAND = "plugin:decoration|frontend_ack";
  const STYLESHEET_TIMEOUT_MS = 4000;
  const CLOSED_EVENTS = new Set([
    "snap-mousemove",
    "snap-mouseenter",
    "snap-mouseleave",
    "snap-mousedown",
    "snap-mouseup",
    "snap-click",
    "macos-titlebar-state",
    "fullscreen-did-enter",
    "fullscreen-did-exit",
  ]);

  const existing = window[GLOBAL_NAME];
  if (existing?.version === VERSION) return;

  class InstallationFailure extends Error {
    constructor(code) {
      super(code);
      this.code = code;
    }
  }

  const platforms = new Map();
  let current = null;

  const isDecimalToken = (value) =>
    typeof value === "string" && /^[1-9][0-9]*$/.test(value);

  const isCurrent = (installation) => current === installation && !installation.disposed;

  const runDisposers = (installation) => {
    const disposers = installation.disposers.splice(0).reverse();
    for (const dispose of disposers) dispose();
  };

  const removeOwnedDom = (installation) => {
    installation.root?.remove();
    installation.root = null;
    installation.stylesheet?.remove();
  };

  const dispose = (installation) => {
    if (installation.disposed) return;
    installation.disposed = true;
    runDisposers(installation);
    removeOwnedDom(installation);
    if (current === installation) current = null;
  };

  const sentinelReady = () =>
    window
      .getComputedStyle(document.documentElement)
      .getPropertyValue(READY_PROPERTY)
      .trim() === "ready";

  const waitForStylesheet = async (installation) => {
    const internals = window.__TAURI_INTERNALS__;
    if (
      !internals ||
      typeof internals.convertFileSrc !== "function" ||
      typeof internals.invoke !== "function"
    ) {
      throw new InstallationFailure("tauri-internals-unavailable");
    }

    const href = internals.convertFileSrc(
      "controls.css",
      "tauri-plugin-decoration",
    );
    let link = document.querySelector(STYLESHEET_SELECTOR);
    if (link && sentinelReady()) {
      installation.stylesheet = link;
      return;
    }
    link?.remove();

    link = document.createElement("link");
    link.setAttribute("rel", "stylesheet");
    link.setAttribute("href", href);
    link.setAttribute("data-tauri-plugin-decoration-stylesheet", "v1");
    link.setAttribute("data-tauri-plugin-decoration-owned", "stylesheet");
    installation.stylesheet = link;

    await new Promise((resolve, reject) => {
      let settled = false;
      let timeoutId = null;
      const cleanup = () => {
        if (timeoutId !== null) window.clearTimeout(timeoutId);
        link.removeEventListener("load", onLoad);
        link.removeEventListener("error", onError);
      };
      const finish = (callback) => {
        if (settled) return;
        settled = true;
        cleanup();
        callback();
      };
      const onLoad = () => {
        if (!isCurrent(installation)) {
          finish(() => reject(new InstallationFailure("stale")));
        } else if (!sentinelReady()) {
          finish(() => reject(new InstallationFailure("stylesheet-sentinel")));
        } else {
          finish(resolve);
        }
      };
      const onError = () =>
        finish(() => reject(new InstallationFailure("stylesheet-load")));
      const onTimeout = () =>
        finish(() => reject(new InstallationFailure("stylesheet-timeout")));
      installation.disposers.push(() =>
        finish(() => reject(new InstallationFailure("stale"))),
      );
      link.addEventListener("load", onLoad, { once: true });
      link.addEventListener("error", onError, { once: true });
      timeoutId = window.setTimeout(onTimeout, STYLESHEET_TIMEOUT_MS);
      document.head.appendChild(link);
    });
  };

  const createRoot = (installation) => {
    const root = document.createElement("div");
    root.setAttribute("data-tauri-plugin-decoration-root", "");
    root.setAttribute("data-tauri-plugin-decoration-owned", "root");

    const titlebar = document.createElement("div");
    titlebar.setAttribute("data-tauri-decoration-tb", "");
    const drag = document.createElement("div");
    drag.setAttribute("data-tauri-drag-region", "");
    drag.setAttribute("aria-hidden", "true");
    titlebar.appendChild(drag);
    root.appendChild(titlebar);
    document.body.prepend(root);
    installation.root = root;
    return root;
  };

  const setClearance = (installation, side, value) => {
    if (
      (side !== "left" && side !== "right") ||
      typeof value !== "number" ||
      !Number.isFinite(value) ||
      value < 0
    ) {
      return false;
    }
    const property = `--tauri-plugin-decoration-${side}-clearance`;
    if (!installation.clearances.has(property)) {
      installation.clearances.add(property);
      installation.disposers.push(() =>
        document.documentElement.style.setProperty(property, "0px"),
      );
    }
    document.documentElement.style.setProperty(property, `${value}px`);
    return true;
  };

  const acknowledge = (installation, ok) =>
    window.__TAURI_INTERNALS__.invoke(ACK_COMMAND, {
      windowGeneration: installation.config.windowGeneration,
      documentToken: installation.config.documentToken,
      ok,
    });

  const fail = async (installation) => {
    if (!isCurrent(installation)) return;
    installation.disposed = true;
    current = null;
    runDisposers(installation);
    removeOwnedDom(installation);
    try {
      await acknowledge(installation, false);
    } catch {
      // Failure reporting is best-effort; the application must still request
      // explicit native restoration before revealing a failed installation.
    }
  };

  const prepare = async (installation) => {
    try {
      if (!document.head || !document.body) {
        throw new InstallationFailure("document-unavailable");
      }
      await waitForStylesheet(installation);
      if (!isCurrent(installation)) return;

      const root = createRoot(installation);
      const platformFactory = platforms.get(installation.config.platform);
      if (!platformFactory) throw new InstallationFailure("platform-unavailable");
      installation.platform = await platformFactory({
        addDisposer: (disposer) => installation.disposers.push(disposer),
        config: installation.config,
        document,
        isCurrent: () => isCurrent(installation),
        root,
        setClearance: (side, value) => setClearance(installation, side, value),
        window,
      });
      if (!isCurrent(installation)) return;

      root.setAttribute("data-tauri-plugin-decoration-prepared", "");
      try {
        await acknowledge(installation, true);
      } catch {
        throw new InstallationFailure("native-activation");
      }
      if (!isCurrent(installation)) return;

      root.removeAttribute("data-tauri-plugin-decoration-prepared");
      root.setAttribute("data-tauri-plugin-decoration-active", "");
    } catch (error) {
      const code =
        error instanceof InstallationFailure ? error.code : "platform-install";
      if (code !== "stale") await fail(installation);
    }
  };

  const install = (config) => {
    if (
      !config ||
      !isDecimalToken(config.windowGeneration) ||
      !isDecimalToken(config.documentToken) ||
      typeof config.platform !== "string"
    ) {
      return false;
    }

    if (current) {
      if (
        config.windowGeneration === current.config.windowGeneration &&
        config.documentToken === current.config.documentToken
      ) {
        return true;
      }
      dispose(current);
    }

    const installation = {
      config,
      clearances: new Set(),
      disposed: false,
      disposers: [],
      platform: null,
      root: null,
      stylesheet: null,
    };
    current = installation;
    void prepare(installation);
    return true;
  };

  const cancel = (windowGeneration, documentToken) => {
    if (
      !current ||
      current.config.windowGeneration !== windowGeneration ||
      current.config.documentToken !== documentToken
    ) {
      return false;
    }
    dispose(current);
    return true;
  };

  const dispatch = (windowGeneration, documentToken, event, payload) => {
    if (
      !current ||
      current.disposed ||
      current.config.windowGeneration !== windowGeneration ||
      current.config.documentToken !== documentToken ||
      !CLOSED_EVENTS.has(event) ||
      typeof current.platform?.handle !== "function"
    ) {
      return false;
    }
    try {
      return current.platform.handle(event, payload) === true;
    } catch {
      return false;
    }
  };

  platforms.set("macos", async (context) => {
    const property = "--decoration-traffic-light-left";
    const publishClearance = (value) => {
      if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
        return false;
      }
      context.document.documentElement.style.setProperty(property, `${value}px`);
      return context.setClearance("left", value);
    };
    const render = (state) => {
      if (
        !state ||
        typeof state !== "object" ||
        typeof state.fullscreen !== "boolean" ||
        typeof state.clearance !== "number" ||
        !Number.isFinite(state.clearance) ||
        state.clearance < 0
      ) {
        return false;
      }
      if (state.fullscreen) {
        context.root.setAttribute("data-tauri-decoration-fullscreen", "");
      } else {
        context.root.removeAttribute("data-tauri-decoration-fullscreen");
      }
      return publishClearance(state.fullscreen ? 0 : state.clearance);
    };

    context.setClearance("left", 0);
    context.addDisposer(() => publishClearance(0));
    return {
      handle(event, payload) {
        return event === "macos-titlebar-state" && render(payload);
      },
    };
  });
  window[GLOBAL_NAME] = {
    version: VERSION,
    cancel,
    dispatch,
    install,
    registerPlatform(name, factory) {
      if ((name === "windows" || name === "linux") && typeof factory === "function") {
        platforms.set(name, factory);
      }
    },
  };
})();
