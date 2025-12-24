//! Simple test to verify Catppuccin theme loading
//!
//! This test loads theme files directly using absolute paths.

use std::env;
use std::path::PathBuf;

use theme::ThemeFamily;

fn main() -> anyhow::Result<()> {
    println!("Simple theme loading test\n");

    // Get the current directory
    let current_dir = env::current_dir()?;
    println!("Current directory: {}", current_dir.display());

    // Try to find the project root
    // Look for workspace Cargo.toml (the one with [workspace] section)
    let mut project_root = current_dir.clone();
    let mut found = false;

    for _ in 0..5 {
        let cargo_toml = project_root.join("Cargo.toml");
        if cargo_toml.exists() {
            // Check if this is a workspace Cargo.toml
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    found = true;
                    break;
                }
            }
        }
        if !project_root.pop() {
            break;
        }
    }

    if !found {
        println!("Could not find project root (Cargo.toml)");
        return Ok(());
    }

    println!("Project root: {}\n", project_root.display());

    // Test loading latte theme
    let theme_path = project_root.join("assets/themes/catppuccin-latte.json");
    println!("Testing latte theme:");
    println!("  Path: {}", theme_path.display());

    if !theme_path.exists() {
        println!("  ✗ Theme file does not exist!");
        // List directory contents
        let themes_dir = project_root.join("assets/themes");
        if themes_dir.exists() {
            println!("  Contents of {}:", themes_dir.display());
            if let Ok(entries) = std::fs::read_dir(&themes_dir) {
                for entry in entries.flatten() {
                    println!("    - {}", entry.file_name().to_string_lossy());
                }
            }
        } else {
            println!("  Themes directory does not exist!");
        }
        return Ok(());
    }

    // Try to load the theme
    match ThemeFamily::from_file(&theme_path) {
        Ok(theme) => {
            println!("  ✓ Successfully loaded!");
            println!("    ID: {}", theme.id);
            println!("    Name: {}", theme.name);
            println!("    Light background: {}", theme.light().background);
            println!("    Dark background: {}", theme.dark().background);

            // Test a few more colors
            println!("    Text color: {}", theme.light().text);
            println!("    Accent color: {}", theme.light().text_accent);
            println!(
                "    Element background: {}",
                theme.light().element_background
            );
        }
        Err(e) => {
            println!("  ✗ Failed to load: {}", e);
            // Try to read the file to see what's wrong
            if let Ok(content) = std::fs::read_to_string(&theme_path) {
                println!("  File content (first 500 chars):");
                println!("{}", &content[..content.len().min(500)]);
            }
        }
    }

    Ok(())
}
