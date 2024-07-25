import { commands } from "@/commands";
import { Frame } from "@/components/frame";
import { Spinner } from "@/components/spinner";
import { createLazyFileRoute } from "@tanstack/react-router";
import { message } from "@tauri-apps/plugin-dialog";
import { useState, useTransition } from "react";

export const Route = createLazyFileRoute("/import-key")({
	component: Screen,
});

function Screen() {
	const navigate = Route.useNavigate();

	const [key, setKey] = useState("");
	const [password, setPassword] = useState("");
	const [isPending, startTransition] = useTransition();

	const submit = async () => {
		startTransition(async () => {
			if (!key.startsWith("nsec1")) {
				await message(
					"You need to enter a valid private key starts with nsec or ncryptsec",
					{ title: "Import Key", kind: "info" },
				);
				return;
			}

			const res = await commands.importKey(key, password);

			if (res.status === "ok") {
				navigate({ to: "/", replace: true });
			} else {
				await message(res.error, {
					title: "Import Private Ket",
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
					<h1 className="leading-tight text-xl font-semibold">
						Import Private Key
					</h1>
				</div>
				<div className="flex flex-col gap-3">
					<Frame
						className="flex flex-col gap-3 p-3 rounded-xl overflow-hidden"
						shadow
					>
						<div className="flex flex-col gap-1">
							<label
								htmlFor="key"
								className="font-medium text-neutral-900 dark:text-neutral-100"
							>
								Private Key
							</label>
							<input
								name="key"
								type="text"
								placeholder="nsec or ncryptsec..."
								value={key}
								onChange={(e) => setKey(e.target.value)}
								className="px-3 rounded-lg h-10 bg-transparent border border-neutral-200 dark:border-neutral-800 focus:border-blue-500 focus:outline-none"
							/>
						</div>
						<div className="flex flex-col gap-1">
							<label
								htmlFor="password"
								className="font-medium text-neutral-900 dark:text-neutral-100"
							>
								Password (Optional)
							</label>
							<input
								name="password"
								type="password"
								value={password}
								onChange={(e) => setPassword(e.target.value)}
								className="px-3 rounded-lg h-10 bg-transparent border border-neutral-200 dark:border-neutral-800 focus:border-blue-500 focus:outline-none"
							/>
						</div>
					</Frame>
					<div className="flex flex-col items-center gap-1">
						<button
							type="button"
							onClick={() => submit()}
							disabled={isPending}
							className="inline-flex items-center justify-center w-full h-10 text-sm font-semibold text-white bg-blue-500 rounded-lg shrink-0 hover:bg-blue-600 disabled:opacity-50"
						>
							{isPending ? <Spinner /> : "Continue"}
						</button>
					</div>
				</div>
			</div>
		</div>
	);
}
