import { commands } from "@/commands";
import { ago, cn } from "@/commons";
import { User } from "@/components/user";
import { Plus, UsersThree } from "@phosphor-icons/react";
import * as ScrollArea from "@radix-ui/react-scroll-area";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, Outlet, createLazyFileRoute } from "@tanstack/react-router";
import { listen } from "@tauri-apps/api/event";
import type { NostrEvent } from "nostr-tools";
import { useEffect } from "react";

type Payload = {
	event: string;
	sender: string;
};

export const Route = createLazyFileRoute("/$account/chats")({
	component: Screen,
});

function Screen() {
	return (
		<div className="size-full flex">
			<div
				data-tauri-drag-region
				className="shrink-0 w-[280px] h-full flex flex-col justify-between border-r border-black/5 dark:border-white/5"
			>
				<div data-tauri-drag-region className="flex-1">
					<Header />
					<ChatList />
				</div>
				<div className="h-12 shrink-0 flex items-center px-2.5 border-t border-black/5 dark:border-white/5">
					<CurrentUser />
				</div>
			</div>
			<div className="flex-1 min-w-0 min-h-0 bg-white dark:bg-neutral-900 overflow-auto">
				<Outlet />
			</div>
		</div>
	);
}

function Header() {
	return (
		<div
			data-tauri-drag-region
			className="h-12 px-3.5 flex items-center justify-end"
		>
			<div className="flex items-center gap-2">
				<Link
					to="/new"
					className="size-7 rounded-md inline-flex items-center justify-center text-neutral-600 dark:text-neutral-400 hover:bg-black/10 dark:hover:bg-white/10"
				>
					<UsersThree className="size-4" />
				</Link>
				<Link
					to="/new"
					className="h-7 w-12 rounded-t-md rounded-b-md rounded-l-md rounded-r inline-flex items-center justify-center bg-black/5 hover:bg-black/10 dark:bg-white/5 dark:hover:bg-white/10"
				>
					<Plus className="size-4" />
				</Link>
			</div>
		</div>
	);
}

function ChatList() {
	const { account } = Route.useParams();
	const { isLoading, isError, data } = useQuery({
		queryKey: ["chats"],
		queryFn: async () => {
			const res = await commands.getChats();

			if (res.status === "ok") {
				const raw = res.data;
				const events = raw.map((item) => JSON.parse(item) as NostrEvent);

				return events;
			} else {
				throw new Error(res.error);
			}
		},
		refetchOnWindowFocus: false,
	});

	const queryClient = useQueryClient();

	useEffect(() => {
		const unlisten = listen("synchronized", async () => {
			await queryClient.refetchQueries({ queryKey: ["chats"] });
		});

		return () => {
			unlisten.then((f) => f());
		};
	}, []);

	useEffect(() => {
		const unlisten = listen<Payload>("event", async (data) => {
			const event: NostrEvent = JSON.parse(data.payload.event);
			const chats: NostrEvent[] = await queryClient.getQueryData(["chats"]);

			if (chats) {
				const exist = chats.find((ev) => ev.pubkey === event.pubkey);

				if (!exist) {
					await queryClient.setQueryData(
						["chats"],
						(prevEvents: NostrEvent[]) => {
							if (!prevEvents) return prevEvents;
							if (event.pubkey === account) return;

							return [event, ...prevEvents];
						},
					);
				}
			}
		});

		return () => {
			unlisten.then((f) => f());
		};
	}, []);

	return (
		<ScrollArea.Root
			type={"scroll"}
			scrollHideDelay={300}
			className="overflow-hidden flex-1 w-full"
		>
			<ScrollArea.Viewport className="relative h-full px-1.5">
				{isLoading ? (
					<p>Loading...</p>
				) : isError ? (
					<p>Error</p>
				) : (
					data.map((item) => (
						<Link
							key={item.pubkey}
							to="/$account/chats/$id"
							params={{ account, id: item.pubkey }}
						>
							{({ isActive }) => (
								<User.Provider pubkey={item.pubkey}>
									<User.Root
										className={cn(
											"flex items-center rounded-lg p-2 mb-1 gap-2 hover:bg-black/5 dark:hover:bg-white/5",
											isActive ? "bg-black/5 dark:bg-white/5" : "",
										)}
									>
										<User.Avatar className="shrink-0 size-9 rounded-full object-cover" />
										<div className="flex-1 inline-flex items-center justify-between text-sm">
											<div className="inline-flex leading-tight">
												<User.Name className="max-w-[8rem] truncate font-semibold" />
												<span className="ml-1.5 text-neutral-500">
													{account === item.pubkey ? "(you)" : ""}
												</span>
											</div>
											<span className="leading-tight text-right text-neutral-600 dark:text-neutral-400">
												{ago(item.created_at)}
											</span>
										</div>
									</User.Root>
								</User.Provider>
							)}
						</Link>
					))
				)}
			</ScrollArea.Viewport>
			<ScrollArea.Scrollbar
				className="flex select-none touch-none p-0.5 duration-[160ms] ease-out data-[orientation=vertical]:w-2"
				orientation="vertical"
			>
				<ScrollArea.Thumb className="flex-1 bg-black/40 dark:bg-white/40 rounded-full relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-[44px] before:min-h-[44px]" />
			</ScrollArea.Scrollbar>
			<ScrollArea.Corner className="bg-transparent" />
		</ScrollArea.Root>
	);
}

function CurrentUser() {
	const { account } = Route.useParams();

	return (
		<User.Provider pubkey={account}>
			<User.Root className="inline-flex items-center gap-2">
				<User.Avatar className="size-8 rounded-full object-cover" />
				<User.Name className="text-sm font-medium leading-tight" />
			</User.Root>
		</User.Provider>
	);
}
