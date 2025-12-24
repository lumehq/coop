#!/usr/bin/env python3
"""
Validate generated Catppuccin theme files.

This script validates that the generated theme files:
1. Are valid JSON
2. Have the correct structure for ThemeFamily
3. Have valid hex color values
4. Can be loaded by the theme system
"""

import json
import os
import sys
from typing import Any, Dict, List


def validate_hex_color(color: str) -> bool:
    """Validate that a string is a valid hex color."""
    if not color.startswith("#"):
        return False

    hex_part = color[1:]  # Remove #

    # Check valid hex characters
    if not all(c in "0123456789abcdefABCDEF" for c in hex_part):
        return False

    # Check length (3, 4, 6, or 8 characters)
    if len(hex_part) not in [3, 4, 6, 8]:
        return False

    return True


def validate_theme_colors(colors: Dict[str, str], theme_name: str) -> List[str]:
    """Validate a ThemeColors object."""
    errors = []

    # Required color fields from ThemeColors struct
    required_fields = [
        # Surface colors
        "background",
        "surface_background",
        "elevated_surface_background",
        "panel_background",
        "overlay",
        "title_bar",
        "title_bar_inactive",
        "window_border",
        # Border colors
        "border",
        "border_variant",
        "border_focused",
        "border_selected",
        "border_transparent",
        "border_disabled",
        "ring",
        # Text colors
        "text",
        "text_muted",
        "text_placeholder",
        "text_accent",
        # Icon colors
        "icon",
        "icon_muted",
        "icon_accent",
        # Element colors
        "element_foreground",
        "element_background",
        "element_hover",
        "element_active",
        "element_selected",
        "element_disabled",
        # Secondary element colors
        "secondary_foreground",
        "secondary_background",
        "secondary_hover",
        "secondary_active",
        "secondary_selected",
        "secondary_disabled",
        # Danger element colors
        "danger_foreground",
        "danger_background",
        "danger_hover",
        "danger_active",
        "danger_selected",
        "danger_disabled",
        # Warning element colors
        "warning_foreground",
        "warning_background",
        "warning_hover",
        "warning_active",
        "warning_selected",
        "warning_disabled",
        # Ghost element colors
        "ghost_element_background",
        "ghost_element_background_alt",
        "ghost_element_hover",
        "ghost_element_active",
        "ghost_element_selected",
        "ghost_element_disabled",
        # Tab colors
        "tab_inactive_background",
        "tab_hover_background",
        "tab_active_background",
        # Scrollbar colors
        "scrollbar_thumb_background",
        "scrollbar_thumb_hover_background",
        "scrollbar_thumb_border",
        "scrollbar_track_background",
        "scrollbar_track_border",
        # Interactive colors
        "drop_target_background",
        "cursor",
        "selection",
    ]

    # Check all required fields are present
    for field in required_fields:
        if field not in colors:
            errors.append(f"Missing required field: {field}")

    # Validate all color values
    for field, color_value in colors.items():
        if not validate_hex_color(color_value):
            errors.append(f"Invalid hex color in {field}: {color_value}")

    return errors


def validate_theme_file(file_path: str) -> Dict[str, Any]:
    """Validate a theme JSON file."""
    print(f"Validating {os.path.basename(file_path)}...")

    try:
        with open(file_path, "r") as f:
            data = json.load(f)
    except json.JSONDecodeError as e:
        return {"valid": False, "errors": [f"Invalid JSON: {e}"]}
    except Exception as e:
        return {"valid": False, "errors": [f"Error reading file: {e}"]}

    errors = []

    # Check required top-level fields
    required_top_fields = ["id", "name", "light", "dark"]
    for field in required_top_fields:
        if field not in data:
            errors.append(f"Missing required top-level field: {field}")

    if errors:
        return {"valid": False, "errors": errors}

    # Validate light theme colors
    light_errors = validate_theme_colors(data["light"], f"{data['id']} (light)")
    if light_errors:
        errors.extend([f"Light theme: {e}" for e in light_errors])

    # Validate dark theme colors
    dark_errors = validate_theme_colors(data["dark"], f"{data['id']} (dark)")
    if dark_errors:
        errors.extend([f"Dark theme: {e}" for e in dark_errors])

    return {
        "valid": len(errors) == 0,
        "id": data["id"],
        "name": data["name"],
        "errors": errors,
    }


def main():
    """Validate all Catppuccin theme files."""
    themes_dir = "assets/themes"

    if not os.path.exists(themes_dir):
        print(f"Error: Themes directory not found: {themes_dir}")
        sys.exit(1)

    # Find all Catppuccin theme files
    theme_files = []
    for filename in os.listdir(themes_dir):
        if filename.startswith("catppuccin-") and filename.endswith(".json"):
            theme_files.append(os.path.join(themes_dir, filename))

    if not theme_files:
        print("No Catppuccin theme files found!")
        sys.exit(1)

    print(f"Found {len(theme_files)} Catppuccin theme files:")
    for file in theme_files:
        print(f"  - {os.path.basename(file)}")
    print()

    # Validate each file
    results = []
    all_valid = True

    for file_path in theme_files:
        result = validate_theme_file(file_path)
        results.append(result)

        if result["valid"]:
            print(f"  ✓ {os.path.basename(file_path)}: VALID")
            print(f"    ID: {result['id']}, Name: {result['name']}")
        else:
            print(f"  ✗ {os.path.basename(file_path)}: INVALID")
            for error in result["errors"]:
                print(f"    - {error}")
            all_valid = False
        print()

    # Summary
    print("=" * 60)
    print("VALIDATION SUMMARY")
    print("=" * 60)

    valid_count = sum(1 for r in results if r["valid"])
    invalid_count = len(results) - valid_count

    print(f"Total files: {len(results)}")
    print(f"Valid: {valid_count}")
    print(f"Invalid: {invalid_count}")

    if all_valid:
        print("\n🎉 All theme files are valid!")
        sys.exit(0)
    else:
        print("\n❌ Some theme files have validation errors.")
        sys.exit(1)


if __name__ == "__main__":
    main()
