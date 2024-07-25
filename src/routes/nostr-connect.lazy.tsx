import { commands } from "@/commands";
import { Frame } from "@/components/frame";
import { Spinner } from "@/components/spinner";
import { createLazyFileRoute } from "@tanstack/react-router";
import { message } from "@tauri-apps/plugin-dialog";
import { useState, useTransition } from "react";

export const Route = createLazyFileRoute("/nostr-connect")({
	component: Screen,
});

function Screen() {
	const navigate = Route.useNavigate();

	const [uri, setUri] = useState("");
	const [isPending, startTransition] = useTransition();

	const submit = async () => {
		startTransition(async () => {
			if (!uri.startsWith("bunker://")) {
				await message(
					"You need to enter a valid Connect URI starts with bunker://",
					{ title: "Nostr Connect", kind: "info" },
				);
				return;
			}

			const res = await commands.connectAccount(uri);

			if (res.status === "ok") {
				const npub = res.data;
				const parsed = new URL(uri);
				parsed.searchParams.delete("secret");

				// save connection string
				localStorage.setItem(`${npub}_bunker`, parsed.toString());

				navigate({ to: "/", replace: true });
			} else {
				await message(res.error, { title: "Nostr Connect", kind: "error" });
				return;
			}
		});
	};

	return (
		<div className="size-full flex items-center justify-center">
			<div className="w-[320px] flex flex-col gap-8">
				<div className="flex flex-col gap-1 text-center">
					<h1 className="leading-tight text-xl font-semibold">
						Nostr Connect.
					</h1>
				</div>
				<div className="flex flex-col gap-3">
					<Frame
						className="flex flex-col gap-1 p-3 rounded-xl overflow-hidden"
						shadow
					>
						<label
							htmlFor="uri"
							className="font-medium text-neutral-900 dark:text-neutral-100"
						>
							Connection String
						</label>
						<input
							name="uri"
							type="text"
							placeholder="bunker://..."
							value={uri}
							onChange={(e) => setUri(e.target.value)}
							className="px-3 rounded-lg h-10 bg-transparent border border-neutral-200 dark:border-neutral-800 focus:border-blue-500 focus:outline-none"
						/>
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
						{isPending ? (
							<p className="text-sm text-center text-neutral-600 dark:text-neutral-400">
								Waiting confirmation...
							</p>
						) : null}
					</div>
				</div>
			</div>
		</div>
	);
}
