import { commands } from "@/commands";
import { Frame } from "@/components/frame";
import { Spinner } from "@/components/spinner";
import { Plus, X } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { createLazyFileRoute } from "@tanstack/react-router";
import { message } from "@tauri-apps/plugin-dialog";
import { useState, useTransition } from "react";

export const Route = createLazyFileRoute("/inbox-relays")({
	component: Screen,
});

function Screen() {
	const { account, redirect } = Route.useSearch();
	const { queryClient } = Route.useRouteContext();
	const {
		data: relays,
		error,
		isError,
		isLoading,
	} = useQuery({
		queryKey: ["relays", account],
		queryFn: async () => {
			const res = await commands.getInboxRelays(account);

			if (res.status === "ok") {
				return res.data;
			} else {
				throw new Error(res.error);
			}
		},
		refetchOnWindowFocus: false,
	});

	const [newRelay, setNewRelay] = useState("");
	const [isPending, startTransition] = useTransition();

	const navigate = Route.useNavigate();

	const add = () => {
		try {
			let url = newRelay;

			if (relays?.length >= 3) {
				return message("You should keep relay lists small (1 - 3 relays).", {
					kind: "info",
				});
			}

			if (!url.startsWith("wss://")) {
				url = `wss://${url}`;
			}

			// Validate URL
			const relay = new URL(url);

			// Update
			queryClient.setQueryData(["relays", account], (prev: string[]) => [
				...prev,
				relay.toString(),
			]);
			setNewRelay("");
		} catch {
			message("URL is not valid.", { kind: "error" });
		}
	};

	const remove = (relay: string) => {
		queryClient.setQueryData(["relays", account], (prev: string[]) =>
			prev.filter((item) => item !== relay),
		);
	};

	const submit = () => {
		startTransition(async () => {
			if (!relays?.length) {
				await message("You need to add at least 1 relay", { kind: "info" });
				return;
			}

			const res = await commands.setInboxRelays(relays);

			if (res.status === "ok") {
				navigate({
					to: redirect,
					replace: true,
				});
			} else {
				await message(res.error, {
					title: "Inbox Relays",
					kind: "error",
				});
				return;
			}
		});
	};

	if (isLoading) {
		return (
			<div className="size-full flex items-center justify-center">
				<Spinner />
			</div>
		);
	}

	if (isError) {
		return (
			<div className="size-full flex items-center justify-center">
				<p className="text-sm">{error.message}</p>
			</div>
		);
	}

	return (
		<div className="size-full flex items-center justify-center">
			<div className="w-[320px] flex flex-col gap-8">
				<div className="flex flex-col gap-1 text-center">
					<h1 className="leading-tight text-xl font-semibold">Inbox Relays</h1>
					<p className="text-sm text-neutral-700 dark:text-neutral-300">
						Inbox Relay is used to receive message from others
					</p>
				</div>
				<div className="flex flex-col gap-3">
					<Frame
						className="flex flex-col gap-3 p-3 rounded-xl overflow-hidden"
						shadow
					>
						<div className="text-sm text-neutral-700 dark:text-neutral-300">
							<p className="mb-1.5">
								You need to set at least 1 inbox relay in order to receive
								message from others.
							</p>
							<p>
								If you don't know which relay to add, you can use{" "}
								<span
									onClick={() => setNewRelay("wss://auth.nostr1.com")}
									onKeyDown={() => setNewRelay("wss://auth.nostr1.com")}
									className="font-semibold"
								>
									auth.nostr1.com
								</span>
							</p>
						</div>
						<div className="flex gap-2">
							<input
								name="relay"
								type="text"
								placeholder="ex: relay.nostr.net, ..."
								value={newRelay}
								onChange={(e) => setNewRelay(e.target.value)}
								onKeyDown={(e) => {
									if (e.key === "Enter") add();
								}}
								className="flex-1 px-3 rounded-lg h-9 bg-transparent border border-neutral-200 dark:border-neutral-800 focus:border-blue-500 focus:outline-none placeholder:text-neutral-400 dark:placeholder:text-neutral-600"
							/>
							<button
								type="submit"
								onClick={() => add()}
								className="inline-flex items-center justify-center size-9 rounded-lg bg-neutral-100 hover:bg-neutral-200 dark:bg-neutral-900 dark:hover:bg-neutral-800"
							>
								<Plus className="size-5" />
							</button>
						</div>
						<div className="flex flex-col gap-2">
							{relays.map((relay) => (
								<div
									key={relay}
									className="flex items-center justify-between h-9 px-2 rounded-lg bg-neutral-100 dark:bg-neutral-900"
								>
									<div className="text-sm font-medium">{relay}</div>
									<div className="flex items-center gap-2">
										<button
											type="button"
											onClick={() => remove(relay)}
											className="inline-flex items-center justify-center rounded-md size-7 text-neutral-700 dark:text-white/20 hover:bg-black/10 dark:hover:bg-white/10"
										>
											<X className="size-3" />
										</button>
									</div>
								</div>
							))}
						</div>
					</Frame>
					<div className="flex flex-col items-center gap-1">
						<button
							type="button"
							onClick={() => submit()}
							disabled={isPending || !relays?.length}
							className="inline-flex items-center justify-center w-full h-9 text-sm font-semibold text-white bg-blue-500 rounded-lg shrink-0 hover:bg-blue-600 disabled:opacity-50"
						>
							{isPending ? <Spinner /> : "Continue"}
						</button>
					</div>
				</div>
			</div>
		</div>
	);
}
