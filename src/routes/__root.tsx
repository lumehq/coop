import { cn } from "@/commons";
import type { QueryClient } from "@tanstack/react-query";
import { Outlet, createRootRouteWithContext } from "@tanstack/react-router";
import type { OsType } from "@tauri-apps/plugin-os";

interface RouterContext {
	queryClient: QueryClient;
	platform: OsType;
}

export const Route = createRootRouteWithContext<RouterContext>()({
	component: RootComponent,
});

function RootComponent() {
	const { platform } = Route.useRouteContext();

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
