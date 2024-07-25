import { createLazyFileRoute } from "@tanstack/react-router";
import { CoopIcon } from "@/icons/coop";

export const Route = createLazyFileRoute("/$account/chats/new")({
	component: Screen,
});

function Screen() {
	return (
		<div className="size-full flex items-center justify-center">
			<CoopIcon className="size-10 text-neutral-200 dark:text-neutral-800" />
		</div>
	);
}
