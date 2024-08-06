import { commands } from "@/commands";
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/$account/relays")({
	loader: async ({ params }) => {
		const res = await commands.getInboxRelays(params.account);

		if (res.status === "ok") {
			return res.data;
		} else {
			throw new Error(res.error);
		}
	},
});
