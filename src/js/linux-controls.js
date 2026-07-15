(() => {
  "use strict";

  const runtime = window.__TAURI_PLUGIN_DECORATION__;
  if (!runtime) return;

  runtime.registerPlatform("linux", async (context) => {
    const tauriWindow = window.__TAURI__?.window?.getCurrentWindow?.();
    if (!tauriWindow) throw new Error("Tauri window API unavailable");
    if (typeof tauriWindow.isFullscreen !== "function") {
      throw new Error("Tauri fullscreen-state API unavailable");
    }

    const pngDataUrl = (value) =>
      typeof value === "string" &&
      /^data:image\/png;base64,[A-Za-z0-9+/]+={0,2}$/.test(value)
        ? value
        : null;
    const config = context.config;
    let maximize = null;
    let maximized = false;
    let fullscreen = false;
    let windowStateSequence = 0;

    const createHost = (side) => {
      const host = document.createElement("div");
      host.setAttribute("data-tauri-decoration-controls", "");
      host.setAttribute("data-tauri-decoration-platform", "linux");
      host.setAttribute("data-tauri-decoration-side", side);
      host.setAttribute("role", "group");
      host.setAttribute("aria-label", "Window controls");
      context.root.appendChild(host);
      return host;
    };

    const createButton = (host, name, label, action) => {
      const state = { actionLocked: false };
      const button = document.createElement("button");
      button.setAttribute("type", "button");
      button.setAttribute("data-tauri-decoration-control", name);
      button.setAttribute("data-tauri-decoration-icon", name);
      button.setAttribute("aria-label", label);
      const image = document.createElement("img");
      image.setAttribute("alt", "");
      image.setAttribute("aria-hidden", "true");
      const source = pngDataUrl(config.icons[name]);
      if (source) image.setAttribute("src", source);
      else button.setAttribute("data-tauri-decoration-icon-fallback", "");
      image.addEventListener("error", () => {
        if (!context.isCurrent()) return;
        image.removeAttribute("src");
        button.setAttribute("data-tauri-decoration-icon-fallback", "");
      });
      button.appendChild(image);

      button.addEventListener("click", (event) => {
        event.preventDefault();
        if (state.actionLocked || !context.isCurrent()) {
          return;
        }
        state.actionLocked = true;
        Promise.resolve()
          .then(action)
          .catch((error) => {
            console.error(
              `tauri-plugin-decoration: Linux ${name} action failed`,
              error,
            );
          })
          .finally(() => {
            state.actionLocked = false;
          });
      });
      host.appendChild(button);
      return { button, image };
    };

    const renderMaximized = (maximized) => {
      if (!maximize) return;
      const icon = maximized ? "restore" : "maximize";
      const source = pngDataUrl(config.icons[icon]);
      maximize.button.setAttribute("data-tauri-decoration-icon", icon);
      maximize.button.setAttribute(
        "aria-label",
        maximized ? "Restore window size" : "Maximize window size",
      );
      if (source) {
        maximize.image.setAttribute("src", source);
        maximize.button.removeAttribute("data-tauri-decoration-icon-fallback");
      } else {
        maximize.image.removeAttribute("src");
        maximize.button.setAttribute("data-tauri-decoration-icon-fallback", "");
      }
    };

    const clearance = (side) => {
      const count = config.layout[side].length;
      return count === 0 ? 0 : count * 32 + (count - 1) * 6 + 16;
    };

    const publishClearances = () => {
      context.setClearance("left", fullscreen ? 0 : clearance("left"));
      context.setClearance("right", fullscreen ? 0 : clearance("right"));
    };

    const renderFullscreen = (value) => {
      fullscreen = Boolean(value);
      if (fullscreen) {
        context.root.setAttribute("data-tauri-decoration-fullscreen", "");
      } else {
        context.root.removeAttribute("data-tauri-decoration-fullscreen");
      }
      publishClearances();
    };

    const actions = {
      minimize: (host) =>
        createButton(host, "minimize", "Minimize window", () =>
          tauriWindow.minimize(),
        ),
      maximize: (host) => {
        maximize = createButton(host, "maximize", "Maximize window size", () =>
          tauriWindow.toggleMaximize(),
        );
      },
      close: (host) =>
        createButton(host, "close", "Close window", () => tauriWindow.close()),
    };
    for (const side of ["left", "right"]) {
      if (config.layout[side].length === 0) continue;
      const host = createHost(side);
      for (const control of config.layout[side]) actions[control](host);
    }
    renderMaximized(maximized);
    publishClearances();

    const refreshWindowState = async () => {
      const sequence = ++windowStateSequence;
      const [nextMaximized, nextFullscreen] = await Promise.all([
        maximize ? tauriWindow.isMaximized() : false,
        tauriWindow.isFullscreen(),
      ]);
      if (context.isCurrent() && sequence === windowStateSequence) {
        maximized = Boolean(nextMaximized);
        renderMaximized(maximized);
        renderFullscreen(nextFullscreen);
      }
    };

    const reportRefreshFailure = (error) => {
      console.error(
        "tauri-plugin-decoration: Linux window-state refresh failed",
        error,
      );
    };

    await refreshWindowState();

    const onResize = () => {
      void refreshWindowState().catch((error) => {
        reportRefreshFailure(error);
      });
    };
    window.addEventListener("resize", onResize);
    context.addDisposer(() => {
      windowStateSequence += 1;
      window.removeEventListener("resize", onResize);
    });

    return {};
  });
})();
