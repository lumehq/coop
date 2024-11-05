import { experimental_createPersister } from "@tanstack/query-persist-client-core";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, createRouter } from "@tanstack/react-router";
import { type } from "@tauri-apps/plugin-os";
import { LazyStore } from "@tauri-apps/plugin-store";
import { type ReactNode, StrictMode } from "react";
import ReactDOM from "react-dom/client";
import { newQueryStorage } from "./commons";

// Global CSS
import "./global.css";
// Import the generated route tree
import { routeTree } from "./routes.gen";

// Register the router instance for type safety
declare module "@tanstack/react-router" {
	interface Register {
		router: typeof router;
	}
}

const platform = type();
const store = new LazyStore(".data", { autoSave: 300 });
const storage = newQueryStorage(store);
const queryClient = new QueryClient({
	defaultOptions: {
		queries: {
			gcTime: 1000 * 20, // 20 seconds
			persister: experimental_createPersister({
				storage: storage,
				maxAge: 1000 * 60 * 60 * 6, // 6 hours
			}),
		},
	},
});

const router = createRouter({
	routeTree,
	context: {
		queryClient,
		platform,
	},
	Wrap: ({ children }: { children: ReactNode }) => {
		return (
			<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
		);
	},
});

const rootElement = document.getElementById("root");
const root = ReactDOM.createRoot(rootElement as unknown as HTMLElement);

root.render(
	<StrictMode>
		<RouterProvider router={router} />
	</StrictMode>,
);
