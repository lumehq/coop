import { commands } from "@/commands";
import { checkForAppUpdates } from "@/commons";
import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/")({
	beforeLoad: async () => {
		// Check for app updates
		// TODO: move this function to rust
		await checkForAppUpdates(true);

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
