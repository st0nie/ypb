# YPB Architecture

[← Back to Usage](../README.md) · [Installation & Configuration](install.md)
## Overview

`ypb` (Yet another PasteBin) is an HTTP pastebin and URL shortener service built on Rust, Tokio, and Axum. Files are stored as `<4-char-hash>.txt` inside `--file-path` (default `./files`) and periodically cleaned up by a background task.

## Module Structure

- `src/main.rs`:
  - `app(Arc<AppState>) -> Router`: Builds the Axum router with `TraceLayer`, `TimeoutLayer` (10s), and `DefaultBodyLimit` (`--limit-size`).
  - `main()`: Initializes `tracing_subscriber`, parses CLI arguments (`Args`), ensures the storage directory exists, spawns the background cleaner task, and runs the HTTP server with graceful shutdown (`SIGINT` / `SIGTERM`).
- `src/util.rs`: Shared `AppState` holding `Arc<Args>`.
- `src/util/args.rs`: CLI flags (`--port`, `--file-path`, `--clean-period`, `--limit-size`, `--syntax-theme`, `--base-url`) and validation/normalization for `--base-url`.
- `src/util/base_url.rs`: `BaseUrl` Axum request extractor. Uses `--base-url` when configured; otherwise derives `<scheme>://<host>` from `X-Forwarded-Proto` (defaulting to `http`) and the `Host` header.
- `src/util/cleaner.rs`: Background Tokio interval task (`cleaner_task`) that removes expired files from the storage directory based on modification timestamp and file size.
- `src/util/handler.rs`: HTTP route handlers and HTML rendering (`index_handler`, `get_handler`, `put_handler`, `delete_handler`, `markdown_page`, `highlighted_page`).

## Request Flows

### 1. Index & Documentation Pages (`GET /`, `GET /README.md`, `GET /docs/{doc}`)
- `index_handler` embeds `README.md` at compile time (`include_str!("../../README.md")`) and replaces the placeholder `http://localhost:3000` origin with the resolved `BaseUrl`.
- `doc_handler` serves embedded project documentation (`docs/install.md`, `docs/architecture.md`) so links in `README.md` work both on GitHub and on the running `ypb` web instance (including inside minimal Docker images).
- Content negotiation via `Accept` (`Vary: accept`):
  - Clients accepting `text/html` (browsers) receive a GitHub-flavored Markdown HTML page rendered by `markdown_page`.
  - Terminal clients (such as `curl`) receive raw `text/plain; charset=utf-8`.

### 2. Upload (`PUT /`)
- Computes a 4-character URL-safe Base64 prefix of the `CRC-32/ISO-HDLC` checksum of the request body.
- Writes the raw bytes to `{file_path}/{hash}.txt`.
- Derives a deletion `secret` by hashing the file's modification timestamp (seconds since `UNIX_EPOCH`) with `CRC-32/ISO-HDLC`.
- Returns plain-text metadata (`url`, `short`, `size`, `secret`).

### 3. Retrieve (`GET /{hash}` and `GET /{hash}.{ext}`)
- `parse_filehash` splits `{file_hash}` into the underlying storage file `{stem}.txt` and optional extension `{ext}`.
- If the stored file is valid UTF-8:
  1. **Single-line HTTP(S) URL**: Returns `302 Temporary Redirect` to the target URL.
  2. **No extension or `.txt`**: Returns raw `text/plain; charset=utf-8` content byte-for-byte.
  3. **Markdown extension (`.md`, `.markdown`, `.mdown`, `.mkd`)**: Renders GitHub-flavored Markdown via `pulldown-cmark` (`markdown_page`), escaping raw HTML tags and sanitizing link/image URL schemes to `http`, `https`, or `mailto`.
  4. **Other extensions (e.g. `.rs`, `.lock`, `.py`)**: Renders a syntax-highlighted HTML view via `highlighted_page`.
- If the stored file is binary (non-UTF-8): Streams the file via `ReaderStream` with a MIME type guessed from `{ext}`.

### 4. Syntax-Highlighted View (`highlighted_page`)
- **Theme pairing**: `resolve_theme_pair` pairs light and dark highlight.js stylesheets via `prefers-color-scheme` media queries and syncs the document/gutter background color (`syncTheme`).
- **Extension language mapping**: `resolve_language` maps extensions not natively aliased by the highlight.js common CDN bundle:
  - `.lock`: Inspects content to map JSON lockfiles to `json`, TOML lockfiles (`Cargo.lock`, `uv.lock`, `poetry.lock`) to `toml`, and YAML lockfiles to `yaml` (defaulting to `toml`).
  - `.conf`, `.cfg`, `.service`, `.socket`, `.timer`, `.desktop`, `.gitconfig`, `.editorconfig` -> `ini`.
  - `.zsh`, `.bashrc`, `.zshrc`, `.profile`, `.env`, `.pkgbuild` -> `bash`.
  - `.jsonc`, `.jsonl`, `.ndjson`, `.geojson`, `.webmanifest` -> `json`.
  - `.htm`, `.vue`, `.svelte` -> `html`.
- **Sticky line numbers & selection isolation**:
  - Server-renders `<a id="ln-{n}" class="line-num" href="#L{n}" data-line="{n}">{n}</a>` inside a sticky left column (`position: sticky; left: 0; user-select: none`) alongside `<pre><code class="{resolved_language}">`. Using `id="ln-{n}"` (instead of `id="L{n}"`) prevents browser native fragment jumps to `top: 0` on page load and prevents focus-induced `scroll-margin` auto-scrolling when clicking lines near the top of the viewport.
  - Code remains a single contiguous `<pre><code>` block with `overflow-x: visible; width: max-content; min-width: 100%`, preserving multi-line syntax tokens and clean text selection without line numbers.
- **Line-range deep linking, deselection & context scroll**:
  - Clicking a line number selects `#L<n>`, updates the URL hash via `history.replaceState`, highlights the line number and full-width `#line-highlight` overlay, and copies the full URL to the clipboard without scrolling the viewport.
  - Holding `Shift` while clicking another line number selects a `#L<start>-L<end>` range and copies the range URL.
  - Deselection is supported by clicking the active single line number again, pressing `Escape`, or clicking the `#L... ×` clear button in the top-right toolbar, which strips the hash from the URL and clears the highlight.
  - Opening a URL with `#L<n>` or `#L<start>-L<end>` (or triggering `hashchange`) scrolls the first selected line into view via `scrollToLine` with ~28% viewport height (`>= 140px`) of upper context so preceding lines remain visible.
- **Content copy (selected range or full file)**: A floating top-right `Copy` button (`#copy-raw-btn`) copies only the selected lines (`start..=end`) when a `#L<n>` or `#L<start>-L<end>` selection is active, or the entire `<pre><code>.textContent` when no lines are selected.

### 5. Delete (`DELETE /{hash}`)
- Recomputes the expected `secret` from the file's modification timestamp and removes `{hash}.txt` if the request body matches; otherwise returns `403 Forbidden`.
