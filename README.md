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
   git clone <repository-url>
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

Example:
```bash
cargo run -- --port 8080
```

### API Endpoints

#### Upload File
- **Method**: `PUT`
- **Path**: `/`
- **Description**: Upload a file.

curl Example:
```bash
$ echo "1232" | curl -X PUT --data-binary @- "localhost:3000/"
url: http://localhost:3000/coBF
size: 4 bytes
secret: 1745900203
```

#### Retrieve File
- **Method**: `GET`
- **Path**: `/{file_hash}`
- **Description**: Retrieve file content by its hash.

curl Example:
```bash
$ curl http://localhost:3000/coBF
```

#### Delete File
- **Method**: `DELETE`
- **Path**: `/{file_hash}`
- **Description**: Deletes a file by its hash.

curl Example
```bash
$ echo "1745900203" | curl -X DELETE --data @- http://localhost:3000/coBF
File coBF deleted successfully.
```
