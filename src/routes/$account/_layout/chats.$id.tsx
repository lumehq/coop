import { commands } from "@/commands";
import { Spinner } from "@/components/spinner";
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/$account/_layout/chats/$id")({
	loader: async ({ params }) => {
		const res = await commands.connectInboxRelays(params.id, false);

		if (res.status === "ok") {
			return res.data;
		} else {
			return [];
		}
	},
	pendingComponent: Pending,
	pendingMs: 200,
	pendingMinMs: 100,
});

function Pending() {
	return (
		<div className="size-full flex items-center justify-center">
			<div className="flex flex-col gap-2 items-center justify-center">
				<Spinner />
				<span className="text-xs text-center text-neutral-600 dark:text-neutral-400">
					Connection in progress. Please wait ...
				</span>
			</div>
		</div>
	);
}
