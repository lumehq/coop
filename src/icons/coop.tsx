import { SVGProps } from "react";

export function CoopIcon(props: SVGProps<SVGSVGElement>) {
	return (
		<svg
			width={24}
			height={24}
			viewBox="0 0 24 24"
			fill="none"
			xmlns="http://www.w3.org/2000/svg"
			{...props}
		>
			<path
				d="M0 12C0 5.373 5.373 0 12 0c6.628 0 12 5.373 12 12 0 6.628-5.372 12-12 12H1.426A1.426 1.426 0 010 22.574V12z"
				fill="currentColor"
			/>
		</svg>
	);
}
