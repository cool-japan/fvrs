# FVRS (File-Vision Rust Suite)

A modern, high-performance file manager written in Rust.

## Features

- Native Windows GUI with cross-platform support via Slint
- High-performance file operations with async I/O
- Plugin system for extensibility
- Shell integration for Windows
- Internationalization support

## Requirements

- Rust 1.78 or later
- Windows 10/11 x64 (Tier 1)
- Linux (Wayland/X11) or macOS 12+ (Tier 2)

## Building

```bash
# Clone the repository
git clone https://github.com/yourusername/fvrs.git
cd fvrs

# Build the project
cargo build --release

# Run tests
cargo test
```

## Project Structure

```
fvrs/
├─ crates/
│   ├─ fvrs-core/    ── Core file system operations
│   ├─ fvrs-gui-nwg/ ── Native Windows GUI
│   ├─ fvrs-gui-slint/── Cross-platform GUI
│   ├─ fvrs-shell/   ── Shell extensions
│   ├─ fvrs-plugin-api/─ Plugin API
│   └─ fvrs-plugins/ ── Official plugins
└─ tools/
    └─ ci/           ── CI scripts
```

## Contributing

Please read our [Contributing Guidelines](CONTRIBUTING.md) before submitting pull requests.

## License

This project is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option. 