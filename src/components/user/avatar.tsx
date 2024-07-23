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
				"shrink-0 block overflow-hidden bg-neutral-200 dark:bg-neutral-800",
				className,
			)}
		>
			<Avatar.Image
				src={user.profile?.picture}
				alt={user.pubkey}
				loading="lazy"
				decoding="async"
				className="w-full aspect-square object-cover outline-[.5px] outline-black/5 content-visibility-auto contain-intrinsic-size-[auto]"
			/>
			<Avatar.Fallback>
				<img
					src={fallback}
					alt={user.pubkey}
					className="size-full bg-black dark:bg-white outline-[.5px] outline-black/5 content-visibility-auto contain-intrinsic-size-[auto]"
				/>
			</Avatar.Fallback>
		</Avatar.Root>
	);
}
