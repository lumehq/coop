import { type Metadata, useProfile } from "@/hooks/useProfile";
import { type ReactNode, createContext, useContext } from "react";

type UserContext = {
	pubkey: string;
	profile: Metadata | undefined;
	isLoading: boolean;
};

const UserContext = createContext<UserContext | null>(null);

export function UserProvider({
	pubkey,
	children,
}: {
	pubkey: string;
	children: ReactNode;
}) {
	const { isLoading, profile } = useProfile(pubkey);

	return (
		<UserContext.Provider value={{ pubkey, profile, isLoading }}>
			{children}
		</UserContext.Provider>
	);
}

export function useUserContext() {
	const context = useContext(UserContext);
	return context;
}
