import { commands } from "@/commands";
import { GoBack } from "@/components/back";
import { Frame } from "@/components/frame";
import { Spinner } from "@/components/spinner";
import { createLazyFileRoute } from "@tanstack/react-router";
import { message } from "@tauri-apps/plugin-dialog";
import { useState, useTransition } from "react";

export const Route = createLazyFileRoute("/create-account")({
	component: Screen,
});

function Screen() {
	const navigate = Route.useNavigate();

	const [picture, setPicture] = useState(null);
	const [name, setName] = useState("");
	const [isPending, startTransition] = useTransition();

	const submit = async () => {
		startTransition(async () => {
			const res = await commands.createAccount(name, picture);

			if (res.status === "ok") {
				navigate({
					to: "/$account/relays",
					params: { account: res.data },
					replace: true,
				});
			} else {
				await message(res.error, {
					title: "New Identity",
					kind: "error",
				});
				return;
			}
		});
	};

	return (
		<div className="size-full flex items-center justify-center">
			<div className="w-[320px] flex flex-col gap-8">
				<div className="flex flex-col gap-1 text-center">
					<h1 className="leading-tight text-xl font-semibold">New Identity</h1>
				</div>
				<div className="flex flex-col gap-3">
					<Frame
						className="flex flex-col gap-3 p-3 rounded-xl overflow-hidden"
						shadow
					>
						<div className="flex flex-col gap-1">
							<label
								htmlFor="avatar"
								className="font-medium text-neutral-900 dark:text-neutral-100"
							>
								Avatar
							</label>
							<input
								name="avatar"
								type="text"
								placeholder="https://"
								value={picture}
								onChange={(e) => setPicture(e.target.value)}
								className="px-3 rounded-lg h-10 bg-transparent border border-neutral-200 dark:border-neutral-800 focus:border-blue-500 focus:outline-none"
							/>
						</div>
						<div className="flex flex-col gap-1">
							<label
								htmlFor="name"
								className="font-medium text-neutral-900 dark:text-neutral-100"
							>
								Name
							</label>
							<input
								name="name"
								type="text"
								value={name}
								onChange={(e) => setName(e.target.value)}
								className="px-3 rounded-lg h-10 bg-transparent border border-neutral-200 dark:border-neutral-800 focus:border-blue-500 focus:outline-none"
							/>
						</div>
					</Frame>
					<div className="flex flex-col items-center gap-1">
						<button
							type="button"
							onClick={() => submit()}
							disabled={isPending}
							className="inline-flex items-center justify-center w-full h-9 text-sm font-semibold text-white bg-blue-500 rounded-lg shrink-0 hover:bg-blue-600 disabled:opacity-50"
						>
							{isPending ? <Spinner /> : "Continue"}
						</button>
						<GoBack className="mt-2 w-full text-sm text-neutral-600 dark:text-neutral-400 inline-flex items-center justify-center">
							Back
						</GoBack>
					</div>
				</div>
			</div>
		</div>
	);
}
