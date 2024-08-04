import { createFileRoute } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";

export const Route = createFileRoute("/$account/chats/$id")({
	loader: async ({ params }) => {
		const inboxRelays: string[] = await invoke("connect_inbox", {
			id: params.id,
		});
		return inboxRelays;
	},
});
