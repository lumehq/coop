import { commands } from "@/commands";
import { cn, getReceivers, time } from "@/commons";
import { ArrowUp } from "@phosphor-icons/react";
import * as ScrollArea from "@radix-ui/react-scroll-area";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { listen } from "@tauri-apps/api/event";
import type { NostrEvent } from "nostr-tools";
import { useCallback, useRef } from "react";
import { useEffect } from "react";
import { Virtualizer } from "virtua";

type Payload = {
	event: string;
	sender: string;
};

export const Route = createFileRoute("/$account/chats/$id")({
	component: Screen,
});

function Screen() {
	const { account, id } = Route.useParams();
	const { isLoading, isError, data } = useQuery({
		queryKey: ["chats", id],
		queryFn: async () => {
			const res = await commands.getChatMessages(id);

			if (res.status === "ok") {
				const raw = res.data;
				const events = raw
					.map((item) => JSON.parse(item) as NostrEvent)
					.sort((a, b) => a.created_at - b.created_at);

				return events;
			} else {
				throw new Error(res.error);
			}
		},
	});

	const queryClient = useQueryClient();
	const ref = useRef<HTMLDivElement>(null);

	const renderItem = useCallback(
		(item: NostrEvent) => {
			const self = account === item.pubkey;

			return (
				<div
					key={item.id}
					className="flex items-center justify-between gap-3 my-1.5 px-3 border-l-2 border-transparent hover:border-blue-400"
				>
					<div
						className={cn(
							"flex-1 min-w-0 inline-flex",
							self ? "justify-end" : "justify-start",
						)}
					>
						<div
							className={cn(
								"py-2 px-3 w-fit max-w-[400px] text-pretty break-message rounded-t-2xl",
								!self
									? "bg-neutral-100 dark:bg-neutral-800 rounded-l-md rounded-r-xl"
									: "bg-blue-500 text-white rounded-l-xl rounded-r-md",
							)}
						>
							{item.content}
						</div>
					</div>
					<div className="shrink-0 w-16 flex items-center justify-end">
						<span className="text-xs text-right text-neutral-600 dark:text-neutral-400">
							{time(item.created_at)}
						</span>
					</div>
				</div>
			);
		},
		[data],
	);

	useEffect(() => {
		const unlisten = listen<Payload>("event", async (data) => {
			const event: NostrEvent = JSON.parse(data.payload.event);
			const sender = data.payload.sender;
			const receivers = getReceivers(event.tags);

			if (sender !== account || sender !== id) return;
			if (!receivers.includes(account) || !receivers.includes(id)) return;

			await queryClient.setQueryData(
				["chats", id],
				(prevEvents: NostrEvent[]) => {
					if (!prevEvents) {
						return prevEvents;
					}
					return [...prevEvents, event];
					// queryClient.invalidateQueries(['chats', id]);
				},
			);
		});

		return () => {
			unlisten.then((f) => f());
		};
	}, []);

	return (
		<div className="size-full flex flex-col">
			<div className="h-11 shrink-0 border-b border-neutral-100 dark:border-neutral-900" />
			<ScrollArea.Root
				type={"scroll"}
				scrollHideDelay={300}
				className="overflow-hidden flex-1 w-full"
			>
				<ScrollArea.Viewport
					ref={ref}
					className="relative h-full py-2 [&>div]:!flex [&>div]:flex-col [&>div]:justify-end [&>div]:min-h-full"
				>
					<Virtualizer scrollRef={ref} shift>
						{isLoading ? (
							<p>Loading...</p>
						) : isError || !data ? (
							<p>Error</p>
						) : (
							data.map((item) => renderItem(item))
						)}
					</Virtualizer>
				</ScrollArea.Viewport>
				<ScrollArea.Scrollbar
					className="flex select-none touch-none p-0.5 duration-[160ms] ease-out data-[orientation=vertical]:w-2"
					orientation="vertical"
				>
					<ScrollArea.Thumb className="flex-1 bg-black/40 dark:bg-white/40 rounded-full relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-[44px] before:min-h-[44px]" />
				</ScrollArea.Scrollbar>
				<ScrollArea.Corner className="bg-transparent" />
			</ScrollArea.Root>
			<div className="h-12 shrink-0 flex items-center gap-2 px-3.5">
				<input
					placeholder="Message..."
					className="flex-1 h-9 rounded-full px-3.5 bg-transparent border border-neutral-200 dark:border-neutral-800 focus:outline-none focus:border-blue-500"
				/>
				<button
					type="button"
					className="rounded-full size-9 inline-flex items-center justify-center bg-blue-300 hover:bg-blue-500 dark:bg-blue-700 text-white"
				>
					<ArrowUp className="size-4" />
				</button>
			</div>
		</div>
	);
}
