import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { type ReactNode, createContext, useContext } from "react";

type Metadata = {
	name?: string;
	display_name?: string;
	about?: string;
	website?: string;
	picture?: string;
	banner?: string;
	nip05?: string;
	lud06?: string;
	lud16?: string;
};

type UserContext = {
	pubkey: string;
	isLoading: boolean;
	isError: boolean;
	profile: Metadata | undefined;
};

const UserContext = createContext<UserContext>(null);

export function UserProvider({
	pubkey,
	children,
}: {
	pubkey: string;
	children: ReactNode;
}) {
	const {
		isLoading,
		isError,
		data: profile,
	} = useQuery({
		queryKey: ["profile", pubkey],
		queryFn: async () => {
			try {
				const normalizePubkey = pubkey
					.replace("nostr:", "")
					.replace(/[^\w\s]/gi, "");

				const query: string = await invoke("get_metadata", {
					id: normalizePubkey,
				});

				return JSON.parse(query) as Metadata;
			} catch (e) {
				throw new Error(String(e));
			}
		},
		refetchOnMount: false,
		refetchOnWindowFocus: false,
		refetchOnReconnect: false,
		staleTime: Number.POSITIVE_INFINITY,
		retry: 2,
	});

	return (
		<UserContext.Provider value={{ pubkey, profile, isError, isLoading }}>
			{children}
		</UserContext.Provider>
	);
}

export function useUserContext() {
	const context = useContext(UserContext);
	return context;
}
