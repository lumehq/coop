import { commands } from "@/commands";
import { npub } from "@/commons";
import { Frame } from "@/components/frame";
import { Spinner } from "@/components/spinner";
import { User } from "@/components/user";
import { Plus, X } from "@phosphor-icons/react";
import { Link, createLazyFileRoute } from "@tanstack/react-router";
import { useEffect, useMemo, useState, useTransition } from "react";

export const Route = createLazyFileRoute("/")({
	component: Screen,
});

function Screen() {
	const context = Route.useRouteContext();
	const navigate = Route.useNavigate();

	const currentDate = useMemo(
		() =>
			new Date().toLocaleString("default", {
				weekday: "long",
				month: "long",
				day: "numeric",
			}),
		[],
	);

	const [accounts, setAccounts] = useState([]);
	const [value, setValue] = useState("");
	const [isPending, startTransition] = useTransition();

	const deleteAccount = async (npub: string) => {
		const res = await commands.deleteAccount(npub);

		if (res.status === "ok") {
			setAccounts((prev) => prev.filter((item) => item !== npub));
		}
	};

	const loginWith = async (npub: string) => {
		setValue(npub);
		startTransition(async () => {
			const res = await commands.login(npub);

			if (res.status === "ok") {
				navigate({
					to: "/$account/chats/new",
					params: { account: res.data },
					replace: true,
				});
			} else {
				if (res.error === "404") {
					navigate({
						to: "/$account/relays",
						params: { account: npub },
						replace: true,
					});
				}
			}
		});
	};

	useEffect(() => {
		setAccounts(context.accounts);
	}, [context.accounts]);

	return (
		<div
			data-tauri-drag-region
			className="size-full flex items-center justify-center"
		>
			<div className="w-[320px] flex flex-col gap-8">
				<div className="flex flex-col gap-1 text-center">
					<h3 className="leading-tight text-neutral-700 dark:text-neutral-300">
						{currentDate}
					</h3>
					<h1 className="leading-tight text-xl font-semibold">Welcome back!</h1>
				</div>
				<Frame
					className="flex flex-col w-full divide-y divide-neutral-100 dark:divide-white/5 rounded-xl overflow-hidden"
					shadow
				>
					{accounts.map((account) => (
						<div
							key={account}
							onClick={() => loginWith(account)}
							onKeyDown={() => loginWith(account)}
							className="group flex items-center justify-between hover:bg-black/5 dark:hover:bg-white/5"
						>
							<User.Provider pubkey={account}>
								<User.Root className="flex items-center gap-2.5 p-3">
									<User.Avatar className="rounded-full size-10" />
									<div className="inline-flex flex-col items-start">
										<User.Name className="max-w-[6rem] truncate font-medium leading-tight" />
										<span className="text-sm text-neutral-700 dark:text-neutral-300">
											{npub(account, 16)}
										</span>
									</div>
								</User.Root>
							</User.Provider>
							<div className="inline-flex items-center justify-center size-10">
								{value === account && isPending ? (
									<Spinner />
								) : (
									<button
										type="button"
										onClick={() => deleteAccount(account)}
										className="size-10 hidden group-hover:flex items-center justify-center text-neutral-600 dark:text-neutral-400"
									>
										<X className="size-4" />
									</button>
								)}
							</div>
						</div>
					))}
					<Link
						to="/new"
						className="flex items-center justify-between hover:bg-black/5 dark:hover:bg-white/5"
					>
						<div className="flex items-center gap-2.5 p-3">
							<div className="inline-flex items-center justify-center rounded-full size-10 bg-neutral-200 dark:bg-white/10">
								<Plus className="size-5" />
							</div>
							<span className="truncate text-sm font-medium leading-tight">
								Add an account
							</span>
						</div>
					</Link>
				</Frame>
			</div>
		</div>
	);
}
