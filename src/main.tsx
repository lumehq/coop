import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, createRouter } from "@tanstack/react-router";
import { type } from "@tauri-apps/plugin-os";
import { LRUCache } from "lru-cache";
import { type ReactNode, StrictMode } from "react";
import ReactDOM from "react-dom/client";
import "./global.css";
// Import the generated commands
import { commands } from "./commands";
// Import the generated route tree
import { routeTree } from "./routes.gen";

const platform = type();
const queryClient = new QueryClient();
const chatManager = new LRUCache<string, string>({
	max: 3,
	dispose: async (v, _) => await commands.disconnectInboxRelays(v),
});

const router = createRouter({
	routeTree,
	context: {
		queryClient,
		chatManager,
		platform,
	},
	Wrap: ({ children }: { children: ReactNode }) => {
		return (
			<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
		);
	},
});

// Register the router instance for type safety
declare module "@tanstack/react-router" {
	interface Register {
		router: typeof router;
	}
}

// Render the app
const rootElement = document.getElementById("root") as HTMLElement;
if (!rootElement.innerHTML) {
	const root = ReactDOM.createRoot(rootElement);
	root.render(
		<StrictMode>
			<RouterProvider router={router} />
		</StrictMode>,
	);
}
