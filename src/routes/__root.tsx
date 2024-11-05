import { cn } from "@/commons";
import type { Metadata } from "@/hooks/useProfile";
import type { QueryClient } from "@tanstack/react-query";
import { Outlet, createRootRouteWithContext } from "@tanstack/react-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { OsType } from "@tauri-apps/plugin-os";
import type { NostrEvent } from "nostr-tools";
import { useEffect } from "react";

interface RouterContext {
	queryClient: QueryClient;
	platform: OsType;
}

export const Route = createRootRouteWithContext<RouterContext>()({
	component: RootComponent,
});

function RootComponent() {
	const { platform, queryClient } = Route.useRouteContext();

	useEffect(() => {
		const unlisten = getCurrentWindow().listen<string>(
			"metadata",
			async (data) => {
				const event: NostrEvent = JSON.parse(data.payload);
				const metadata: Metadata = JSON.parse(event.content);

				// Update query cache
				queryClient.setQueryData(["profile", event.pubkey], () => metadata);

				// Reset query cache
				await queryClient.invalidateQueries({
					queryKey: ["profile", event.pubkey],
				});
			},
		);

		return () => {
			unlisten.then((f) => f());
		};
	}, []);

	return (
		<div
			className={cn(
				"size-full",
				platform === "linux"
					? "bg-neutral-50 dark:bg-neutral-950"
					: "bg-transparent",
			)}
		>
			<Outlet />
		</div>
	);
}
