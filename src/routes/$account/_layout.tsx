import { commands } from "@/commands";
import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/$account/_layout")({
	beforeLoad: async ({ params }) => {
		const res = await commands.ensureInboxRelays(params.account);

		if (res.status === "error") {
			throw redirect({
				to: "/inbox-relays",
				search: { account: params.account, redirect: window.location.href },
				replace: true,
			});
		}
	},
});
