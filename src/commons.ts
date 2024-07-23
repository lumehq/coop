import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}

export function npub(pubkey: string, len: number) {
	if (pubkey.length <= len) return pubkey;

	const separator = " ... ";

	const sepLen = separator.length;
	const charsToShow = len - sepLen;
	const frontChars = Math.ceil(charsToShow / 2);
	const backChars = Math.floor(charsToShow / 2);

	return (
		pubkey.substring(0, frontChars) +
		separator +
		pubkey.substring(pubkey.length - backChars)
	);
}
