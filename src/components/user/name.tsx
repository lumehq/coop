import { cn } from "@/commons";
import { useUserContext } from "./provider";
import { useMemo } from "react";
import { uniqueNamesGenerator, names } from "unique-names-generator";

export function UserName({ className }: { className?: string }) {
	const user = useUserContext();
	const name = useMemo(
		() => uniqueNamesGenerator({ dictionaries: [names] }),
		[user.pubkey],
	);

	if (user.isLoading) {
		return (
			<div className="size-4 w-20 bg-black/10 dark:bg-white/10 animate-pulse" />
		);
	}

	return (
		<div className={cn("max-w-[12rem] truncate", className)}>
			{user.profile?.display_name || user.profile?.name || name}
		</div>
	);
}
