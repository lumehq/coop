import { cn, npub } from "@/commons";
import { useUserContext } from "./provider";

export function UserName({ className }: { className?: string }) {
	const user = useUserContext();

	if (user.isLoading) {
		return (
			<div className="size-4 w-20 rounded bg-black/10 dark:bg-white/10 animate-pulse" />
		);
	}

	return (
		<div className={cn("max-w-[12rem] truncate", className)}>
			{user.profile?.display_name ||
				user.profile?.name ||
				npub(user.pubkey, 16)}
		</div>
	);
}
