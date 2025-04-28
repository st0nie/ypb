# YPB

YPB (Yet another PasteBin) is a Rust-based temporary Pastebin service and URL shorter. It supports file uploads and periodically cleans up expired files, making it ideal for short-term file sharing needs.

## Features

- **File Upload**: Upload files via HTTP interface.
- **File Cleanup**: Periodically check and delete expired files.

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
$ echo "1232" | curl -X PUT --data @- "localhost:3000/"
url: http://localhost:3000/coBF
size: 4 bytes
```

#### Retrieve File
- **Method**: `GET`
- **Path**: `/{file_hash}`
- **Description**: Retrieve file content by its hash.

curl Example:
```bash
$ curl http://localhost:3000/coBF
```