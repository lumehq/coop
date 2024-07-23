import { npub } from "@/commons";
import { Spinner } from "@/components/spinner";
import { User } from "@/components/user";
import { Plus } from "@phosphor-icons/react";
import { Link, createFileRoute, redirect } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { useMemo, useState } from "react";

export const Route = createFileRoute("/")({
	beforeLoad: async () => {
		const accounts: string[] = await invoke("get_accounts");

		if (!accounts.length) {
			throw redirect({
				to: "/new",
				replace: true,
			});
		}

		return { accounts };
	},
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

	const [loading, setLoading] = useState({ npub: "", status: false });

	const login = async (npub: string) => {
		try {
			setLoading({ npub, status: true });

			const status = await invoke("login", { id: npub });

			if (status) {
				return navigate({
					to: "/$account/chats",
					params: { account: npub },
					replace: true,
				});
			}
		} catch (e) {
			setLoading({ npub: "", status: false });
		}
	};

	return (
		<div className="size-full flex items-center justify-center">
			<div className="w-[320px] flex flex-col gap-8">
				<div className="flex flex-col gap-1 text-center">
					<h3 className="leading-tight text-neutral-700 dark:text-neutral-300">
						{currentDate}
					</h3>
					<h1 className="leading-tight text-xl font-semibold">Welcome back!</h1>
				</div>
				<div className="flex flex-col w-full bg-white divide-y divide-neutral-100 dark:divide-white/5 rounded-xl shadow-lg shadow-neutral-500/10 dark:shadow-none dark:bg-white/10 dark:ring-1 dark:ring-white/5">
					{context.accounts.map((account) => (
						<div
							key={account}
							onClick={() => login(account)}
							onKeyDown={() => login(account)}
							className="flex items-center justify-between hover:bg-black/5 dark:hover:bg-white/5"
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
								{loading.npub === account && loading.status ? (
									<Spinner />
								) : null}
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
							<span className="max-w-[6rem] truncate text-sm font-medium leading-tight">
								Add an account
							</span>
						</div>
					</Link>
				</div>
			</div>
		</div>
	);
}
