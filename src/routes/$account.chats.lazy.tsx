import { commands } from "@/commands";
import { ago, cn } from "@/commons";
import { Spinner } from "@/components/spinner";
import { User } from "@/components/user";
import {
	ArrowRight,
	CaretDown,
	CirclesFour,
	Plus,
	X,
} from "@phosphor-icons/react";
import * as Dialog from "@radix-ui/react-dialog";
import * as Progress from "@radix-ui/react-progress";
import * as ScrollArea from "@radix-ui/react-scroll-area";
import { useQuery } from "@tanstack/react-query";
import { Link, Outlet, createLazyFileRoute } from "@tanstack/react-router";
import { listen } from "@tauri-apps/api/event";
import { Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { message } from "@tauri-apps/plugin-dialog";
import { open } from "@tauri-apps/plugin-shell";
import { type NostrEvent, nip19 } from "nostr-tools";
import { useCallback, useEffect, useRef, useState, useTransition } from "react";
import { Virtualizer } from "virtua";

type EventPayload = {
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
				<Header />
				<ChatList />
			</div>
			<div className="flex-1 min-w-0 min-h-0 bg-white dark:bg-neutral-900 overflow-auto">
				<Outlet />
			</div>
		</div>
	);
}

function Header() {
	const { platform } = Route.useRouteContext();
	const { account } = Route.useParams();

	return (
		<div
			data-tauri-drag-region
			className={cn(
				"shrink-0 h-12 flex items-center justify-between",
				platform === "macos" ? "pl-[78px] pr-3.5" : "px-3.5",
			)}
		>
			<CurrentUser />
			<div className="flex items-center justify-end gap-2">
				<Link
					to="/$account/contacts"
					params={{ account }}
					className="size-8 rounded-full inline-flex items-center justify-center bg-black/5 hover:bg-black/10 dark:bg-white/5 dark:hover:bg-white/10"
				>
					<CirclesFour className="size-4" />
				</Link>
				<Compose />
			</div>
		</div>
	);
}

function ChatList() {
	const { account } = Route.useParams();
	const { queryClient } = Route.useRouteContext();
	const { isLoading, data } = useQuery({
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
		select: (data) => data.sort((a, b) => b.created_at - a.created_at),
		refetchOnMount: false,
		refetchOnWindowFocus: false,
	});

	const [isSync, setIsSync] = useState(false);
	const [progress, setProgress] = useState(0);

	useEffect(() => {
		const timer = setInterval(
			() => setProgress((prev) => (prev <= 100 ? prev + 4 : 100)),
			1200,
		);
		return () => clearInterval(timer);
	}, []);

	useEffect(() => {
		const unlisten = listen("synchronized", async () => {
			await queryClient.refetchQueries({ queryKey: ["chats"] });
			setIsSync(true);
		});

		return () => {
			unlisten.then((f) => f());
		};
	}, []);

	useEffect(() => {
		const unlisten = listen<EventPayload>("event", async (data) => {
			const event: NostrEvent = JSON.parse(data.payload.event);
			const chats: NostrEvent[] = await queryClient.getQueryData(["chats"]);

			if (chats) {
				const index = chats.findIndex((item) => item.pubkey === event.pubkey);

				if (index === -1) {
					await queryClient.setQueryData(
						["chats"],
						(prevEvents: NostrEvent[]) => {
							if (!prevEvents) return prevEvents;
							if (event.pubkey === account) return;

							return [event, ...prevEvents];
						},
					);
				} else {
					const newEvents = [...chats];
					newEvents[index] = {
						...event,
					};

					await queryClient.setQueryData(["chats"], newEvents);
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
			className="relative overflow-hidden flex-1 w-full"
		>
			<ScrollArea.Viewport className="relative h-full px-1.5">
				{isLoading ? (
					<>
						{[...Array(5).keys()].map((i) => (
							<div
								key={i}
								className="flex items-center rounded-lg p-2 mb-1 gap-2"
							>
								<div className="size-9 rounded-full animate-pulse bg-black/10 dark:bg-white/10" />
								<div className="size-4 w-20 rounded animate-pulse bg-black/10 dark:bg-white/10" />
							</div>
						))}
					</>
				) : isSync && !data.length ? (
					<div className="p-2">
						<div className="px-2 h-12 w-full rounded-lg bg-black/5 dark:bg-white/5 flex items-center justify-center text-sm">
							No chats.
						</div>
					</div>
				) : (
					data.map((item) => (
						<Link
							key={item.id + item.pubkey}
							to="/$account/chats/$id"
							params={{ account, id: item.pubkey }}
						>
							{({ isActive, isTransitioning }) => (
								<User.Provider pubkey={item.pubkey}>
									<User.Root
										className={cn(
											"flex items-center rounded-lg p-2 mb-1 gap-2 hover:bg-black/5 dark:hover:bg-white/5",
											isActive ? "bg-black/5 dark:bg-white/5" : "",
										)}
									>
										<User.Avatar className="size-8 rounded-full" />
										<div className="flex-1 inline-flex items-center justify-between text-sm">
											<div className="inline-flex leading-tight">
												<User.Name className="max-w-[8rem] truncate font-semibold" />
												<span className="ml-1.5 text-neutral-500">
													{account === item.pubkey ? "(you)" : ""}
												</span>
											</div>
											{isTransitioning ? (
												<Spinner className="size-4" />
											) : (
												<span className="leading-tight text-right text-neutral-600 dark:text-neutral-400">
													{ago(item.created_at)}
												</span>
											)}
										</div>
									</User.Root>
								</User.Provider>
							)}
						</Link>
					))
				)}
			</ScrollArea.Viewport>
			{!isSync ? <SyncPopup progress={progress} /> : null}
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

function SyncPopup({ progress }: { progress: number }) {
	return (
		<div className="absolute bottom-0 w-full p-4">
			<div className="relative flex flex-col items-center gap-1.5">
				<Progress.Root
					className="relative overflow-hidden bg-black/20 dark:bg-white/20 rounded-full w-full h-1"
					style={{
						transform: "translateZ(0)",
					}}
					value={progress}
				>
					<Progress.Indicator
						className="bg-blue-500 size-full transition-transform duration-[660ms] ease-[cubic-bezier(0.65, 0, 0.35, 1)]"
						style={{ transform: `translateX(-${100 - progress}%)` }}
					/>
				</Progress.Root>
				<span className="text-center text-xs">Syncing message...</span>
			</div>
		</div>
	);
}

function Compose() {
	const [isOpen, setIsOpen] = useState(false);
	const [target, setTarget] = useState("");
	const [newMessage, setNewMessage] = useState("");
	const [isPending, startTransition] = useTransition();

	const { account } = Route.useParams();
	const { isLoading, data: contacts } = useQuery({
		queryKey: ["contacts", account],
		queryFn: async () => {
			const res = await commands.getContactList();

			if (res.status === "ok") {
				return res.data;
			} else {
				return [];
			}
		},
		refetchOnWindowFocus: false,
		enabled: isOpen,
	});

	const navigate = Route.useNavigate();
	const scrollRef = useRef<HTMLDivElement>(null);

	const pasteFromClipboard = async () => {
		const val = await readText();
		setTarget(val);
	};

	const sendMessage = () => {
		startTransition(async () => {
			if (!newMessage.length) return;
			if (!target.length) return;
			if (!target.startsWith("npub1")) {
				await message("You must enter the public key as npub", {
					title: "Send Message",
					kind: "error",
				});
				return;
			}

			const decoded = nip19.decode(target);
			let id: string;

			if (decoded.type !== "npub") {
				await message("You must enter the public key as npub", {
					title: "Send Message",
					kind: "error",
				});
				return;
			} else {
				id = decoded.data;
			}

			// Connect to user's inbox relays
			const connect = await commands.connectInboxRelays(target, false);

			// Send message
			if (connect.status === "ok") {
				const res = await commands.sendMessage(id, newMessage);

				if (res.status === "ok") {
					setTarget("");
					setNewMessage("");
					setIsOpen(false);

					navigate({
						to: "/$account/chats/$id",
						params: { account, id },
					});
				} else {
					await message(res.error, { title: "Send Message", kind: "error" });
					return;
				}
			} else {
				await message(connect.error, {
					title: "Connect Inbox Relays",
					kind: "error",
				});
				return;
			}
		});
	};

	return (
		<Dialog.Root open={isOpen} onOpenChange={setIsOpen}>
			<Dialog.Trigger asChild>
				<button
					type="button"
					className="size-8 rounded-full inline-flex items-center justify-center bg-black/10 hover:bg-black/20 dark:bg-white/10 dark:hover:bg-white/20"
				>
					<Plus className="size-4" weight="bold" />
				</button>
			</Dialog.Trigger>
			<Dialog.Portal>
				<Dialog.Overlay className="bg-black/20 dark:bg-white/20 data-[state=open]:animate-overlay fixed inset-0" />
				<Dialog.Content className="flex flex-col data-[state=open]:animate-content fixed top-[50%] left-[50%] w-full h-full max-h-[500px] max-w-[400px] translate-x-[-50%] translate-y-[-50%] rounded-xl bg-white dark:bg-neutral-900 shadow-[hsl(206_22%_7%_/_35%)_0px_10px_38px_-10px,_hsl(206_22%_7%_/_20%)_0px_10px_20px_-15px] focus:outline-none">
					<div className="h-28 shrink-0 flex flex-col justify-end">
						<div className="h-10 inline-flex items-center justify-between px-3.5 text-sm font-semibold text-neutral-600 dark:text-neutral-400">
							<Dialog.Title>Send to</Dialog.Title>
							<Dialog.Close asChild>
								<button type="button">
									<X className="size-4" />
								</button>
							</Dialog.Close>
						</div>
						<div className="flex items-center gap-1 px-3.5 border-b border-neutral-100 dark:border-neutral-800">
							<span className="shrink-0 font-medium">To:</span>
							<div className="flex-1 relative">
								<input
									placeholder="npub1..."
									value={target}
									onChange={(e) => setTarget(e.target.value)}
									disabled={isPending}
									className="w-full pr-14 h-9 bg-transparent focus:outline-none placeholder:text-neutral-400 dark:placeholder:text-neutral-600"
								/>
								<button
									type="button"
									onClick={() => pasteFromClipboard()}
									className="absolute uppercase top-1/2 right-2 transform -translate-y-1/2 text-xs font-semibold text-blue-500"
								>
									Paste
								</button>
							</div>
						</div>
						<div className="flex items-center gap-1 px-3.5 border-b border-neutral-100 dark:border-neutral-800">
							<span className="shrink-0 font-medium">Message:</span>
							<input
								placeholder="hello..."
								value={newMessage}
								onChange={(e) => setNewMessage(e.target.value)}
								disabled={isPending}
								className="flex-1 h-9 bg-transparent focus:outline-none placeholder:text-neutral-400 dark:placeholder:text-neutral-600"
							/>
							<button
								type="button"
								disabled={isPending || isLoading || !newMessage.length}
								onClick={() => sendMessage()}
								className="rounded-full size-7 inline-flex items-center justify-center bg-blue-300 hover:bg-blue-500 dark:bg-blue-700 dark:hover:bg-blue-800 text-white"
							>
								{isPending ? (
									<Spinner className="size-4" />
								) : (
									<ArrowRight className="size-4" />
								)}
							</button>
						</div>
					</div>
					<ScrollArea.Root
						type={"scroll"}
						scrollHideDelay={300}
						className="overflow-hidden flex-1 size-full"
					>
						<ScrollArea.Viewport
							ref={scrollRef}
							className="relative h-full p-2"
						>
							<Virtualizer scrollRef={scrollRef} overscan={1}>
								{isLoading ? (
									<div className="h-[400px] flex items-center justify-center">
										<Spinner className="size-4" />
									</div>
								) : !contacts?.length ? (
									<div className="h-[400px] flex items-center justify-center">
										<p className="text-sm">Contact is empty.</p>
									</div>
								) : (
									contacts?.map((contact) => (
										<button
											key={contact}
											type="button"
											onClick={() => setTarget(contact)}
											className="block w-full p-2 rounded-lg hover:bg-neutral-100 dark:hover:bg-neutral-800"
										>
											<User.Provider pubkey={contact}>
												<User.Root className="flex items-center gap-2">
													<User.Avatar className="size-8 rounded-full" />
													<User.Name className="text-sm font-medium" />
												</User.Root>
											</User.Provider>
										</button>
									))
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
				</Dialog.Content>
			</Dialog.Portal>
		</Dialog.Root>
	);
}

function CurrentUser() {
	const params = Route.useParams();
	const navigate = Route.useNavigate();

	const showContextMenu = useCallback(async (e: React.MouseEvent) => {
		e.preventDefault();

		const menuItems = await Promise.all([
			MenuItem.new({
				text: "Copy Public Key",
				action: async () => {
					const npub = nip19.npubEncode(params.account);
					await writeText(npub);
				},
			}),
			MenuItem.new({
				text: "Settings",
				action: () => navigate({ to: "/" }),
			}),
			MenuItem.new({
				text: "Feedback",
				action: async () => await open("https://github.com/lumehq/coop/issues"),
			}),
			PredefinedMenuItem.new({ item: "Separator" }),
			MenuItem.new({
				text: "Switch account",
				action: () => navigate({ to: "/" }),
			}),
		]);

		const menu = await Menu.new({
			items: menuItems,
		});

		await menu.popup().catch((e) => console.error(e));
	}, []);

	return (
		<button
			type="button"
			onClick={(e) => showContextMenu(e)}
			className="h-8 inline-flex items-center gap-1.5"
		>
			<User.Provider pubkey={params.account}>
				<User.Root className="shrink-0">
					<User.Avatar className="size-8 rounded-full" />
				</User.Root>
			</User.Provider>
			<CaretDown className="size-3 text-neutral-600 dark:text-neutral-400" />
		</button>
	);
}
