import { Link, createLazyFileRoute } from "@tanstack/react-router";

export const Route = createLazyFileRoute("/new")({
	component: Screen,
});

function Screen() {
	return (
		<div
			data-tauri-drag-region
			className="size-full flex items-center justify-center"
		>
			<div className="w-[320px] flex flex-col gap-8">
				<div className="flex flex-col gap-1 text-center">
					<h1 className="leading-tight text-xl font-semibold">
						Direct Message on Nostr.
					</h1>
				</div>
				<div className="flex flex-col gap-3">
					<Link
						to="/create-account"
						className="w-full h-10 bg-blue-500 hover:bg-blue-600 text-white rounded-lg inline-flex items-center justify-center shadow"
					>
						Create a new identity
					</Link>
					<Link
						to="/import-key"
						className="w-full h-10 bg-white hover:bg-neutral-100 dark:hover:bg-neutral-100 dark:bg-white dark:text-black rounded-lg inline-flex items-center justify-center"
					>
						Login with Private Key
					</Link>
					{/*
					<Link
						to="/import-key"
						className="w-full text-sm text-neutral-600 dark:text-neutral-400 inline-flex items-center justify-center"
					>
						Login with Private Key (not recommended)
					</Link>
					*/}
				</div>
			</div>
		</div>
	);
}
