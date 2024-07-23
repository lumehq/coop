import { cn } from "@/commons";
import { useUserContext } from "./provider";

export function UserName({ className }: { className?: string }) {
	const user = useUserContext();

	return (
		<div className={cn("max-w-[12rem] truncate", className)}>
			{user.profile?.display_name || user.profile?.name || "Anon"}
		</div>
	);
}
