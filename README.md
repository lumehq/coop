![Coop](/docs/coop.png)

<p>
    <a href="https://github.com/lumehq/coop/actions/workflows/rust.yml">
      <img alt="Actions" src="https://github.com/lumehq/coop/actions/workflows/rust.yml/badge.svg">
    </a>
    <img alt="GitHub repo size" src="https://img.shields.io/github/repo-size/lumehq/coop">
    <img alt="GitHub issues" src="https://img.shields.io/github/issues-raw/lumehq/coop">
    <img alt="GitHub pull requests" src="https://img.shields.io/github/issues-pr/lumehq/coop">
</p>

Coop is a simple, fast, and reliable nostr client for secure messaging across all platforms.

### Screenshots

<p float="left">
  <img src="/docs/mac_01.png" width="250" />
  <img src="/docs/mac_02.png" width="250" />
  <img src="/docs/mac_03.png" width="250" />
  <img src="/docs/mac_04.png" width="250" />
  <img src="/docs/mac_05.png" width="250" />
  <img src="/docs/mac_06.png" width="250" />
  <img src="/docs/mac_07.png" width="250" />
  <img src="/docs/mac_08.png" width="250" />
  <img src="/docs/mac_09.png" width="250" />
  <img src="/docs/linux_01.png" width="250" />
  <img src="/docs/linux_02.png" width="250" />
  <img src="/docs/linux_03.png" width="250" />
  <img src="/docs/linux_04.png" width="250" />
  <img src="/docs/linux_05.png" width="250" />
</p>

### Installation

To install Coop, follow these steps:

1. **Download the Latest Release**:

   - Visit the [Coop Releases page on GitHub](https://github.com/lumehq/coop/releases).
   - Download the package that matches your operating system (Windows, macOS, or Linux).

2. **Install**:

   - **Windows**: Run the downloaded `.exe` installer and follow the on-screen instructions.
   - **macOS**: Open the downloaded `.dmg` file and drag Coop to your Applications folder.
   - **Linux**: Run the downloaded `.flatpak` or `.snap` installer and follow the on-screen instructions.

3. **Run Coop**:
   - Launch Coop from your Applications folder (macOS) or by double-clicking the executable (Windows/Linux).

For more detailed instructions, refer to the [Release Notes](#) on GitHub.

### Developing Coop

Coop is built using Rust and GPUI. All Nostr related stuffs handled by [Rust Nostr SDK](https://github.com/rust-nostr/nostr)

#### Prerequisites

- **Rust Toolchain**: Ensure you have Rust installed. If not, you can install it using [rustup](https://rustup.rs/).
- **Cargo**: Rust's package manager, which comes bundled with the Rust installation.
- **Git**: To clone the repository and manage version control.

#### Setting Up the Development Environment

1. Clone the repository:

   ```bash
   git clone https://github.com/lumehq/coop.git
   cd coop
   ```

2.1 Install Linux dependencies:

   ```bash
   ./script/linux
   ```

2.2 Install FreeBSD dependencies:

   ```bash
   ./script/freebsd
   ```

3. Install Rust dependencies:

   ```bash
   cargo build
   ```

4. Run the app:
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
