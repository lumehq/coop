![CoopDemo](/docs/coop.jpg)

<p>
    <a href="https://github.com/lumehq/coop/actions/workflows/main.yml">
      <img alt="Actions" src="https://github.com/lumehq/coop/actions/workflows/main.yml/badge.svg">
    </a>
    <img alt="GitHub repo size" src="https://img.shields.io/github/repo-size/lumehq/coop">
    <img alt="GitHub issues" src="https://img.shields.io/github/issues-raw/lumehq/coop">
    <img alt="GitHub pull requests" src="https://img.shields.io/github/issues-pr/lumehq/coop">
</p>

Coop is a cross-platform Nostr client designed for secure communication focus on simplicity and customizability.

**New**✨: A blog post introducing Coop in details has been posted [here](#).

> Coop is currently in the **alpha stage** of development. This means the app may contain bugs, incomplete features, or unexpected behavior. We recommend using it for testing purposes only and not for critical or sensitive communications. Your feedback is invaluable in helping us improve Coop, so please report any issues or suggestions via the [GitHub Issue Tracker](https://github.com/lumehq/coop/issues). Thank you for your understanding and support!

### Installation

To install Coop, follow these steps:

1. **Download the Latest Release**:

   - Visit the [Coop Releases page on GitHub](https://github.com/lumehq/coop/releases).
   - Download the package that matches your operating system (Windows, macOS, or Linux).

2. **Install**:

   - **Windows**: Run the downloaded `.exe` installer and follow the on-screen instructions.
   - **macOS**: Open the downloaded `.dmg` file and drag Coop to your Applications folder.
   - **Ubuntu**: Run the downloaded `.deb` or `.AppImage` installer and follow the on-screen instructions.
   - **Arch Linux**: For `.tar.gz` packages, extract and install manually. For PKGBUILD, use `makepkg -si` to build and install.
   - **Flatpak**: Coming soon.

3. **Run Coop**:
   - Launch Coop from your Applications folder (macOS) or by double-clicking the executable (Windows/Linux).

For more detailed instructions, refer to the [Release Notes](#) on GitHub.

### Developing Coop

Coop is built using Rust and GPUI. All Nostr related stuffs handled by [Rust Nostr SDK](https://github.com/rust-nostr/nostr)

#### Prerequisites

- **Rust Toolchain**: Ensure you have Rust installed. If not, you can install it using [rustup](https://rustup.rs/).
- **Cargo**: Rust's package manager, which comes bundled with the Rust installation.
- **Git**: To clone the repository and manage version control.

#### Linux Ubuntu Prerequisites
- **x11**:Provides the X11 client-side library development headers and tools needed for compiling applications that use the X Window System. `sudo apt install libx11-dev libxrandr-dev libxi-dev libgl1-mesa-dev` `sudo apt install libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev`
- **build-essential**: Required to build debian files in Ubuntu. `sudo apt install build-essential pkg-config`
-**openSSL**: OpenSSL is a comprehensive open-source cryptography library and command-line tool used for secure communications and certificate management. `sudo apt install openssl` `sudo apt install libssl-dev`



#### Setting Up the Development Environment

1. Clone the repository:

   ```bash
   git clone https://github.com/lumehq/coop.git
   cd coop
   ```

2. Install dependencies:

   ```bash
   cargo build
   ```

3. Run the app:
   ```bash
   cargo run
   ```

#### Building for Production

To build Coop for production, use the following command:

```bash
cargo build --release
```

This will generate an optimized binary in the `target/release` directory.

#### Contributing Code

If you'd like to contribute to Coop, please follow these steps:

1. Fork the repository.
2. Create a new branch for your feature or bugfix.
3. Make your changes and ensure all tests pass.
4. Submit a pull request with a detailed description of your changes.

For more information, see the [Contributing](#contributing) section.

#### Debugging

To debug Coop, you can use `cargo`'s built-in debugging tools or attach a debugger like `gdb` or `lldb`. For example:

```bash
cargo run -- --debug
```

#### Additional Resources

- [Rust Nostr](https://github.com/rust-nostr/nostr/)
- [GPUI](https://www.gpui.rs/)
- [GPUI Components](https://github.com/longbridge/gpui-component/)
- [Coop Issue Tracker](https://github.com/lumehq/coop/issues/)

### License

Copyright (C) 2025 Ren Amamiya & other Coop contributors

This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.

You should have received a copy of the GNU General Public License along with this program. If not, see https://www.gnu.org/licenses/.
