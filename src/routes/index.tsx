import { commands } from "@/commands";
import { checkForAppUpdates } from "@/commons";
import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/")({
	beforeLoad: async () => {
		// Check for app updates
		await checkForAppUpdates(true);

		// Get all accounts from system
		const accounts = await commands.getAccounts();

		if (!accounts.length) {
			throw redirect({
				to: "/new",
				replace: true,
			});
		}

		// Workaround for keyring bug on Windows
		const fil = accounts.filter((item) => !item.includes("Coop"));

		return { accounts: fil };
	},
});
