pub static PLUS_ICON: &[u8] = include_bytes!("../assets/plus.svg");
pub static GRID_ICON: &[u8] = include_bytes!("../assets/grid.svg");
pub static ARROW_UP_ICON: &[u8] = include_bytes!("../assets/arrow_up.svg");
pub static LOADER_ICON: &[u8] = include_bytes!("../assets/loader.svg");

pub struct Sizes<'a> {
	pub xs: &'a str,
	pub sm: &'a str,
	pub base: &'a str,
	pub lg: &'a str,
	pub xl: &'a str,
}

pub const SIZES: Sizes<'static> = Sizes {
	xs: "4",
	sm: "6",
	base: "8",
	lg: "12",
	xl: "16",
};

pub struct Smoothing<'a> {
	pub base: &'a str,
}

pub const SMOOTHING: Smoothing<'static> = Smoothing {
	base: "60%",
};

pub struct Colors<'a> {
	pub neutral_100: &'a str,
	pub neutral_200: &'a str,
	pub neutral_250: &'a str,
	pub neutral_300: &'a str,
	pub neutral_400: &'a str,
	pub neutral_500: &'a str,
	pub neutral_600: &'a str,
	pub neutral_700: &'a str,
	pub neutral_800: &'a str,
	pub neutral_900: &'a str,
	pub neutral_950: &'a str,
	pub blue_300: &'a str,
	pub blue_500: &'a str,
	pub white: &'a str,
	pub black: &'a str,
}

pub const COLORS: Colors<'static> = Colors {
	neutral_100: "#F3F3F3",
	neutral_200: "#E4E4E4",
	neutral_250: "#DADADA",
	neutral_300: "#CFCFCF",
	neutral_400: "#BABABA",
	neutral_500: "#A6A6A6",
	neutral_600: "#838383",
	neutral_700: "#6A6A6A",
	neutral_800: "#4B4B4B",
	neutral_900: "#2E2E2E",
	neutral_950: "#141414",
	blue_300: "#93c5fd",
	blue_500: "#3b82f6",
	white: "#FFFFFF",
	black: "#000000",
};