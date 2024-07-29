import { User } from "@/components/user";
import { X } from "@phosphor-icons/react";
import * as ScrollArea from "@radix-ui/react-scroll-area";
import { Link, createLazyFileRoute } from "@tanstack/react-router";

export const Route = createLazyFileRoute("/$account/contacts")({
	component: Screen,
});

function Screen() {
	const params = Route.useParams();
	const contacts = Route.useLoaderData();

	return (
		<ScrollArea.Root
			type={"scroll"}
			scrollHideDelay={300}
			className="overflow-hidden size-full flex flex-col"
		>
			<div
				data-tauri-drag-region
				className="h-12 shrink-0 flex items-center justify-between px-3.5"
			>
				<div />
				<div className="text-sm font-semibold uppercase">Contact List</div>
				<div className="inline-flex items-center justify-end">
					<Link
						to="/$account/chats/new"
						params={{ account: params.account }}
						className="size-7 inline-flex items-center justify-center rounded-md hover:bg-black/5 dark:hover:bg-white/5"
					>
						<X className="size-5" />
					</Link>
				</div>
			</div>
			<ScrollArea.Viewport className="relative h-full flex-1 px-3.5 pb-3.5">
				<div className="grid grid-cols-4 gap-3">
					{contacts.map((contact) => (
						<Link
							key={contact}
							to="/$account/chats/$id"
							params={{ account: params.account, id: contact }}
						>
							<User.Provider key={contact} pubkey={contact}>
								<User.Root className="h-44 flex flex-col items-center justify-center gap-3 p-2 rounded-lg hover:bg-black/5 dark:hover:bg-white/5">
									<User.Avatar className="size-16 rounded-full" />
									<User.Name className="text-sm font-medium" />
								</User.Root>
							</User.Provider>
						</Link>
					))}
				</div>
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
