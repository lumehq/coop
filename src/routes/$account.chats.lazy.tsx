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
import * as ScrollArea from "@radix-ui/react-scroll-area";
import { useQuery } from "@tanstack/react-query";
import { Link, Outlet, createLazyFileRoute } from "@tanstack/react-router";
import { listen } from "@tauri-apps/api/event";
import { Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { message } from "@tauri-apps/plugin-dialog";
import type { NostrEvent } from "nostr-tools";
import { useCallback, useEffect, useState, useTransition } from "react";

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
	});

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
				} else {
					const index = chats.findIndex((item) => item.pubkey === event.pubkey);
					const newEvents = [...chats];

					if (index !== -1) {
						newEvents[index] = {
							...event,
						};

						await queryClient.setQueryData(["chats"], newEvents);
					}
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
					<div>
						{[...Array(5).keys()].map((i) => (
							<div
								key={i}
								className="flex items-center rounded-lg p-2 mb-1 gap-2"
							>
								<div className="size-9 rounded-full animate-pulse bg-black/10 dark:bg-white/10" />
								<div className="size-4 w-20 rounded animate-pulse bg-black/10 dark:bg-white/10" />
							</div>
						))}
					</div>
				) : !data?.length ? (
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
							{({ isActive }) => (
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

	const sendMessage = async () => {
		startTransition(async () => {
			if (!newMessage.length) return;
			if (!target.length) return;

			const res = await commands.sendMessage(target, newMessage);

			if (res.status === "ok") {
				navigate({
					to: "/$account/chats/$id",
					params: { account, id: target },
				});
			} else {
				await message(res.error, { title: "Coop", kind: "error" });
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
							Send to
							<Dialog.Close asChild>
								<button type="button">
									<X className="size-4" />
								</button>
							</Dialog.Close>
						</div>
						<div className="flex items-center gap-1 px-3.5 border-b border-neutral-100 dark:border-neutral-800">
							<span className="shrink-0 font-medium">To:</span>
							<input
								placeholder="npub1..."
								value={target}
								onChange={(e) => setTarget(e.target.value)}
								disabled={isPending || isLoading}
								className="flex-1 h-9 bg-transparent focus:outline-none placeholder:text-neutral-400 dark:placeholder:text-neutral-600"
							/>
						</div>
						<div className="flex items-center gap-1 px-3.5 border-b border-neutral-100 dark:border-neutral-800">
							<span className="shrink-0 font-medium">Message:</span>
							<input
								placeholder="hello..."
								value={newMessage}
								onChange={(e) => setNewMessage(e.target.value)}
								disabled={isPending || isLoading}
								className="flex-1 h-9 bg-transparent focus:outline-none placeholder:text-neutral-400 dark:placeholder:text-neutral-600"
							/>
							<button
								type="button"
								disabled={isPending || isLoading || !newMessage.length}
								onClick={() => sendMessage()}
								className="rounded-full size-7 inline-flex items-center justify-center bg-blue-300 hover:bg-blue-500 dark:bg-blue-700 dark:hover:bg-blue-800 text-white"
							>
								<ArrowRight className="size-4" />
							</button>
						</div>
					</div>
					<ScrollArea.Root
						type={"scroll"}
						scrollHideDelay={300}
						className="overflow-hidden flex-1 size-full"
					>
						<ScrollArea.Viewport className="relative h-full p-2">
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
												<User.Avatar className="size-10 rounded-full" />
												<User.Name className="font-medium" />
											</User.Root>
										</User.Provider>
									</button>
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
				text: "Contacts",
				action: () =>
					navigate({
						to: "/$account/contacts",
						params: { account: params.account },
					}),
			}),
			MenuItem.new({
				text: "Settings",
				action: () => navigate({ to: "/" }),
			}),
			MenuItem.new({
				text: "Feedback",
				action: () => navigate({ to: "/" }),
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