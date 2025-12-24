#!/usr/bin/env python3
"""
Generate Catppuccin theme files for the Coop application.

This script generates JSON theme files for all four Catppuccin flavors:
- Latte (light)
- Frappé (dark)
- Macchiato (dark)
- Mocha (dark)

Each theme file will be saved to assets/themes/ directory.
"""

import json
import os
from typing import Any, Dict

# Catppuccin color palettes from the website
# Format: {flavor: {color_name: hex_value}}
CATPPUCCIN_PALETTES = {
    "latte": {
        # Accent colors
        "rosewater": "#dc8a78",
        "flamingo": "#dd7878",
        "pink": "#ea76cb",
        "mauve": "#8839ef",
        "red": "#d20f39",
        "maroon": "#e64553",
        "peach": "#fe640b",
        "yellow": "#df8e1d",
        "green": "#40a02b",
        "teal": "#179299",
        "sky": "#04a5e5",
        "sapphire": "#209fb5",
        "blue": "#1e66f5",
        "lavender": "#7287fd",
        # Text colors
        "text": "#4c4f69",
        "subtext1": "#5c5f77",
        "subtext0": "#6c6f85",
        # Overlay colors
        "overlay2": "#7c7f93",
        "overlay1": "#8c8fa1",
        "overlay0": "#9ca0b0",
        # Surface colors
        "surface2": "#acb0be",
        "surface1": "#bcc0cc",
        "surface0": "#ccd0da",
        "base": "#eff1f5",
        "mantle": "#e6e9ef",
        "crust": "#dce0e8",
    },
    "frappe": {
        # Accent colors
        "rosewater": "#f2d5cf",
        "flamingo": "#eebebe",
        "pink": "#f4b8e4",
        "mauve": "#ca9ee6",
        "red": "#e78284",
        "maroon": "#ea999c",
        "peach": "#ef9f76",
        "yellow": "#e5c890",
        "green": "#a6d189",
        "teal": "#81c8be",
        "sky": "#99d1db",
        "sapphire": "#85c1dc",
        "blue": "#8caaee",
        "lavender": "#babbf1",
        # Text colors
        "text": "#c6d0f5",
        "subtext1": "#b5bfe2",
        "subtext0": "#a5adce",
        # Overlay colors
        "overlay2": "#949cbb",
        "overlay1": "#838ba7",
        "overlay0": "#737994",
        # Surface colors
        "surface2": "#626880",
        "surface1": "#51576d",
        "surface0": "#414559",
        "base": "#303446",
        "mantle": "#292c3c",
        "crust": "#232634",
    },
    "macchiato": {
        # Accent colors
        "rosewater": "#f4dbd6",
        "flamingo": "#f0c6c6",
        "pink": "#f5bde6",
        "mauve": "#c6a0f6",
        "red": "#ed8796",
        "maroon": "#ee99a0",
        "peach": "#f5a97f",
        "yellow": "#eed49f",
        "green": "#a6da95",
        "teal": "#8bd5ca",
        "sky": "#91d7e3",
        "sapphire": "#7dc4e4",
        "blue": "#8aadf4",
        "lavender": "#b7bdf8",
        # Text colors
        "text": "#cad3f5",
        "subtext1": "#b8c0e0",
        "subtext0": "#a5adcb",
        # Overlay colors
        "overlay2": "#939ab7",
        "overlay1": "#8087a2",
        "overlay0": "#6e738d",
        # Surface colors
        "surface2": "#5b6078",
        "surface1": "#494d64",
        "surface0": "#363a4f",
        "base": "#24273a",
        "mantle": "#1e2030",
        "crust": "#181926",
    },
    "mocha": {
        # Accent colors
        "rosewater": "#f5e0dc",
        "flamingo": "#f2cdcd",
        "pink": "#f5c2e7",
        "mauve": "#cba6f7",
        "red": "#f38ba8",
        "maroon": "#eba0ac",
        "peach": "#fab387",
        "yellow": "#f9e2af",
        "green": "#a6e3a1",
        "teal": "#94e2d5",
        "sky": "#89dceb",
        "sapphire": "#74c7ec",
        "blue": "#89b4fa",
        "lavender": "#b4befe",
        # Text colors
        "text": "#cdd6f4",
        "subtext1": "#bac2de",
        "subtext0": "#a6adc8",
        # Overlay colors
        "overlay2": "#9399b2",
        "overlay1": "#7f849c",
        "overlay0": "#6c7086",
        # Surface colors
        "surface2": "#585b70",
        "surface1": "#45475a",
        "surface0": "#313244",
        "base": "#1e1e2e",
        "mantle": "#181825",
        "crust": "#11111b",
    },
}


def hex_to_rgba(hex_color: str, alpha: float = 1.0) -> str:
    """Convert hex color to RGBA hex string with alpha."""
    hex_color = hex_color.lstrip("#")

    if len(hex_color) == 6:
        r = int(hex_color[0:2], 16)
        g = int(hex_color[2:4], 16)
        b = int(hex_color[4:6], 16)
    elif len(hex_color) == 8:
        r = int(hex_color[0:2], 16)
        g = int(hex_color[2:4], 16)
        b = int(hex_color[4:6], 16)
        a = int(hex_color[6:8], 16) / 255.0
        alpha = a
    else:
        raise ValueError(f"Invalid hex color: #{hex_color}")

    # Convert alpha to hex (0-255)
    alpha_hex = format(int(alpha * 255), "02x")

    return (
        f"#{hex_color}{alpha_hex}"
        if len(hex_color) == 6
        else f"#{hex_color[0:6]}{alpha_hex}"
    )


def darken_hex(hex_color: str, factor: float = 0.8) -> str:
    """Darken a hex color by a factor."""
    hex_color = hex_color.lstrip("#")

    if len(hex_color) == 6:
        r = int(hex_color[0:2], 16)
        g = int(hex_color[2:4], 16)
        b = int(hex_color[4:6], 16)
    elif len(hex_color) == 8:
        r = int(hex_color[0:2], 16)
        g = int(hex_color[2:4], 16)
        b = int(hex_color[4:6], 16)
    else:
        raise ValueError(f"Invalid hex color: #{hex_color}")

    # Darken each component
    r = int(r * factor)
    g = int(g * factor)
    b = int(b * factor)

    # Clamp to 0-255
    r = max(0, min(255, r))
    g = max(0, min(255, g))
    b = max(0, min(255, b))

    return f"#{r:02x}{g:02x}{b:02x}"


def generate_theme_colors(flavor: str, palette: Dict[str, str]) -> Dict[str, Any]:
    """Generate ThemeColors structure for a Catppuccin flavor."""

    # Helper function to get color with optional alpha
    def color(name: str, alpha: float = 1.0) -> str:
        return hex_to_rgba(palette[name], alpha)

    # Determine if this is a light theme
    is_light = flavor == "latte"

    # For light themes, element foreground should be light (base)
    # For dark themes, element foreground should be dark (text)
    element_foreground = palette["base"] if is_light else palette["text"]
    danger_foreground = palette["base"] if is_light else palette["text"]
    warning_foreground = palette["base"] if is_light else palette["text"]

    # Choose accent color - using blue as primary accent
    accent_color = palette["blue"]
    danger_color = palette["red"]
    warning_color = palette["peach"]  # Using peach for warning

    return {
        # Surface colors
        "background": palette["base"],
        "surface_background": palette["mantle"],
        "elevated_surface_background": palette["crust"],
        "panel_background": palette["base"],
        "overlay": color("overlay0", 0.1),
        "title_bar": "#00000000",  # Transparent
        "title_bar_inactive": palette["base"],
        "window_border": palette["surface2"],
        # Border colors
        "border": palette["surface2"],
        "border_variant": palette["surface1"],
        "border_focused": accent_color,
        "border_selected": accent_color,
        "border_transparent": "#00000000",  # Transparent
        "border_disabled": palette["surface0"],
        "ring": accent_color,
        # Text colors
        "text": palette["text"],
        "text_muted": palette["subtext1"],
        "text_placeholder": palette["subtext0"],
        "text_accent": accent_color,
        # Icon colors
        "icon": palette["text"],
        "icon_muted": palette["subtext1"],
        "icon_accent": accent_color,
        # Element colors (primary buttons)
        "element_foreground": element_foreground,
        "element_background": accent_color,
        "element_hover": color("blue", 0.9),
        "element_active": darken_hex(accent_color, 0.9),
        "element_selected": darken_hex(accent_color, 0.8),
        "element_disabled": color("blue", 0.3),
        # Secondary element colors
        "secondary_foreground": accent_color,
        "secondary_background": palette["surface0"],
        "secondary_hover": color("surface1", 0.1),
        "secondary_active": palette["surface1"],
        "secondary_selected": palette["surface1"],
        "secondary_disabled": color("surface0", 0.3),
        # Danger element colors
        "danger_foreground": danger_foreground,
        "danger_background": danger_color,
        "danger_hover": color("red", 0.9),
        "danger_active": darken_hex(danger_color, 0.9),
        "danger_selected": darken_hex(danger_color, 0.8),
        "danger_disabled": color("red", 0.3),
        # Warning element colors
        "warning_foreground": warning_foreground,
        "warning_background": warning_color,
        "warning_hover": color("peach", 0.9),
        "warning_active": darken_hex(warning_color, 0.9),
        "warning_selected": darken_hex(warning_color, 0.8),
        "warning_disabled": color("peach", 0.3),
        # Ghost element colors (transparent buttons)
        "ghost_element_background": "#00000000",  # Transparent
        "ghost_element_background_alt": palette["surface0"],
        "ghost_element_hover": color("overlay0", 0.1),
        "ghost_element_active": palette["surface1"],
        "ghost_element_selected": palette["surface1"],
        "ghost_element_disabled": color("overlay0", 0.05),
        # Tab colors
        "tab_inactive_background": palette["surface0"],
        "tab_hover_background": palette["surface1"],
        "tab_active_background": palette["surface2"],
        # Scrollbar colors
        "scrollbar_thumb_background": color("overlay0", 0.2),
        "scrollbar_thumb_hover_background": color("overlay0", 0.3),
        "scrollbar_thumb_border": "#00000000",  # Transparent
        "scrollbar_track_background": "#00000000",  # Transparent
        "scrollbar_track_border": palette["surface1"],
        # Interactive colors
        "drop_target_background": color("blue", 0.1),
        "cursor": palette["sky"],
        "selection": color("sky", 0.25),
    }


def generate_theme_family(flavor: str, palette: Dict[str, str]) -> Dict[str, Any]:
    """Generate a complete ThemeFamily for a Catppuccin flavor."""

    # Capitalize flavor name for display
    display_name = flavor.capitalize()

    # For latte (light theme), we need to generate both light and dark variants
    # For dark themes, we'll use the same palette for both light and dark
    # since Catppuccin only provides one palette per flavor
    if flavor == "latte":
        # For latte, we need to create a dark variant
        # We'll create a simple inverted version for the dark mode
        light_colors = generate_theme_colors(flavor, palette)

        # Create a simple dark variant by inverting some colors
        # This is a simplified approach since Catppuccin doesn't provide
        # separate dark variants for latte
        dark_palette = palette.copy()
        # Invert surface colors for dark mode
        dark_palette.update(
            {
                "base": "#1a1a1a",
                "mantle": "#1f1f1f",
                "crust": "#242424",
                "surface0": "#262626",
                "surface1": "#383838",
                "surface2": "#404040",
                "overlay0": "#ffffff1a",
                "overlay1": "#ffffff33",
                "overlay2": "#ffffff4d",
                "text": "#f2f2f2",
                "subtext1": "#b3b3b3",
                "subtext0": "#808080",
            }
        )

        dark_colors = generate_theme_colors(f"{flavor}-dark", dark_palette)
    else:
        # For dark themes, use the same palette for both light and dark
        # since they're already dark themes
        light_colors = generate_theme_colors(flavor, palette)
        dark_colors = light_colors  # Same for dark mode

    return {
        "id": f"catppuccin-{flavor}",
        "name": f"Catppuccin {display_name}",
        "light": light_colors,
        "dark": dark_colors,
    }


def main():
    """Generate all Catppuccin theme files."""

    # Create output directory if it doesn't exist
    output_dir = "assets/themes"
    os.makedirs(output_dir, exist_ok=True)

    print(f"Generating Catppuccin theme files in {output_dir}/...")

    for flavor, palette in CATPPUCCIN_PALETTES.items():
        print(f"  Generating {flavor} theme...")

        # Generate theme family
        theme_family = generate_theme_family(flavor, palette)

        # Write to file
        output_file = os.path.join(output_dir, f"catppuccin-{flavor}.json")
        with open(output_file, "w") as f:
            json.dump(theme_family, f, indent=2)

        print(f"    Saved to {output_file}")

    print("\nDone! Generated theme files:")
    for flavor in CATPPUCCIN_PALETTES.keys():
        print(f"  - assets/themes/catppuccin-{flavor}.json")


if __name__ == "__main__":
    main()
