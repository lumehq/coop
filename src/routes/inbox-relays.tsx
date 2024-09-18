import { createFileRoute } from "@tanstack/react-router";

type RouteSearch = {
	account: string;
	redirect: string;
};

export const Route = createFileRoute("/inbox-relays")({
	validateSearch: (search: Record<string, string>): RouteSearch => {
		return {
			account: search.account,
			redirect: search.redirect,
		};
	},
});
