import { commands } from "@/commands";
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/$account/chats/$id")({
	loader: async ({ params }) => {
		const res = await commands.connectInboxRelays(params.id, false);

		if (res.status === "ok") {
			return res.data;
		} else {
			return [];
		}
	},
});
