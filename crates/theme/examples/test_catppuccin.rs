//! Test program to verify Catppuccin themes load correctly
//!
//! This example demonstrates loading all Catppuccin theme files
//! and verifying they can be parsed successfully.

use std::env;
use std::path::PathBuf;

use theme::ThemeFamily;

fn main() -> anyhow::Result<()> {
    println!("Testing Catppuccin theme loading...\n");

    // Get the project root directory
    // When running from cargo, we can use CARGO_MANIFEST_DIR
    let project_root = env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // Fallback: assume we're in crates/theme directory
            let mut path = env::current_dir().unwrap();
            path.push("../../../"); // Go up to project root (crates/theme -> crates -> coop)
            path.canonicalize().unwrap()
        });

    println!("Project root: {}\n", project_root.display());

    // List of Catppuccin flavors to test
    let flavors = ["latte", "frappe", "macchiato", "mocha"];

    for flavor in flavors {
        let theme_name = format!("catppuccin-{}", flavor);
        let theme_path = project_root
            .join("assets/themes")
            .join(format!("{}.json", theme_name));

        println!("Testing {} theme:", theme_name);
        println!("  Path: {}", theme_path.display());

        // Check if file exists
        if !theme_path.exists() {
            println!("  ✗ Theme file does not exist!");
            println!();
            continue;
        }

        // Try to load the theme
        match ThemeFamily::from_file(&theme_path) {
            Ok(theme) => {
                println!("  ✓ Successfully loaded!");
                println!("    ID: {}", theme.id);
                println!("    Name: {}", theme.name);
                println!("    Light background: {}", theme.light().background);
                println!("    Dark background: {}", theme.dark().background);

                // Verify the theme has the expected ID
                if theme.id != theme_name {
                    println!(
                        "    Warning: Theme ID '{}' doesn't match expected '{}'",
                        theme.id, theme_name
                    );
                }
            }
            Err(e) => {
                println!("  ✗ Failed to load: {}", e);
                // Don't print full error details to avoid clutter
            }
        }
        println!();
    }

    println!("All Catppuccin themes tested!");

    // Also test loading via from_assets when running from project root
    println!("\nTesting from_assets() function (requires running from project root):");

    // Change to project root directory
    if env::set_current_dir(&project_root).is_ok() {
        for flavor in flavors {
            let theme_name = format!("catppuccin-{}", flavor);
            println!("Testing {} via from_assets():", theme_name);

            match ThemeFamily::from_assets(&theme_name) {
                Ok(theme) => {
                    println!("  ✓ Successfully loaded via from_assets()!");
                    println!("    ID: {}", theme.id);
                }
                Err(e) => {
                    println!("  ✗ Failed to load via from_assets(): {}", e);
                }
            }
        }
    } else {
        println!("  Could not change to project root directory");
    }

    Ok(())
}
