import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import { createDomWindow, spy } from "./dom-harness.mjs";

const root = fileURLToPath(new URL("../../", import.meta.url));
const titlebarSource = readFileSync(`${root}/src/js/titlebar.js`, "utf8");
const windowsSource = readFileSync(`${root}/src/js/controls.js`, "utf8");
const linuxSource = readFileSync(`${root}/src/js/linux-controls.js`, "utf8");
const stylesheet = readFileSync(`${root}/src/css/controls.css`, "utf8");
const exampleStylesheet = readFileSync(
  `${root}/examples/tauri-app/src/styles.css`,
  "utf8",
);
const exampleAppSource = readFileSync(
  `${root}/examples/tauri-app/src/App.tsx`,
  "utf8",
);

const plain = (value) => JSON.parse(JSON.stringify(value));

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function createHarness({
  invokeImpl,
  isFullscreen = spy(async () => false),
  isMaximized = spy(async () => false),
  setTimeoutImpl,
} = {}) {
  const window = createDomWindow();
  if (setTimeoutImpl) window.setTimeout = setTimeoutImpl;
  const consoleError = spy();
  window.console = { ...console, error: consoleError };
  let sentinelReady = false;
  const acknowledgements = [];
  const invoke = spy(async (command, args) => {
    acknowledgements.push({ command, args });
    return invokeImpl?.(command, args);
  });
  const windowApi = {
    minimize: spy(async () => {}),
    toggleMaximize: spy(async () => {}),
    close: spy(async () => {}),
    isFullscreen,
    isMaximized,
  };

  window.__TAURI_INTERNALS__ = {
    convertFileSrc: spy(
      () => "tauri-plugin-decoration://localhost/controls.css",
    ),
    invoke,
  };
  window.__TAURI__ = { window: { getCurrentWindow: () => windowApi } };
  window.getComputedStyle = () => ({
    getPropertyValue: (name) =>
      name === "--tauri-plugin-decoration-ready" && sentinelReady ? "ready" : "",
  });

  window.eval(titlebarSource);
  return {
    acknowledgements,
    consoleError,
    invoke,
    loadPlatform(platform) {
      window.eval(platform === "windows" ? windowsSource : linuxSource);
    },
    setSentinelReady(value = true) {
      sentinelReady = value;
    },
    window,
    windowApi,
  };
}

async function stylesheetLink(window) {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const link = window.document.querySelector(
      "link[data-tauri-plugin-decoration-stylesheet]",
    );
    if (link) return link;
    await Promise.resolve();
  }
  throw new Error("stylesheet link was not installed");
}

async function waitForCalls(callable, count) {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    if (callable.calls.length >= count) return;
    await Promise.resolve();
  }
  throw new Error(`expected ${count} calls, observed ${callable.calls.length}`);
}

async function waitForCondition(condition, message) {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    if (condition()) return;
    await Promise.resolve();
  }
  throw new Error(message);
}

function config(documentToken, platform = "windows") {
  const value = {
    windowGeneration: "7",
    documentToken: String(documentToken),
    platform,
    controls: ["minimize", "maximize", "close"],
    icons: {
      minimize: "data:image/png;base64,bWlu",
      maximize: "data:image/png;base64,bWF4",
      restore: "data:image/png;base64,cmVzdG9yZQ==",
      close: "data:image/png;base64,Y2xvc2U=",
    },
  };
  if (platform === "linux") {
    value.layout = { left: [], right: [...value.controls] };
  }
  return value;
}

async function loadStylesheet(harness, installed = true) {
  assert.equal(installed, true);
  const expectedAcknowledgements = harness.invoke.calls.length + 1;
  const link = await stylesheetLink(harness.window);
  harness.setSentinelReady();
  link.dispatchEvent(new harness.window.Event("load"));
  await waitForCalls(harness.invoke, expectedAcknowledgements);
  await waitForCondition(
    () =>
      harness.window.document.querySelector(
        "[data-tauri-plugin-decoration-active]",
      ) !== null,
    "decoration did not become active",
  );
}

describe("embedded decoration runtime", () => {
  it("waits for the fixed stylesheet and initial maximize state before acknowledgement", async () => {
    const harness = createHarness({ isMaximized: spy(async () => true) });
    harness.loadPlatform("windows");

    assert.equal(
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(11)),
      true,
    );
    await stylesheetLink(harness.window);
    assert.equal(harness.invoke.calls.length, 0);

    await loadStylesheet(harness);
    assert.deepEqual(plain(harness.acknowledgements), [
      {
        command: "plugin:decoration|frontend_ack",
        args: {
          windowGeneration: "7",
          documentToken: "11",
          ok: true,
        },
      },
    ]);

    const maximize = harness.window.document.querySelector(
      '[data-tauri-decoration-control="maximize"]',
    );
    assert.ok(maximize);
    assert.equal(maximize.getAttribute("type"), "button");
    assert.equal(maximize.getAttribute("aria-label"), "Restore window size");
    assert.equal(maximize.textContent, "\uE923");
    assert.equal(
      harness.window.document.documentElement.style.getPropertyValue(
        "--tauri-plugin-decoration-right-clearance",
      ),
      "174px",
    );
    assert.ok(
      harness.window.document.querySelector(
        "[data-tauri-plugin-decoration-active]",
      ),
    );
  });

  it("completes stylesheet readiness while hidden pages suspend animation frames", async () => {
    let timeoutCallback;
    const harness = createHarness({
      setTimeoutImpl(callback) {
        timeoutCallback = callback;
        return 1;
      },
    });
    const suspendedAnimationFrames = spy();
    harness.window.requestAnimationFrame = suspendedAnimationFrames;
    harness.loadPlatform("windows");

    assert.equal(
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(12)),
      true,
    );
    const link = await stylesheetLink(harness.window);
    harness.setSentinelReady();
    link.dispatchEvent(new harness.window.Event("load"));
    await waitForCalls(harness.invoke, 1);

    assert.equal(typeof timeoutCallback, "function");
    assert.equal(suspendedAnimationFrames.calls.length, 0);
    assert.equal(harness.acknowledgements[0].args.ok, true);
  });

  it("acknowledges stylesheet failure and removes every partial owned node", async () => {
    const harness = createHarness();
    harness.loadPlatform("windows");
    assert.equal(
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(12)),
      true,
    );
    const link = await stylesheetLink(harness.window);

    link.dispatchEvent(new harness.window.Event("error"));
    await waitForCalls(harness.invoke, 1);
    assert.deepEqual(plain(harness.acknowledgements[0]), {
      command: "plugin:decoration|frontend_ack",
      args: {
        windowGeneration: "7",
        documentToken: "12",
        ok: false,
      },
    });
    assert.equal(
      harness.window.document.querySelector("[data-tauri-plugin-decoration-owned]"),
      null,
    );
  });

  it("cancels only the exact pending installation and removes owned resources", async () => {
    const harness = createHarness();
    harness.loadPlatform("windows");
    assert.equal(
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(14)),
      true,
    );
    await stylesheetLink(harness.window);

    const runtime = harness.window.__TAURI_PLUGIN_DECORATION__;
    assert.equal(runtime.cancel("7", "13"), false);
    assert.equal(runtime.cancel("7", "14"), true);
    assert.equal(
      harness.window.document.querySelector("[data-tauri-plugin-decoration-owned]"),
      null,
    );
  });

  it("clears owned titlebar space when an active installation is cancelled", async () => {
    const harness = createHarness();
    harness.loadPlatform("windows");
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(15)),
    );
    const style = harness.window.document.documentElement.style;
    assert.equal(
      style.getPropertyValue("--tauri-plugin-decoration-right-clearance"),
      "174px",
    );

    assert.equal(harness.window.__TAURI_PLUGIN_DECORATION__.cancel("7", "15"), true);
    assert.equal(
      style.getPropertyValue("--tauri-plugin-decoration-right-clearance"),
      "0px",
    );
    assert.equal(
      harness.window.document.querySelector("[data-tauri-plugin-decoration-owned]"),
      null,
      "restoration must remove the loaded plugin stylesheet as well as the controls",
    );
  });

  it("bounds stylesheet preparation before a hidden-window fallback can show", async () => {
    let timeoutCallback;
    const harness = createHarness({
      setTimeoutImpl(callback) {
        timeoutCallback = callback;
        return 1;
      },
    });
    harness.loadPlatform("windows");
    assert.equal(
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(13)),
      true,
    );
    await stylesheetLink(harness.window);

    assert.equal(typeof timeoutCallback, "function");
    timeoutCallback();
    await waitForCalls(harness.invoke, 1);
    assert.equal(harness.acknowledgements[0].args.ok, false);
    assert.equal(
      harness.window.document.querySelector(
        "link[data-tauri-plugin-decoration-stylesheet]",
      ),
      null,
    );
  });

  it("prevents a delayed old document from overwriting the current controls", async () => {
    const firstMaximized = deferred();
    const secondMaximized = deferred();
    const isMaximized = spy(async () => false);
    isMaximized
      .mockImplementationOnce(() => firstMaximized.promise)
      .mockImplementationOnce(() => secondMaximized.promise);
    const harness = createHarness({ isMaximized });
    harness.loadPlatform("windows");

    assert.equal(
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(20)),
      true,
    );
    const firstLink = await stylesheetLink(harness.window);
    harness.setSentinelReady();
    firstLink.dispatchEvent(new harness.window.Event("load"));
    await waitForCalls(isMaximized, 1);

    assert.equal(
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(21)),
      true,
    );
    const secondLink = await stylesheetLink(harness.window);
    secondLink.dispatchEvent(new harness.window.Event("load"));
    await waitForCalls(isMaximized, 2);
    secondMaximized.resolve(true);
    await waitForCalls(harness.invoke, 1);
    firstMaximized.resolve(false);
    await Promise.resolve();
    await Promise.resolve();

    const maximize = harness.window.document.querySelector(
      '[data-tauri-decoration-control="maximize"]',
    );
    assert.equal(maximize.getAttribute("aria-label"), "Restore window size");
    assert.equal(
      harness.acknowledgements.filter(({ args }) => args.ok).length,
      1,
    );
    assert.equal(harness.acknowledgements.at(-1).args.documentToken, "21");
  });

  it("routes only closed events for the exact active target", async () => {
    const harness = createHarness();
    harness.loadPlatform("windows");
    const installation = harness.window.__TAURI_PLUGIN_DECORATION__.install(config(30));
    await loadStylesheet(harness, installation);

    const runtime = harness.window.__TAURI_PLUGIN_DECORATION__;
    assert.equal(runtime.dispatch("7", "29", "snap-click", null), false);
    assert.equal(runtime.dispatch("7", "30", "unknown", null), false);
    assert.equal(runtime.dispatch("7", "30", "snap-click", null), true);
    await Promise.resolve();
    assert.equal(harness.windowApi.toggleMaximize.calls.length, 1);
  });

  it("reports Windows action failures and allows a later retry", async () => {
    const harness = createHarness();
    harness.windowApi.minimize.implementation = async () => {
      throw new Error("denied");
    };
    harness.loadPlatform("windows");
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(31)),
    );

    const minimize = harness.window.document.querySelector(
      '[data-tauri-decoration-control="minimize"]',
    );
    minimize.dispatchEvent(new harness.window.Event("click"));
    await waitForCalls(harness.consoleError, 1);

    assert.match(harness.consoleError.calls[0][0], /Windows minimize action failed/);
    assert.equal(minimize.getAttribute("disabled"), null);

    harness.windowApi.minimize.implementation = async () => {};
    minimize.dispatchEvent(new harness.window.Event("click"));
    await waitForCalls(harness.windowApi.minimize, 2);
  });

  it("hides Windows controls and clearances for native fullscreen events", async () => {
    const harness = createHarness();
    harness.loadPlatform("windows");
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(32)),
    );

    const runtime = harness.window.__TAURI_PLUGIN_DECORATION__;
    const root = harness.window.document.querySelector(
      "[data-tauri-plugin-decoration-root]",
    );
    const style = harness.window.document.documentElement.style;

    assert.equal(runtime.dispatch("7", "32", "fullscreen-did-enter", null), true);
    assert.equal(root.getAttribute("data-tauri-decoration-fullscreen"), "");
    assert.equal(
      style.getPropertyValue("--tauri-plugin-decoration-right-clearance"),
      "0px",
    );

    assert.equal(runtime.dispatch("7", "32", "fullscreen-did-exit", null), true);
    assert.equal(root.getAttribute("data-tauri-decoration-fullscreen"), null);
    assert.equal(
      style.getPropertyValue("--tauri-plugin-decoration-right-clearance"),
      "174px",
    );
  });

  it("derives Windows fullscreen state even when no Snap overlay is installed", async () => {
    const isFullscreen = spy(async () => true);
    const harness = createHarness({ isFullscreen });
    harness.loadPlatform("windows");
    const options = config(33);
    options.controls = ["minimize", "close"];
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(options),
    );

    const root = harness.window.document.querySelector(
      "[data-tauri-plugin-decoration-root]",
    );
    const style = harness.window.document.documentElement.style;
    assert.equal(root.getAttribute("data-tauri-decoration-fullscreen"), "");
    assert.equal(
      style.getPropertyValue("--tauri-plugin-decoration-right-clearance"),
      "0px",
    );

    isFullscreen.implementation = async () => false;
    harness.window.dispatchEvent(new harness.window.Event("resize"));
    await waitForCalls(isFullscreen, 2);
    await waitForCondition(
      () => root.getAttribute("data-tauri-decoration-fullscreen") === null,
      "Windows fullscreen marker was not cleared",
    );
    assert.equal(
      style.getPropertyValue("--tauri-plugin-decoration-right-clearance"),
      "116px",
    );
  });

  it("converts Win32 physical hit-test coordinates to CSS pixels", async () => {
    const harness = createHarness();
    harness.window.devicePixelRatio = 2;
    harness.loadPlatform("windows");
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(31)),
    );

    const maximize = harness.window.document.querySelector(
      '[data-tauri-decoration-control="maximize"]',
    );
    const elementFromPoint = spy(() => maximize);
    harness.window.document.elementFromPoint = elementFromPoint;

    assert.equal(
      harness.window.__TAURI_PLUGIN_DECORATION__.dispatch(
        "7",
        "31",
        "snap-mousemove",
        [116, 32],
      ),
      true,
    );
    assert.deepEqual(elementFromPoint.calls, [[58, 16]]);
  });

  it("disposes the previous resize handler during same-document reinjection", async () => {
    const harness = createHarness();
    harness.loadPlatform("windows");
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(40)),
    );
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(41)),
    );

    harness.windowApi.isMaximized.mockClear();
    harness.window.dispatchEvent(new harness.window.Event("resize"));
    await Promise.resolve();
    await Promise.resolve();
    assert.equal(harness.windowApi.isMaximized.calls.length, 1);
  });

  it("keeps two webview documents fully isolated", async () => {
    const first = createHarness();
    const second = createHarness();
    first.loadPlatform("windows");
    second.loadPlatform("windows");

    await Promise.all([
      loadStylesheet(
        first,
        first.window.__TAURI_PLUGIN_DECORATION__.install(config(50)),
      ),
      loadStylesheet(
        second,
        second.window.__TAURI_PLUGIN_DECORATION__.install(config(50)),
      ),
    ]);
    first.window.__TAURI_PLUGIN_DECORATION__.dispatch(
      "7",
      "50",
      "snap-click",
      null,
    );
    await Promise.resolve();

    assert.equal(first.windowApi.toggleMaximize.calls.length, 1);
    assert.equal(second.windowApi.toggleMaximize.calls.length, 0);
    assert.notEqual(first.window.document, second.window.document);
  });

  it("applies macOS traffic-light clearance only for the exact active document", async () => {
    const harness = createHarness();
    const installation = harness.window.__TAURI_PLUGIN_DECORATION__.install(
      config(55, "macos"),
    );
    await loadStylesheet(harness, installation);

    const runtime = harness.window.__TAURI_PLUGIN_DECORATION__;
    const style = harness.window.document.documentElement.style;
    const normalState = { fullscreen: false, clearance: 72 };
    assert.equal(
      runtime.dispatch("7", "54", "macos-titlebar-state", normalState),
      false,
    );
    assert.equal(style.getPropertyValue("--decoration-traffic-light-left"), "");
    for (const invalid of [
      null,
      { fullscreen: "false", clearance: 72 },
      { fullscreen: false, clearance: -1 },
      { fullscreen: false, clearance: Number.NaN },
      { fullscreen: false, clearance: Number.POSITIVE_INFINITY },
      { fullscreen: false, clearance: "72" },
    ]) {
      assert.equal(
        runtime.dispatch("7", "55", "macos-titlebar-state", invalid),
        false,
      );
    }
    assert.equal(
      runtime.dispatch("7", "55", "macos-titlebar-state", normalState),
      true,
    );
    assert.equal(
      style.getPropertyValue("--decoration-traffic-light-left"),
      "72px",
    );
    assert.equal(
      style.getPropertyValue("--tauri-plugin-decoration-left-clearance"),
      "72px",
    );
  });

  it("collapses macOS titlebar clearance while the native window is fullscreen", async () => {
    const harness = createHarness();
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(56, "macos")),
    );

    const runtime = harness.window.__TAURI_PLUGIN_DECORATION__;
    const root = harness.window.document.querySelector(
      "[data-tauri-plugin-decoration-root]",
    );
    const style = harness.window.document.documentElement.style;
    assert.equal(
      runtime.dispatch("7", "56", "macos-titlebar-state", {
        fullscreen: false,
        clearance: 72,
      }),
      true,
    );
    assert.equal(
      runtime.dispatch("7", "56", "macos-titlebar-state", {
        fullscreen: true,
        clearance: 0,
      }),
      true,
    );
    assert.equal(root.getAttribute("data-tauri-decoration-fullscreen"), "");
    assert.equal(
      style.getPropertyValue("--tauri-plugin-decoration-left-clearance"),
      "0px",
    );
    assert.equal(
      style.getPropertyValue("--decoration-traffic-light-left"),
      "0px",
    );

    assert.equal(
      runtime.dispatch("7", "56", "macos-titlebar-state", {
        fullscreen: false,
        clearance: 72,
      }),
      true,
    );
    assert.equal(root.hasAttribute("data-tauri-decoration-fullscreen"), false);
    assert.equal(
      style.getPropertyValue("--tauri-plugin-decoration-left-clearance"),
      "72px",
    );
  });

  it("renders atomic native macOS titlebar state across fullscreen exit", async () => {
    const isFullscreen = spy(async () => true);
    const harness = createHarness({ isFullscreen });
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(57, "macos")),
    );

    const runtime = harness.window.__TAURI_PLUGIN_DECORATION__;
    const root = harness.window.document.querySelector(
      "[data-tauri-plugin-decoration-root]",
    );
    const style = harness.window.document.documentElement.style;
    assert.equal(isFullscreen.calls.length, 0);
    assert.equal(
      runtime.dispatch("7", "57", "macos-titlebar-state", {
        fullscreen: true,
        clearance: 0,
      }),
      true,
    );
    assert.equal(root.getAttribute("data-tauri-decoration-fullscreen"), "");
    assert.equal(
      style.getPropertyValue("--tauri-plugin-decoration-left-clearance"),
      "0px",
    );

    assert.equal(
      runtime.dispatch("7", "57", "macos-titlebar-state", {
        fullscreen: false,
        clearance: 72,
      }),
      true,
    );
    assert.equal(root.getAttribute("data-tauri-decoration-fullscreen"), null);
    assert.equal(
      style.getPropertyValue("--tauri-plugin-decoration-left-clearance"),
      "72px",
    );
    assert.equal(
      style.getPropertyValue("--decoration-traffic-light-left"),
      "72px",
    );
  });

  it("accepts macOS titlebar state only for the current reinjection target", async () => {
    const harness = createHarness();
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(57, "macos")),
    );
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(58, "macos")),
    );

    const runtime = harness.window.__TAURI_PLUGIN_DECORATION__;
    assert.equal(
      runtime.dispatch("7", "57", "macos-titlebar-state", {
        fullscreen: false,
        clearance: 72,
      }),
      false,
    );
    assert.equal(
      runtime.dispatch("7", "58", "macos-titlebar-state", {
        fullscreen: false,
        clearance: 72,
      }),
      true,
    );
  });

  it("does not query asynchronous JavaScript fullscreen state on macOS resize", async () => {
    const isFullscreen = spy(async () => false);
    const harness = createHarness({ isFullscreen });
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(59, "macos")),
    );

    isFullscreen.mockClear();
    harness.window.dispatchEvent(new harness.window.Event("resize"));
    harness.window.dispatchEvent(new harness.window.Event("resize"));
    harness.window.dispatchEvent(new harness.window.Event("resize"));
    await Promise.resolve();
    assert.equal(isFullscreen.calls.length, 0);
  });

  it("keeps the previous macOS titlebar state when a malformed event arrives", async () => {
    const harness = createHarness();
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(60, "macos")),
    );

    const runtime = harness.window.__TAURI_PLUGIN_DECORATION__;
    assert.equal(
      runtime.dispatch("7", "60", "macos-titlebar-state", {
        fullscreen: false,
        clearance: 72,
      }),
      true,
    );
    assert.equal(
      runtime.dispatch("7", "60", "macos-titlebar-state", {
        fullscreen: true,
        clearance: -1,
      }),
      false,
    );
    assert.equal(
      harness.window.document.documentElement.style.getPropertyValue(
        "--tauri-plugin-decoration-left-clearance",
      ),
      "72px",
    );
  });

  it("gives the plugin drag plane a non-editing cursor", async () => {
    const harness = createHarness();
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(59, "macos")),
    );
    const dragRegion = harness.window.document.querySelector(
      "[data-tauri-drag-region]",
    );

    assert.equal(dragRegion.getAttribute("data-tauri-drag-region"), "");
    assert.match(
      stylesheet,
      /\[data-tauri-plugin-decoration-root\]\s+\[data-tauri-decoration-tb\]\s*>\s*\[data-tauri-drag-region\]\s*\{[^}]*pointer-events:\s*auto;[^}]*cursor:\s*default;/s,
    );
  });

  it("scopes drag-plane geometry to plugin-owned elements", () => {
    assert.doesNotMatch(
      stylesheet,
      /(?:^|,)\s*\[data-tauri-drag-region\]\s*(?:,|\{)/m,
    );
    const internalSelector =
      /\[data-tauri-decoration-(?:tb|controls|side|platform|control|icon)/;
    for (const [, selectorList] of stylesheet.matchAll(/([^{}]+)\{/g)) {
      for (const selector of selectorList.split(",")) {
        if (internalSelector.test(selector)) {
          assert.match(selector, /\[data-tauri-plugin-decoration-root\]/);
        }
      }
    }
    assert.match(
      stylesheet,
      /\[data-tauri-plugin-decoration-root\]\s+\[data-tauri-decoration-tb\]\s*>\s*\[data-tauri-drag-region\]/,
    );
  });

  it("layers the visible example titlebar around the plugin controls", () => {
    assert.match(
      exampleAppSource,
      /<header\s+className="titlebar-content"\s+data-tauri-drag-region=""\s*>/s,
    );
    assert.match(
      exampleAppSource,
      /<div\s+className="titlebar-surface"\s+aria-hidden="true"\s*\/>/s,
    );

    const pluginLayer = Number(
      stylesheet.match(
        /\[data-tauri-plugin-decoration-root\]\s*\{[^}]*z-index:\s*(\d+);/s,
      )?.[1],
    );
    const surfaceRule = exampleStylesheet.match(
      /\.titlebar-surface\s*\{([^}]*)\}/s,
    )?.[1];
    const titlebarRule = exampleStylesheet.match(
      /\.titlebar-content\s*\{([^}]*)\}/s,
    )?.[1];
    assert.ok(surfaceRule, "missing example titlebar surface rule");
    assert.ok(titlebarRule, "missing example titlebar rule");
    const surfaceLayer = Number(surfaceRule.match(/z-index:\s*(\d+);/)?.[1]);
    const titlebarLayer = Number(titlebarRule.match(/z-index:\s*(\d+);/)?.[1]);

    assert.match(surfaceRule, /background:/);
    assert.match(surfaceRule, /border-bottom:/);
    assert.ok(
      surfaceLayer < pluginLayer && pluginLayer < titlebarLayer,
      "the plugin controls must sit between the surface and interactive content",
    );
    assert.match(
      titlebarRule,
      /pointer-events:\s*none;[^}]*cursor:\s*default;[^}]*user-select:\s*none;/s,
    );
    assert.doesNotMatch(exampleAppSource, /<nav|Disclosure|Test windows/);
  });

  it("provides an accessible custom Window dropdown with real actions", () => {
    for (const required of [
      'aria-haspopup="menu"',
      'role="menu"',
      'role="menuitem"',
      "currentWindow.minimize()",
      "currentWindow.toggleMaximize()",
      "currentWindow.close()",
    ]) {
      assert.ok(exampleAppSource.includes(required), `missing dropdown behavior: ${required}`);
    }
    assert.match(
      exampleStylesheet,
      /\.window-menu\s*\{[^}]*pointer-events:\s*auto;/s,
    );
    assert.match(exampleAppSource, /event\.key === "Escape"/);
    assert.match(exampleAppSource, /event\.key === "ArrowDown"/);
  });

  it("uses plugin clearances without application fullscreen bookkeeping", () => {
    assert.match(
      exampleStylesheet,
      /padding-left:\s*max\(\s*0px,\s*var\(--tauri-plugin-decoration-left-clearance/s,
    );
    assert.doesNotMatch(exampleAppSource, /nativeFullscreen|isFullscreen/);
  });

  it("presents one copyable activation and native-fallback flow", () => {
    for (const required of [
      "activate_custom_titlebar",
      "show_native_fallback",
      "data-tauri-plugin-decoration-active",
      "DECORATION_FALLBACK_MS",
      "MutationObserver",
      "Custom titlebar active",
      "Native titlebar fallback",
    ]) {
      assert.ok(exampleAppSource.includes(required), `missing example flow: ${required}`);
    }
    assert.match(
      exampleAppSource,
      /sessionStorage\.setItem\(modeStorageKey, "custom"\)[\s\S]*invoke\("activate_custom_titlebar"\)/,
    );
    assert.doesNotMatch(exampleAppSource, /data-tauri-plugin-decoration-status/);
  });

  it("publishes both Linux clearances for left- and right-side GTK layouts", async () => {
    const harness = createHarness();
    harness.loadPlatform("linux");
    const options = config(56, "linux");
    options.layout = {
      left: ["close", "minimize"],
      right: ["maximize"],
    };
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(options),
    );

    const style = harness.window.document.documentElement.style;
    assert.equal(
      style.getPropertyValue("--tauri-plugin-decoration-left-clearance"),
      "86px",
    );
    assert.equal(
      style.getPropertyValue("--tauri-plugin-decoration-right-clearance"),
      "48px",
    );
  });

  it("uses inert Linux images with explicit names and button semantics", async () => {
    const harness = createHarness();
    harness.loadPlatform("linux");
    const installation = harness.window.__TAURI_PLUGIN_DECORATION__.install(
      config(60, "linux"),
    );
    await loadStylesheet(harness, installation);

    const buttons = harness.window.document.querySelectorAll("button");
    assert.deepEqual(
      buttons.map((button) => button.getAttribute("aria-label")),
      ["Minimize window", "Maximize window size", "Close window"],
    );
    for (const button of buttons) {
      assert.equal(button.getAttribute("type"), "button");
      const image = button.querySelector("img");
      assert.equal(image.getAttribute("alt"), "");
      assert.equal(image.getAttribute("aria-hidden"), "true");
      assert.match(image.src, /^data:image\/png;base64,/);
    }
  });

  it("falls back per Linux control when a theme image cannot be decoded", async () => {
    const harness = createHarness();
    harness.loadPlatform("linux");
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(60, "linux")),
    );

    const close = harness.window.document.querySelector(
      '[data-tauri-decoration-control="close"]',
    );
    const image = close.querySelector("img");
    image.dispatchEvent(new harness.window.Event("error"));

    assert.equal(image.getAttribute("src"), null);
    assert.equal(
      close.getAttribute("data-tauri-decoration-icon-fallback"),
      "",
    );
  });

  it("delegates Linux edge resizing to Tauri's undecorated runtime", async () => {
    const harness = createHarness();
    harness.loadPlatform("linux");
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(61, "linux")),
    );

    const handles = harness.window.document.querySelectorAll(
      "[data-tauri-decoration-resize]",
    );
    assert.equal(handles.length, 0);
    assert.doesNotMatch(linuxSource, /startResizeDragging/);
    assert.doesNotMatch(stylesheet, /data-tauri-decoration-resize/);
  });

  it("reports Linux action failures and allows a later retry", async () => {
    const harness = createHarness();
    harness.windowApi.minimize.implementation = async () => {
      throw new Error("denied");
    };
    harness.loadPlatform("linux");
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(62, "linux")),
    );

    const minimize = harness.window.document.querySelector(
      '[data-tauri-decoration-control="minimize"]',
    );
    const close = harness.window.document.querySelector(
      '[data-tauri-decoration-control="close"]',
    );
    minimize.dispatchEvent(new harness.window.Event("click"));
    await waitForCalls(harness.consoleError, 1);
    assert.equal(minimize.getAttribute("disabled"), null);
    assert.equal(
      harness.invoke.calls.some(
        ([command]) => command === "plugin:decoration|frontend_fallback",
      ),
      false,
    );
    assert.match(
      harness.consoleError.calls[0][0],
      /Linux minimize action failed/,
    );
    assert.ok(
      harness.window.document.querySelector(
        "[data-tauri-plugin-decoration-root]",
      ),
    );
    harness.windowApi.minimize.implementation = async () => {};
    minimize.dispatchEvent(new harness.window.Event("click"));
    await waitForCalls(harness.windowApi.minimize, 2);
    close.dispatchEvent(new harness.window.Event("click"));
    await waitForCalls(harness.windowApi.close, 1);
  });

  it("hides Linux controls and clearances throughout fullscreen", async () => {
    const isFullscreen = spy(async () => true);
    const harness = createHarness({ isFullscreen });
    harness.loadPlatform("linux");
    const options = config(63, "linux");
    options.layout = { left: ["close"], right: ["minimize", "maximize"] };
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(options),
    );

    const root = harness.window.document.querySelector(
      "[data-tauri-plugin-decoration-root]",
    );
    const style = harness.window.document.documentElement.style;
    assert.equal(root.getAttribute("data-tauri-decoration-fullscreen"), "");
    assert.equal(
      style.getPropertyValue("--tauri-plugin-decoration-left-clearance"),
      "0px",
    );
    assert.equal(
      style.getPropertyValue("--tauri-plugin-decoration-right-clearance"),
      "0px",
    );

    isFullscreen.implementation = async () => false;
    harness.window.dispatchEvent(new harness.window.Event("resize"));
    await waitForCalls(isFullscreen, 2);
    await waitForCondition(
      () => root.getAttribute("data-tauri-decoration-fullscreen") === null,
      "fullscreen marker was not cleared",
    );
    assert.equal(root.getAttribute("data-tauri-decoration-fullscreen"), null);
    assert.equal(
      style.getPropertyValue("--tauri-plugin-decoration-left-clearance"),
      "48px",
    );
    assert.equal(
      style.getPropertyValue("--tauri-plugin-decoration-right-clearance"),
      "86px",
    );
  });

  it("keeps GTK controls as an activation snapshot across focus changes", async () => {
    const harness = createHarness();
    harness.loadPlatform("linux");
    await loadStylesheet(
      harness,
      harness.window.__TAURI_PLUGIN_DECORATION__.install(config(64, "linux")),
    );

    const original = harness.window.document.querySelector(
      '[data-tauri-decoration-control="close"]',
    );
    harness.window.dispatchEvent(new harness.window.Event("focus"));
    harness.window.dispatchEvent(new harness.window.Event("blur"));
    await Promise.resolve();
    await Promise.resolve();

    assert.equal(
      harness.invoke.calls.some(
        ([command]) => command === "plugin:decoration|linux_options",
      ),
      false,
    );
    assert.equal(
      harness.window.document.querySelector(
        '[data-tauri-decoration-control="close"]',
      ),
      original,
    );
  });

  it("contains no executable Linux markup and retains keyboard focus styles", () => {
    assert.doesNotMatch(linuxSource, /innerHTML/);
    assert.doesNotMatch(linuxSource, /@win-/);
    assert.match(stylesheet, /:focus-visible/);
    assert.match(stylesheet, /@media \(forced-colors: active\)/);
    assert.match(stylesheet, /--tauri-plugin-decoration-ready: ready/);
    assert.match(
      stylesheet,
      /background:\s*var\(\s*--decoration-tb-actions-icon-bg,\s*rgba\(0, 0, 0, 0\.18\)/,
    );
    assert.match(stylesheet, /@supports \(color: color-mix\(/);
    assert.match(
      stylesheet,
      /@media \(prefers-color-scheme: dark\)[\s\S]*\[data-tauri-plugin-decoration-root\][\s\S]*\[data-tauri-decoration-platform="linux"\]\s+\[data-tauri-decoration-control\][\s\S]*rgba\(255, 255, 255, 0\.18\)/,
    );
    assert.match(
      stylesheet,
      /@media \(forced-colors: active\)[\s\S]*\[data-tauri-plugin-decoration-root\][\s\S]*\[data-tauri-decoration-platform="linux"\]\s+\[data-tauri-decoration-control\]/,
    );
    assert.match(
      stylesheet,
      /@media \(forced-colors: active\)[\s\S]*\[data-tauri-decoration-platform="linux"\][\s\S]*img[\s\S]*display:\s*none/,
    );
  });
});
