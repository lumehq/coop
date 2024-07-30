import { commands } from "@/commands";
import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/")({
	beforeLoad: async () => {
		const accounts = await commands.getAccounts();

		if (!accounts.length) {
			throw redirect({
				to: "/new",
				replace: true,
			});
		}

		return { accounts };
	},
});
