import { cn, npub } from "@/commons";
import { useUserContext } from "./provider";

export function UserName({
	className,
	prefix,
}: {
	className?: string;
	prefix?: string;
}) {
	const user = useUserContext();

	return (
		<div className={cn("max-w-[12rem] truncate", className)}>
			{prefix}
			{user.profile?.display_name ||
				user.profile?.name ||
				npub(user.pubkey, 16)}
		</div>
	);
}
