import { cn } from "@/commons";

export function Spinner({
	className,
}: {
	className?: string;
}) {
	return (
		<span className={cn("block relative opacity-65 size-4", className)}>
			<span className="spinner-leaf" />
			<span className="spinner-leaf" />
			<span className="spinner-leaf" />
			<span className="spinner-leaf" />
			<span className="spinner-leaf" />
			<span className="spinner-leaf" />
			<span className="spinner-leaf" />
			<span className="spinner-leaf" />
		</span>
	);
}
