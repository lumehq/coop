import { type ClassValue, clsx } from "clsx";
import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";
import updateLocale from "dayjs/plugin/updateLocale";
import { type NostrEvent, nip19 } from "nostr-tools";
import { twMerge } from "tailwind-merge";

dayjs.extend(relativeTime);
dayjs.extend(updateLocale);

dayjs.updateLocale("en", {
	relativeTime: {
		past: "%s",
		s: "now",
		m: "1m",
		mm: "%dm",
		h: "1h",
		hh: "%dh",
		d: "1d",
		dd: "%dd",
	},
});

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}

export function npub(pubkey: string, len: number) {
	if (pubkey.length <= len) return pubkey;

	const npub = pubkey.startsWith("npub1") ? pubkey : nip19.npubEncode(pubkey);
	const separator = " ... ";

	const sepLen = separator.length;
	const charsToShow = len - sepLen;
	const frontChars = Math.ceil(charsToShow / 2);
	const backChars = Math.floor(charsToShow / 2);

	return (
		npub.substring(0, frontChars) +
		separator +
		npub.substring(npub.length - backChars)
	);
}

export function ago(time: number) {
	let formated: string;

	const now = dayjs();
	const inputTime = dayjs.unix(time);
	const diff = now.diff(inputTime, "hour");

	if (diff < 24) {
		formated = inputTime.from(now, true);
	} else {
		formated = inputTime.format("MMM DD");
	}

	return formated;
}

export function time(time: number) {
	const input = new Date(time * 1000);
	const formattedTime = input.toLocaleTimeString([], {
		hour: "2-digit",
		minute: "2-digit",
		hour12: true,
	});

	return formattedTime;
}

export function getReceivers(tags: string[][]) {
	const p = tags.map((tag) => tag[0] === "p" && tag[1]);
	return p;
}

export function getChatId(pubkey: string, tags: string[][]) {
	const id = [pubkey, tags.map((tag) => tag[0] === "p" && tag[1])].join("-");
	return id;
}

export function groupEventByDate(events: NostrEvent[]) {
	const groups = Object.groupBy(events, (event) => {
		return dayjs.unix(event.created_at).startOf("day").format("MMM DD, YYYY");
	});

	return groups;
}

export function isEmojiOnly(str: string) {
	const stringToTest = str.replace(/ /g, "");
	const emojiRegex =
		/^(?:(?:\p{RI}\p{RI}|\p{Emoji}(?:\p{Emoji_Modifier}|\u{FE0F}\u{20E3}?|[\u{E0020}-\u{E007E}]+\u{E007F})?(?:\u{200D}\p{Emoji}(?:\p{Emoji_Modifier}|\u{FE0F}\u{20E3}?|[\u{E0020}-\u{E007E}]+\u{E007F})?)*)|[\u{1f900}-\u{1f9ff}\u{2600}-\u{26ff}\u{2700}-\u{27bf}])+$/u;
	return emojiRegex.test(stringToTest) && Number.isNaN(Number(stringToTest));
}