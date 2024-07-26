import { commands } from "@/commands";
import { cn, getReceivers, time, useRelays } from "@/commons";
import { Spinner } from "@/components/spinner";
import { ArrowUp, CloudArrowUp, Paperclip } from "@phosphor-icons/react";
import * as ScrollArea from "@radix-ui/react-scroll-area";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createLazyFileRoute } from "@tanstack/react-router";
import { listen } from "@tauri-apps/api/event";
import { message } from "@tauri-apps/plugin-dialog";
import type { NostrEvent } from "nostr-tools";
import { useCallback, useRef, useState, useTransition } from "react";
import { useEffect } from "react";
import { Virtualizer } from "virtua";

type Payload = {
	event: string;
	sender: string;
};

export const Route = createLazyFileRoute("/$account/chats/$id")({
	component: Screen,
});

function Screen() {
	const { id } = Route.useParams();
	const { isLoading, data: relays } = useRelays(id);

	useEffect(() => {
		if (!isLoading && relays?.length)
			commands.subscribeTo(id, relays).then(() => console.log("sub: ", id));

		return () => {
			if (!isLoading && relays?.length)
				commands.unsubscribe(id).then(() => console.log("unsub: ", id));
		};
	}, [isLoading, relays]);

	return (
		<div className="size-full flex flex-col">
			<div className="h-11 shrink-0 border-b border-neutral-100 dark:border-neutral-800" />
			<List />
			<Form />
		</div>
	);
}

function List() {
	const { account, id } = Route.useParams();
	const { isLoading: rl, isError: rE } = useRelays(id);
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
		enabled: !rl && !rE,
		refetchOnWindowFocus: false,
	});

	const queryClient = useQueryClient();
	const ref = useRef<HTMLDivElement>(null);

	const renderItem = useCallback(
		(item: NostrEvent, idx: number) => {
			const self = account === item.pubkey;

			return (
				<div
					key={idx + item.id}
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
			const group = [account, id];

			if (!group.includes(sender)) return;
			if (!group.some((item) => receivers.includes(item))) return;

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
		<ScrollArea.Root
			type={"scroll"}
			scrollHideDelay={300}
			className="overflow-hidden flex-1 w-full"
		>
			<ScrollArea.Viewport
				ref={ref}
				className="relative h-full py-2 [&>div]:!flex [&>div]:flex-col [&>div]:justify-end [&>div]:min-h-full"
			>
				<Virtualizer scrollRef={ref}>
					{isLoading || !data ? (
						<div className="w-full h-56 flex items-center justify-center">
							<div className="flex items-center gap-1.5">
								<Spinner />
								Loading message...
							</div>
						</div>
					) : isError ? (
						<div className="w-full h-56 flex items-center justify-center">
							<div className="flex items-center gap-1.5">
								Cannot load message. Please try again later.
							</div>
						</div>
					) : (
						data.map((item, idx) => renderItem(item, idx))
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
	);
}

function Form() {
	const { id } = Route.useParams();
	const { isLoading, isError, data: relays } = useRelays(id);

	const [newMessage, setNewMessage] = useState("");
	const [isPending, startTransition] = useTransition();

	const submit = async () => {
		startTransition(async () => {
			if (newMessage.length < 1) return;

			const res = await commands.sendMessage(id, newMessage, relays);

			if (res.status === "ok") {
				setNewMessage("");
			} else {
				await message(res.error, { title: "Coop", kind: "error" });
				return;
			}
		});
	};

	return (
		<div className="h-12 shrink-0 flex items-center justify-center px-3.5">
			{isLoading ? (
				<div className="inline-flex items-center justify-center gap-2 h-9 w-fit px-3 bg-neutral-100 dark:bg-neutral-800 rounded-full text-sm">
					<Spinner />
					Connecting to inbox relays
				</div>
			) : isError || !relays.length ? (
				<div className="inline-flex items-center justify-center gap-2 h-9 w-fit px-3 bg-neutral-100 dark:bg-neutral-800 rounded-full text-sm">
					This user doesn't have inbox relays. You cannot send messages to them.
				</div>
			) : (
				<div className="flex-1 flex items-center gap-2">
					<div className="inline-flex gap-px">
						<div
							title="Attach media"
							className="size-9 inline-flex items-center justify-center hover:bg-neutral-100 dark:hover:bg-neutral-800 rounded-full"
						>
							<Paperclip className="size-5" />
						</div>
						<div
							title="Inbox Relays"
							className="size-9 inline-flex items-center justify-center hover:bg-neutral-100 dark:hover:bg-neutral-800 rounded-full"
						>
							<CloudArrowUp className="size-5" />
						</div>
					</div>
					<input
						placeholder="Message..."
						value={newMessage}
						onChange={(e) => setNewMessage(e.target.value)}
						onKeyDown={(e) => {
							if (e.key === "Enter") submit();
						}}
						className="flex-1 h-9 rounded-full px-3.5 bg-transparent border border-neutral-200 dark:border-neutral-800 focus:outline-none focus:border-blue-500"
					/>
					<button
						type="button"
						title="Send message"
						disabled={isPending}
						onClick={() => submit()}
						className="rounded-full size-9 inline-flex items-center justify-center bg-blue-300 hover:bg-blue-500 dark:bg-blue-700 dark:hover:bg-blue-800 text-white"
					>
						{isPending ? <Spinner /> : <ArrowUp className="size-5" />}
					</button>
				</div>
			)}
		</div>
	);
}
