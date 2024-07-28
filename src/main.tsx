import { createSyncStoragePersister } from "@tanstack/query-sync-storage-persister";
import { QueryClient } from "@tanstack/react-query";
import { PersistQueryClientProvider } from "@tanstack/react-query-persist-client";
import { RouterProvider, createRouter } from "@tanstack/react-router";
import { type } from "@tauri-apps/plugin-os";
import { StrictMode } from "react";
import ReactDOM from "react-dom/client";
import "./app.css";
// Import the generated route tree
import { routeTree } from "./routes.gen";

const queryClient = new QueryClient({
	defaultOptions: {
		queries: {
			gcTime: 1000 * 60 * 60 * 24,
		},
	},
});

const persister = createSyncStoragePersister({
	storage: window.localStorage,
});

const platform = type();

const router = createRouter({
	routeTree,
	context: {
		queryClient,
		platform,
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
			<PersistQueryClientProvider
				client={queryClient}
				persistOptions={{ persister }}
			>
				<RouterProvider router={router} />
			</PersistQueryClientProvider>
		</StrictMode>,
	);
}
