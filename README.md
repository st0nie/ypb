# YPB

YPB (Yet another PasteBin) is a Rust-based temporary Pastebin service and URL shorter. It supports file uploads and periodically cleans up expired files, making it ideal for short-term file sharing needs.

## Features

- **File Upload**: Upload files via HTTP interface.
- **File Cleanup**: Periodically check and delete expired files.

## Documentation

- [Installation & Configuration](docs/install.md)
- [Architecture](docs/architecture.md)

## Usage

The examples below use the base URL of the server that serves this page.

### Index
- **Method**: `GET`
- **Path**: `/`
- **Description**: This document. Browsers (clients sending `Accept: text/html`) get a rendered Markdown page; other clients get plain text.

### Upload File
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

### Retrieve File
- **Method**: `GET`
- **Path**: `/{file_hash}` or `/{file_hash}.{ext}`
- **Description**: Retrieve file content by its hash. Without an extension (or with `.txt`) the raw content is returned; with `.md` or `.markdown` it is rendered as Markdown HTML; with another extension such as `.rs` it is shown as a syntax-highlighted page (supports `#L10` and `#L10-L20` line links). Content that is a single URL redirects to that URL.

curl Example:
```bash
$ curl http://localhost:3000/K8mw
```

Browser Example: `http://localhost:3000/K8mw.md`

### Delete File
- **Method**: `DELETE`
- **Path**: `/{file_hash}`
- **Description**: Deletes a file by its hash.

curl Example
```bash
$ echo "2505854602" | curl -X DELETE --data @- http://localhost:3000/K8mw
File K8mw deleted successfully
```
