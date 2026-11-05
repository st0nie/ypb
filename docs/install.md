# Installation & Configuration

[← Back to Usage](../README.md) · [Architecture](architecture.md)

## Installation

### Arch Linux

`ypb` is available on [AUR](https://aur.archlinux.org/packages/ypb) and [archlinuxcn](https://github.com/archlinuxcn/repo/blob/master/archlinuxcn/ypb/PKGBUILD).

#### AUR

```bash
yay -S ypb # For yay
paru -S ypb # For paru
```

#### archlinuxcn

```bash
sudo pacman -S ypb
```

### Building from Source

1. Ensure [Rust](https://www.rust-lang.org/) is installed.
2. Clone the repository:
   ```bash
   git clone https://github.com/st0nie/ypb.git
   cd ypb
   ```
3. Build and run the service:
   ```bash
   cargo run --release
   ```

## Configuration Parameters

Configure the service using command-line arguments:

- `--port`: The port to listen on (default: 3000).
- `--file-path`: Directory for file storage (default: `./files`).
- `--clean-period`: Period to check for expired files (in seconds, default: 3600).
- `--limit-size`: File size limit (in bytes).
- `--syntax-theme`: Syntax highlight theme (highlight.js).
- `--base-url`: Public base URL used in returned links and on the index page, e.g. `https://pb.example.com`. When unset, it is derived from the request's `X-Forwarded-Proto` header (default `http`) and `Host` header, so a reverse proxy should forward the original `Host` (including a non-default port).

Example:
```bash
cargo run -- --port 8080 --base-url https://pb.example.com
```
