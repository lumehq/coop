import { cn } from "@/commons";
import * as Avatar from "@radix-ui/react-avatar";
import { minidenticon } from "minidenticons";
import { useMemo } from "react";
import { useUserContext } from "./provider";

export function UserAvatar({ className }: { className?: string }) {
	const user = useUserContext();
	const fallback = useMemo(
		() =>
			`data:image/svg+xml;utf8,${encodeURIComponent(
				minidenticon(user.pubkey, 60, 50),
			)}`,
		[user.pubkey],
	);

	return (
		<Avatar.Root
			className={cn(
				"shrink-0 block overflow-hidden bg-black/10 dark:bg-white/10",
				user.isLoading ? "animate-pulse" : "",
				className,
			)}
		>
			{!user.isLoading ? (
				<>
					{user.profile?.picture ? (
						<Avatar.Image
							src={`https://wsrv.nl/?url=${user.profile?.picture}&w=200&h=200&default=1`}
							alt={user.pubkey}
							loading="lazy"
							decoding="async"
							className="w-full aspect-square object-cover outline-[.5px] outline-black/15"
						/>
					) : null}
					<Avatar.Fallback>
						<img
							src={fallback}
							alt={user.pubkey}
							className="size-full bg-black dark:bg-white outline-[.5px] outline-black/5"
						/>
					</Avatar.Fallback>
				</>
			) : null}
		</Avatar.Root>
	);
}
