import { useState, useRef, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface MenuItem {
	label: string;
	action: () => void;
}

function DropdownMenu({ label, items }: { label: string; items: MenuItem[] }) {
	const [open, setOpen] = useState(false);
	const ref = useRef<HTMLDivElement>(null);

	useEffect(() => {
		const handler = (e: MouseEvent) => {
			if (ref.current && !ref.current.contains(e.target as Node)) {
				setOpen(false);
			}
		};
		document.addEventListener("mousedown", handler);
		return () => document.removeEventListener("mousedown", handler);
	}, []);

	return (
		<div
			className="dropdown-menu"
			ref={ref}
			onClick={(e) => {
				e.stopPropagation();
				setOpen(!open);
			}}
		>
			<span className="dropdown-label">{label}</span>
			{open && (
				<ul className="dropdown-list">
					{items.map((item, i) => (
						<li
							key={i}
							className="dropdown-item"
							onClick={(e) => {
								e.stopPropagation();
								item.action();
								setOpen(false);
							}}
						>
							{item.label}
						</li>
					))}
				</ul>
			)}
		</div>
	);
}

function App() {
	const win = getCurrentWindow();

	const fileItems: MenuItem[] = [
		{ label: "New Window", action: () => console.log("New Window") },
		{ label: "Open File...", action: () => console.log("Open File") },
		{ label: "Save", action: () => console.log("Save") },
		{ label: "Exit", action: () => win?.close() },
	];

	const viewItems: MenuItem[] = [
		{ label: "Toggle Maximize", action: () => win?.toggleMaximize() },
		{ label: "Minimize", action: () => win?.minimize() },
		{ label: "Center", action: () => win?.center() },
	];

	const helpItems: MenuItem[] = [
		{ label: "About", action: () => console.log("About") },
		{ label: "Documentation", action: () => console.log("Docs") },
	];

	return (
		<div className="app">
			<div className="titlebar-content">
				<div className="menu-bar">
					<DropdownMenu label="File" items={fileItems} />
					<DropdownMenu label="View" items={viewItems} />
					<DropdownMenu label="Help" items={helpItems} />
				</div>
				<span className="app-title">
					Decoration Demo
				</span>
			</div>

			<div className="app-body">
				<h1>Decoration Plugin Demo</h1>
				<p>
					This window uses a frameless decoration with custom window controls
					and a menu bar in the titlebar area.
				</p>
				<p className="hint">
					On Windows 11, hover the maximize button to trigger the native
					Snap Layout flyout.
				</p>
			</div>
		</div>
	);
}

export default App;
