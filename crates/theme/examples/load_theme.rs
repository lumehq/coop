//! Example showing how to load a theme from a JSON file in the assets/themes directory.
//!
//! This example demonstrates the usage of `ThemeFamily::from_assets()` function
//! to load a theme from a JSON file.

use theme::ThemeFamily;

fn main() -> anyhow::Result<()> {
    // Load a theme from the assets/themes directory
    // The file should be named "example-theme.json" in the assets/themes directory
    let theme = ThemeFamily::from_assets("example-theme")?;

    println!("Successfully loaded theme:");
    println!("  ID: {}", theme.id);
    println!("  Name: {}", theme.name);
    println!("  Light theme colors loaded: {}", theme.light().background);
    println!("  Dark theme colors loaded: {}", theme.dark().background);

    // You can also load a theme from an arbitrary path
    let theme_from_path = ThemeFamily::from_file("assets/themes/example-theme.json")?;

    println!("\nAlso loaded theme from specific path:");
    println!("  ID: {}", theme_from_path.id);
    println!("  Name: {}", theme_from_path.name);

    Ok(())
}
