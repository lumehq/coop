import { createFileRoute } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";

export const Route = createFileRoute("/$account/chats/$id")({
	beforeLoad: async ({ params }) => {
		const inbox: string[] = await invoke("connect_inbox", { id: params.id });
		return { inbox };
	},
});
