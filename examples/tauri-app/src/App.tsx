import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";

const DECORATION_FALLBACK_MS = 5000;
const currentWindow = getCurrentWindow();
const modeStorageKey = `decoration-example:mode:${currentWindow.label}`;
const initialMode = window.sessionStorage.getItem(modeStorageKey);
let activationRequest: Promise<unknown> | null = null;

const requestCustomTitlebar = () => {
	window.sessionStorage.setItem(modeStorageKey, "custom");
	activationRequest ??= invoke("activate_custom_titlebar");
	return activationRequest;
};

function App() {
	const [nativeTitlebar, setNativeTitlebar] = useState(
		initialMode === "native",
	);
	const [status, setStatus] = useState(
		initialMode === "native"
			? "Native titlebar fallback"
			: "Activating custom titlebar…",
	);

	useEffect(() => {
		if (initialMode === "native") return;

		let disposed = false;
		let settled = false;
		let fallbackStarted = false;
		let timeoutId: number | undefined;

		const clearTimer = () => {
			if (timeoutId !== undefined) window.clearTimeout(timeoutId);
			timeoutId = undefined;
		};

		const fallbackToNative = async (reason: string) => {
			if (disposed || settled || fallbackStarted) return;
			fallbackStarted = true;
			clearTimer();
			setStatus("Restoring native titlebar…");
			try {
				await invoke("show_native_fallback");
				if (disposed) return;
				settled = true;
				window.sessionStorage.setItem(modeStorageKey, "native");
				setNativeTitlebar(true);
				setStatus("Native titlebar fallback");
			} catch (error) {
				if (!disposed) {
					setStatus(`${reason}; native fallback failed: ${String(error)}`);
				}
			}
		};

		const revealActive = async () => {
			if (disposed || settled || fallbackStarted) return;
			settled = true;
			clearTimer();
			try {
				await currentWindow.show();
				if (disposed) return;
				window.sessionStorage.setItem(modeStorageKey, "custom");
				setStatus("Custom titlebar active");
			} catch (error) {
				settled = false;
				await fallbackToNative(`show failed: ${String(error)}`);
			}
		};

		const observeActivation = () => {
			if (
				document.querySelector("[data-tauri-plugin-decoration-active]")
			) {
				void revealActive();
			}
		};
		const observer = new MutationObserver(observeActivation);
		observer.observe(document.documentElement, {
			attributes: true,
			childList: true,
			subtree: true,
		});

		timeoutId = window.setTimeout(() => {
			void fallbackToNative("custom titlebar activation timed out");
		}, DECORATION_FALLBACK_MS);
		observeActivation();

		if (initialMode !== "custom") {
			void requestCustomTitlebar().catch((error) => {
				void fallbackToNative(`activation request failed: ${String(error)}`);
			});
		}

		return () => {
			disposed = true;
			clearTimer();
			observer.disconnect();
		};
	}, []);

	return (
		<div className="app">
			{!nativeTitlebar && (
				<header className="titlebar-content" data-tauri-drag-region="">
					<span className="app-title">Decoration example</span>
				</header>
			)}

			<main className="app-body">
				<section className="intro" aria-labelledby="demo-title">
					<p className="eyebrow">tauri-plugin-decoration</p>
					<h1 id="demo-title">One window. Native controls.</h1>
					<p>
						This example activates the custom titlebar after the document mounts,
						then reveals the hidden window only after native activation succeeds.
					</p>
					<p>
						If activation times out or fails, Rust restores the native titlebar
						before showing the window.
					</p>
					<p className="status" role="status">
						{status}
					</p>
				</section>
			</main>
		</div>
	);
}

export default App;
