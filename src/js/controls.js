(() => {
  "use strict";

  const runtime = window.__TAURI_PLUGIN_DECORATION__;
  if (!runtime) return;

  runtime.registerPlatform("windows", async (context) => {
    const tauriWindow = window.__TAURI__?.window?.getCurrentWindow?.();
    if (!tauriWindow) throw new Error("Tauri window API unavailable");

    const controls = new Set(
      Array.isArray(context.config.controls) ? context.config.controls : [],
    );
    const host = document.createElement("div");
    host.setAttribute("data-tauri-decoration-controls", "");
    host.setAttribute("data-tauri-decoration-platform", "windows");
    host.setAttribute("role", "group");
    host.setAttribute("aria-label", "Window controls");
    context.root.appendChild(host);

    const buttons = new Map();
    let activeControl = null;
    let refreshSequence = 0;

    const setActive = (control) => {
      activeControl = buttons.has(control) ? control : null;
      for (const [name, button] of buttons) {
        if (name === activeControl) {
          button.setAttribute("data-tauri-decoration-hover", "");
        } else {
          button.removeAttribute("data-tauri-decoration-hover");
        }
      }
    };

    const createButton = (name, label, glyph, action) => {
      const button = document.createElement("button");
      button.setAttribute("type", "button");
      button.setAttribute("data-tauri-decoration-control", name);
      button.setAttribute("aria-label", label);
      button.textContent = glyph;

      let actionLocked = false;
      const run = async () => {
        if (actionLocked || !context.isCurrent()) return;
        actionLocked = true;
        setActive(null);
        try {
          await action();
        } catch (error) {
          console.error(
            `tauri-plugin-decoration: Windows ${name} action failed`,
            error,
          );
        } finally {
          actionLocked = false;
        }
      };
      button.addEventListener("mouseenter", () => setActive(name));
      button.addEventListener("mouseleave", () => {
        if (activeControl === name) setActive(null);
      });
      button.addEventListener("click", (event) => {
        event.preventDefault();
        void run();
      });
      buttons.set(name, button);
      host.appendChild(button);
      return { button, run };
    };

    if (controls.has("minimize")) {
      createButton("minimize", "Minimize window", "\uE921", () =>
        tauriWindow.minimize(),
      );
    }

    let maximize = null;
    if (controls.has("maximize")) {
      maximize = createButton("maximize", "Maximize window size", "\uE922", () =>
        tauriWindow.toggleMaximize(),
      );
    }

    if (controls.has("close")) {
      createButton("close", "Close window", "\uE8BB", () => tauriWindow.close());
    }
    context.setClearance("left", 0);
    context.setClearance("right", buttons.size * 58);

    const renderMaximized = (maximized) => {
      if (!maximize) return;
      const glyph = maximized ? "\uE923" : "\uE922";
      maximize.button.textContent = glyph;
      maximize.button.setAttribute(
        "aria-label",
        maximized ? "Restore window size" : "Maximize window size",
      );
    };

    const renderFullscreen = (fullscreen) => {
      if (fullscreen) {
        context.root.setAttribute("data-tauri-decoration-fullscreen", "");
        setActive(null);
        context.setClearance("right", 0);
      } else {
        context.root.removeAttribute("data-tauri-decoration-fullscreen");
        context.setClearance("right", buttons.size * 58);
      }
    };

    const refreshWindowState = async () => {
      const sequence = ++refreshSequence;
      const [maximized, fullscreen] = await Promise.all([
        maximize ? tauriWindow.isMaximized() : false,
        tauriWindow.isFullscreen(),
      ]);
      if (context.isCurrent() && sequence === refreshSequence) {
        renderMaximized(Boolean(maximized));
        renderFullscreen(Boolean(fullscreen));
      }
    };

    await refreshWindowState();
    const onResize = () => {
      void refreshWindowState().catch((error) => {
        console.error(
          "tauri-plugin-decoration: Windows window-state refresh failed",
          error,
        );
      });
    };
    window.addEventListener("resize", onResize);
    context.addDisposer(() => {
      refreshSequence += 1;
      window.removeEventListener("resize", onResize);
    });

    const hitTest = (payload) => {
      if (
        !Array.isArray(payload) ||
        payload.length !== 2 ||
        !payload.every((coordinate) => Number.isFinite(coordinate))
      ) {
        return false;
      }
      const devicePixelRatio =
        Number.isFinite(window.devicePixelRatio) && window.devicePixelRatio > 0
          ? window.devicePixelRatio
          : 1;
      const element = document.elementFromPoint(
        payload[0] / devicePixelRatio,
        payload[1] / devicePixelRatio,
      );
      const button = element?.closest?.("[data-tauri-decoration-control]");
      setActive(button?.getAttribute("data-tauri-decoration-control") ?? null);
      return true;
    };

    return {
      handle(event, payload) {
        switch (event) {
          case "snap-mousemove":
            return hitTest(payload);
          case "snap-mouseenter":
          case "snap-mousedown":
          case "snap-mouseup":
            setActive("maximize");
            return Boolean(maximize);
          case "snap-mouseleave":
            setActive(null);
            return Boolean(maximize);
          case "snap-click":
            if (!maximize) return false;
            void maximize.run();
            return true;
          case "fullscreen-did-enter":
            refreshSequence += 1;
            renderFullscreen(true);
            return true;
          case "fullscreen-did-exit":
            refreshSequence += 1;
            renderFullscreen(false);
            return true;
          default:
            return false;
        }
      },
    };
  });
})();
