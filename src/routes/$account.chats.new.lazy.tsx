import { CoopIcon } from "@/icons/coop";
import { createLazyFileRoute } from "@tanstack/react-router";

export const Route = createLazyFileRoute("/$account/chats/new")({
	component: Screen,
});

function Screen() {
	return (
		<div
			data-tauri-drag-region
			className="size-full flex flex-col gap-3 items-center justify-center"
		>
			<CoopIcon className="size-10 text-neutral-200 dark:text-neutral-800" />
			<h1 className="text-center font-bold text-neutral-300 dark:text-neutral-700">
				coop on nostr.
			</h1>
		</div>
	);
}
