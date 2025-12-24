//! Comprehensive test for all Catppuccin themes
//!
//! This test loads and validates all Catppuccin theme files,
//! ensuring they can be parsed correctly and have valid color values.

use std::env;
use std::path::Path;

use theme::ThemeFamily;

/// Find the project root by looking for the workspace Cargo.toml
fn find_project_root() -> anyhow::Result<std::path::PathBuf> {
    let mut current_dir = env::current_dir()?;

    // Look for workspace Cargo.toml (the one with [workspace] section)
    for _ in 0..5 {
        let cargo_toml = current_dir.join("Cargo.toml");
        if cargo_toml.exists() {
            // Check if this is a workspace Cargo.toml
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    return Ok(current_dir);
                }
            }
        }
        if !current_dir.pop() {
            break;
        }
    }

    Err(anyhow::anyhow!(
        "Could not find workspace root (Cargo.toml with [workspace] section)"
    ))
}

/// Test loading a single theme file
fn test_theme_file(theme_path: &Path) -> anyhow::Result<ThemeFamily> {
    if !theme_path.exists() {
        return Err(anyhow::anyhow!(
            "Theme file does not exist: {}",
            theme_path.display()
        ));
    }

    let theme = ThemeFamily::from_file(theme_path)?;

    // Basic validation
    if theme.id.is_empty() {
        return Err(anyhow::anyhow!("Theme ID is empty"));
    }

    if theme.name.is_empty() {
        return Err(anyhow::anyhow!("Theme name is empty"));
    }

    Ok(theme)
}

/// Run comprehensive tests on all Catppuccin themes
fn main() -> anyhow::Result<()> {
    println!("Comprehensive Catppuccin Theme Test");
    println!("====================================\n");

    // Find project root
    let project_root = find_project_root()?;
    println!("Project root: {}\n", project_root.display());

    // Define all Catppuccin flavors
    let flavors = ["latte", "frappe", "macchiato", "mocha"];

    let themes_dir = project_root.join("assets/themes");
    if !themes_dir.exists() {
        return Err(anyhow::anyhow!(
            "Themes directory not found: {}",
            themes_dir.display()
        ));
    }

    println!("Themes directory: {}\n", themes_dir.display());

    let mut all_passed = true;
    let mut test_results = Vec::new();

    // Test each flavor
    for flavor in flavors {
        let theme_name = format!("catppuccin-{}", flavor);
        let theme_path = themes_dir.join(format!("{}.json", theme_name));

        println!("Testing {} theme:", theme_name);
        println!("  Path: {}", theme_path.display());

        match test_theme_file(&theme_path) {
            Ok(theme) => {
                println!("  ✓ PASS: Successfully loaded");
                println!("    ID: {}", theme.id);
                println!("    Name: {}", theme.name);

                // Verify theme ID matches filename
                if theme.id != theme_name {
                    println!(
                        "    WARNING: Theme ID '{}' doesn't match filename '{}'",
                        theme.id, theme_name
                    );
                }

                // Check some key colors
                println!("    Light mode:");
                println!("      Background: {}", theme.light().background);
                println!("      Text: {}", theme.light().text);
                println!("      Accent: {}", theme.light().text_accent);

                println!("    Dark mode:");
                println!("      Background: {}", theme.dark().background);
                println!("      Text: {}", theme.dark().text);
                println!("      Accent: {}", theme.dark().text_accent);

                test_results.push((flavor, true, None));
            }
            Err(e) => {
                println!("  ✗ FAIL: {}", e);
                test_results.push((flavor, false, Some(e.to_string())));
                all_passed = false;
            }
        }
        println!();
    }

    // Test from_assets() function
    println!("Testing from_assets() function:");
    println!("===============================\n");

    // Change to project root to test from_assets
    let original_dir = env::current_dir()?;
    if env::set_current_dir(&project_root).is_ok() {
        for flavor in flavors {
            let theme_name = format!("catppuccin-{}", flavor);
            println!("Testing {} via from_assets():", theme_name);

            match ThemeFamily::from_assets(&theme_name) {
                Ok(theme) => {
                    println!("  ✓ PASS: Successfully loaded via from_assets()");
                    println!("    ID: {}", theme.id);

                    // Verify it's the same theme
                    if theme.id != theme_name {
                        println!(
                            "    WARNING: Theme ID '{}' doesn't match expected '{}'",
                            theme.id, theme_name
                        );
                    }
                }
                Err(e) => {
                    println!("  ✗ FAIL: {}", e);
                    all_passed = false;
                }
            }
        }

        // Change back to original directory
        let _ = env::set_current_dir(&original_dir);
    } else {
        println!("  Could not change to project root directory");
        all_passed = false;
    }

    println!("\nTest Summary");
    println!("============\n");

    println!("Flavor       Status  Details");
    println!("------       ------  -------");

    for (flavor, passed, error) in &test_results {
        let status = if *passed { "✓ PASS" } else { "✗ FAIL" };
        let details = error.as_deref().unwrap_or("");
        println!("{:<12} {:<7} {}", flavor, status, details);
    }

    println!("\nTotal tests: {}", test_results.len());
    println!(
        "Passed: {}",
        test_results.iter().filter(|(_, passed, _)| *passed).count()
    );
    println!(
        "Failed: {}",
        test_results
            .iter()
            .filter(|(_, passed, _)| !*passed)
            .count()
    );

    if all_passed {
        println!("\n🎉 All tests passed!");
        Ok(())
    } else {
        println!("\n❌ Some tests failed.");
        Err(anyhow::anyhow!("One or more theme tests failed"))
    }
}
