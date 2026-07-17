import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useId, useRef, useState } from "react";

const DECORATION_FALLBACK_MS = 5000;
const currentWindow = getCurrentWindow();
const modeStorageKey = `decoration-example:mode:${currentWindow.label}`;
const initialMode = window.sessionStorage.getItem(modeStorageKey);
let activationRequest: Promise<unknown> | null = null;

interface WindowAction {
	label: string;
	run: () => Promise<void>;
}

const windowActions: readonly WindowAction[] = [
	{ label: "Minimize", run: () => currentWindow.minimize() },
	{ label: "Toggle maximize", run: () => currentWindow.toggleMaximize() },
	{ label: "Close", run: () => currentWindow.close() },
];

const requestCustomTitlebar = () => {
	window.sessionStorage.setItem(modeStorageKey, "custom");
	activationRequest ??= invoke("activate_custom_titlebar");
	return activationRequest;
};

function WindowMenu({
	onActionError,
}: {
	onActionError: (message: string) => void;
}) {
	const [open, setOpen] = useState(false);
	const menuId = useId();
	const triggerId = `${menuId}-trigger`;
	const rootRef = useRef<HTMLDivElement>(null);
	const triggerRef = useRef<HTMLButtonElement>(null);
	const firstItemRef = useRef<HTMLButtonElement>(null);

	useEffect(() => {
		if (!open) return;

		const dismissOutside = (event: PointerEvent) => {
			if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
		};
		document.addEventListener("pointerdown", dismissOutside);
		firstItemRef.current?.focus();

		return () => document.removeEventListener("pointerdown", dismissOutside);
	}, [open]);

	const closeAndFocusTrigger = () => {
		setOpen(false);
		triggerRef.current?.focus();
	};

	const runAction = async (item: WindowAction) => {
		setOpen(false);
		try {
			await item.run();
		} catch (error) {
			onActionError(`${item.label} failed: ${String(error)}`);
		}
	};

	return (
		<div
			className="window-menu"
			ref={rootRef}
			onBlur={(event) => {
				if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
					setOpen(false);
				}
			}}
		>
			<button
				className="window-menu-trigger"
				type="button"
				ref={triggerRef}
				id={triggerId}
				aria-controls={open ? menuId : undefined}
				aria-expanded={open}
				aria-haspopup="menu"
				onClick={() => setOpen((current) => !current)}
				onKeyDown={(event) => {
					if (event.key === "ArrowDown") {
						event.preventDefault();
						setOpen(true);
					}
				}}
			>
				Window <span aria-hidden="true">▾</span>
			</button>

			{open && (
				<ul
					className="window-menu-list"
					id={menuId}
					role="menu"
					aria-labelledby={triggerId}
					onKeyDown={(event) => {
						if (event.key === "Escape") {
							event.preventDefault();
							closeAndFocusTrigger();
							return;
						}
						if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;

						event.preventDefault();
						const items = Array.from(
							event.currentTarget.querySelectorAll<HTMLButtonElement>(
								'[role="menuitem"]',
							),
						);
						const currentIndex = items.indexOf(document.activeElement as HTMLButtonElement);
						const step = event.key === "ArrowDown" ? 1 : -1;
						const nextIndex = (currentIndex + step + items.length) % items.length;
						items[nextIndex]?.focus();
					}}
				>
					{windowActions.map((item, index) => (
						<li key={item.label} role="none">
							<button
								className="window-menu-item"
								type="button"
								ref={index === 0 ? firstItemRef : undefined}
								role="menuitem"
								onClick={() => void runAction(item)}
							>
								{item.label}
							</button>
						</li>
					))}
				</ul>
			)}
		</div>
	);
}

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
		<div
			className="app"
			data-titlebar-mode={nativeTitlebar ? "native" : "custom"}
		>
			{!nativeTitlebar && (
				<>
					<div className="titlebar-surface" aria-hidden="true" />
					<header className="titlebar-content" data-tauri-drag-region="">
						<WindowMenu onActionError={setStatus} />
						<span className="app-title">Decoration example</span>
					</header>
				</>
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
