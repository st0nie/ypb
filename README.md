# YPB

YPB (Yet another PasteBin) is a Rust-based temporary Pastebin service and URL shorter. It supports file uploads and periodically cleans up expired files, making it ideal for short-term file sharing needs.

## Features

- **File Upload**: Upload files via HTTP interface.
- **File Cleanup**: Periodically check and delete expired files.

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

## Usage

### Running the Service

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

### Configuration Parameters

Configure the service using command-line arguments:

- `--port`: The port to listen on (default: 3000).
- `--file-path`: Directory for file storage (default: `./files`).
- `--clean-period`: Period to check for expired files (in seconds, default: 3600).
- `--limit-size`: File size limit (in bytes).
- `--syntax-theme`: Syntax highlight theme (highlight.js)
- `--base-url`: Public base URL used in returned links and on the index page, e.g. `https://pb.example.com`. When unset, it is derived from the request's `X-Forwarded-Proto` header (default `http`) and `Host` header, so a reverse proxy should forward the original `Host` (including a non-default port).

Example:
```bash
cargo run -- --port 8080 --base-url https://pb.example.com
```

### API Endpoints

The examples below use the base URL of the server that serves this page.

#### Index
- **Method**: `GET`
- **Path**: `/`
- **Description**: This document. Browsers (clients sending `Accept: text/html`) get a rendered Markdown page; other clients get plain text.

#### Upload File
- **Method**: `PUT`
- **Path**: `/`
- **Description**: Upload a file.

curl Example:
```bash
$ echo "1232" | curl -X PUT --data-binary @- "http://localhost:3000/"
url: http://localhost:3000/K8mw
short: K8mw
size: 5 bytes
secret: 2505854602
```

#### Retrieve File
- **Method**: `GET`
- **Path**: `/{file_hash}` or `/{file_hash}.{ext}`
- **Description**: Retrieve file content by its hash. Without an extension (or with `.txt`) the raw content is returned; with `.md` or `.markdown` it is rendered as Markdown HTML; with another extension such as `.rs` it is shown as a syntax-highlighted page. Content that is a single URL redirects to that URL.

curl Example:
```bash
$ curl http://localhost:3000/K8mw
```

Browser Example: `http://localhost:3000/K8mw.md`

#### Delete File
- **Method**: `DELETE`
- **Path**: `/{file_hash}`
- **Description**: Deletes a file by its hash.

curl Example
```bash
$ echo "2505854602" | curl -X DELETE --data @- http://localhost:3000/K8mw
File K8mw deleted successfully
```
