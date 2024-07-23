import { RouterProvider, createRouter } from "@tanstack/react-router";
import { type } from "@tauri-apps/plugin-os";
import { StrictMode } from "react";
import ReactDOM from "react-dom/client";
import "./app.css";

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
// Import the generated route tree
import { routeTree } from "./routes.gen";

// Create a new router instance
const queryClient = new QueryClient();
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
			<QueryClientProvider client={queryClient}>
				<RouterProvider router={router} />
			</QueryClientProvider>
		</StrictMode>,
	);
}
