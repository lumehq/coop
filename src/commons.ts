import { ask, message, open } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import {
	isPermissionGranted,
	requestPermission,
} from "@tauri-apps/plugin-notification";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
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

export function groupEventByDate(events: NostrEvent[]) {
	const groups = Object.groupBy(events, (event) => {
		return dayjs.unix(event.created_at).startOf("day").format("MMM DD, YYYY");
	});

	return groups;
}

export async function checkPermission() {
	if (!(await isPermissionGranted())) {
		return (await requestPermission()) === "granted";
	}
	return true;
}

export async function checkForAppUpdates(silent: boolean) {
	try {
		const update = await check();

		if (!update) {
			if (silent) return;

			await message("You are on the latest version. Stay awesome!", {
				title: "No Update Available",
				kind: "info",
				okLabel: "OK",
			});

			return;
		}

		if (update?.available) {
			const yes = await ask(
				`Update to ${update.version} is available!\n\nRelease notes: ${update.body}`,
				{
					title: "Update Available",
					kind: "info",
					okLabel: "Update",
					cancelLabel: "Cancel",
				},
			);

			if (yes) {
				await update.downloadAndInstall();
				await relaunch();
			}

			return;
		}
	} catch {
		return;
	}
}

export async function upload() {
	const allowExts = [
		"png",
		"jpeg",
		"jpg",
		"gif",
		"mp4",
		"mp3",
		"webm",
		"mkv",
		"avi",
		"mov",
	];

	const selectedPath = await open({
		multiple: false,
		filters: [
			{
				name: "Media",
				extensions: allowExts,
			},
		],
	});

	// User cancelled action
	if (!selectedPath) return null;

	try {
		const file = await readFile(selectedPath);
		const blob = new Blob([file]);

		const data = new FormData();
		data.append("fileToUpload", blob);
		data.append("submit", "Upload Image");

		const res = await fetch("https://nostr.build/api/v2/upload/files", {
			method: "POST",
			body: data,
		});

		if (!res.ok) return null;

		const json = await res.json();
		const content = json.data[0];

		return content.url as string;
	} catch (e) {
		return null;
	}
}
