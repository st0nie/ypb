use std::{path::Path as FilePath, sync::Arc, time::UNIX_EPOCH};

use axum::{
    body::{Body, Bytes},
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::typed_header::TypedHeaderRejection;
use htmlize::escape_text;
use indoc::formatdoc;
use mime_guess::mime;
use pulldown_cmark::{
    CowStr, Event, Options, Parser as MarkdownParser, Tag, html as markdown_html,
};
use tokio::{
    fs::{self, File as TokioFile},
    io::AsyncWriteExt,
};
use tokio_util::io::ReaderStream;
use tracing::info;

use super::AppState;
use super::base_url::BaseUrl;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("File not found")]
    NotFound,
    #[error("Permission denied")]
    Forbidden,
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("System time error: {0}")]
    SystemTimeError(#[from] std::time::SystemTimeError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, self.to_string()),
            AppError::IoError(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
            AppError::SystemTimeError(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };

        (status, error_message).into_response()
    }
}

fn parse_filehash(file_hash: &str) -> (String, Option<String>) {
    let file_hash = FilePath::new(file_hash);
    let file_name = format!(
        "{}.txt",
        file_hash
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
    );
    let file_ext = file_hash
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string());
    (file_name, file_ext)
}

async fn file_to_timestamp(file: &TokioFile) -> Result<String, AppError> {
    Ok(file
        .metadata()
        .await?
        .modified()?
        .duration_since(UNIX_EPOCH)?
        .as_secs()
        .to_string())
}

fn resolve_theme_pair(theme: &str) -> (&str, &str) {
    match theme {
        "vs" | "vs2015" => ("vs", "vs2015"),
        "github" | "github-dark" => ("github", "github-dark"),
        "atom-one" | "atom-one-light" | "atom-one-dark" => ("atom-one-light", "atom-one-dark"),
        "tokyo-night" | "tokyo-night-light" | "tokyo-night-dark" => {
            ("tokyo-night-light", "tokyo-night-dark")
        }
        "solarized" | "solarized-light" | "solarized-dark" => ("solarized-light", "solarized-dark"),
        "stackoverflow" | "stackoverflow-light" | "stackoverflow-dark" => {
            ("stackoverflow-light", "stackoverflow-dark")
        }
        "nnfx" | "nnfx-light" | "nnfx-dark" => ("nnfx-light", "nnfx-dark"),
        "a11y" | "a11y-light" | "a11y-dark" => ("a11y-light", "a11y-dark"),
        "panda-syntax" | "panda-syntax-light" | "panda-syntax-dark" => {
            ("panda-syntax-light", "panda-syntax-dark")
        }
        "isbl-editor" | "isbl-editor-light" | "isbl-editor-dark" => {
            ("isbl-editor-light", "isbl-editor-dark")
        }
        "qtcreator" | "qtcreator-light" | "qtcreator-dark" => ("qtcreator-light", "qtcreator-dark"),
        "base16/onedark" | "base16/one-light" => ("base16/one-light", "base16/onedark"),
        other => (other, other),
    }
}

/// Maps file extensions not natively aliased by highlight.js's common bundle
/// to a supported highlight.js language identifier.
fn resolve_language<'a>(language: &'a str, content: &str) -> &'a str {
    match language.to_ascii_lowercase().as_str() {
        "lock" => {
            let trimmed = content.trim_start();
            if trimmed.starts_with('{') {
                "json"
            } else if content.lines().any(|line| {
                let l = line.trim_start();
                l.starts_with('[') || l.contains(" = ")
            }) {
                "toml"
            } else if content.lines().any(|line| line.trim_end().ends_with(':')) {
                "yaml"
            } else {
                "toml"
            }
        }
        "conf" | "cfg" | "service" | "socket" | "timer" | "mount" | "target" | "path" | "slice"
        | "desktop" | "repo" | "gitconfig" | "editorconfig" => "ini",
        "zsh" | "bashrc" | "zshrc" | "profile" | "env" | "pkgbuild" | "ebuild" | "apkbuild" => {
            "bash"
        }
        "jsonc" | "jsonl" | "ndjson" | "geojson" | "har" | "webmanifest" => "json",
        "htm" | "vue" | "svelte" => "html",
        _ => language,
    }
}

/// Renders `content` as a syntax-highlighted HTML page using highlight.js,
/// with sticky line numbers, `#L<n>` / `#L<start>-L<end>` deep links, and raw copy.
fn highlighted_page(theme: &str, language: &str, content: &str) -> Response {
    use std::fmt::Write;

    let (light_theme, dark_theme) = resolve_theme_pair(theme);
    let resolved_language = resolve_language(language, content);
    let line_count = content.lines().count().max(1);
    let mut line_numbers = String::with_capacity(line_count * 64);
    for line in 1..=line_count {
        let _ = writeln!(
            line_numbers,
            r##"<a id="L{line}" class="line-num" href="#L{line}" data-line="{line}">{line}</a>"##
        );
    }

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static(mime::TEXT_HTML_UTF_8.as_ref()),
        )],
        formatdoc! {
            r#"
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <meta name="color-scheme" content="light dark">
                <link rel="stylesheet" href="https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.9.0/build/styles/{}.css" media="(prefers-color-scheme: light), (prefers-color-scheme: no-preference)">
                <link rel="stylesheet" href="https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.9.0/build/styles/{}.css" media="(prefers-color-scheme: dark)">
                <script src="https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.9.0/build/highlight.min.js"></script>
                <script>hljs.highlightAll();</script>
                <style>
                    :root {{
                        --code-font: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace;
                        --line-height: 20px;
                        --v-pad: 12px;
                        --gutter-bg: #ffffff;
                        --gutter-border: rgba(127, 127, 127, 0.2);
                        --line-highlight-bg: rgba(255, 212, 0, 0.22);
                        --line-num-active-bg: rgba(255, 212, 0, 0.32);
                        --btn-bg: rgba(127, 127, 127, 0.12);
                        --btn-hover-bg: rgba(127, 127, 127, 0.22);
                        --btn-border: rgba(127, 127, 127, 0.3);
                    }}
                    html {{
                        background-color: #ffffff;
                        color: #1f2328;
                        scroll-padding-top: max(140px, 28vh);
                    }}
                    @media (prefers-color-scheme: dark) {{
                        :root {{
                            --gutter-bg: #1e1e1e;
                            --line-highlight-bg: rgba(255, 212, 0, 0.14);
                            --line-num-active-bg: rgba(255, 212, 0, 0.24);
                        }}
                        html {{
                            background-color: #1e1e1e;
                            color: #d4d4d4;
                        }}
                    }}
                    body {{
                        margin: 0;
                        background-color: transparent;
                    }}
                    .code-view {{
                        display: flex;
                        min-width: 100%;
                        width: max-content;
                        position: relative;
                    }}
                    .line-numbers {{
                        position: sticky;
                        left: 0;
                        z-index: 3;
                        user-select: none;
                        -webkit-user-select: none;
                        padding: var(--v-pad) 0;
                        text-align: right;
                        font-family: var(--code-font);
                        font-size: 13px;
                        line-height: var(--line-height);
                        background-color: var(--gutter-bg);
                        border-right: 1px solid var(--gutter-border);
                        flex-shrink: 0;
                    }}
                    .line-num {{
                        display: block;
                        padding: 0 12px;
                        height: var(--line-height);
                        line-height: var(--line-height);
                        scroll-margin-top: max(140px, 28vh);
                        color: inherit;
                        opacity: 0.45;
                        text-decoration: none;
                        cursor: pointer;
                        box-sizing: border-box;
                    }}
                    .line-num:hover {{
                        opacity: 0.85;
                        text-decoration: underline;
                    }}
                    .line-num.active {{
                        opacity: 1;
                        font-weight: 600;
                        background-color: var(--line-num-active-bg);
                    }}
                    .code-area {{
                        position: relative;
                        flex: 1;
                        min-width: 0;
                    }}
                    .line-highlight {{
                        display: none;
                        position: absolute;
                        left: 0;
                        right: 0;
                        pointer-events: none;
                        z-index: 2;
                        background-color: var(--line-highlight-bg);
                    }}
                    pre {{
                        margin: 0;
                        position: relative;
                        z-index: 1;
                    }}
                    pre code,
                    pre code.hljs {{
                        display: block;
                        overflow-x: visible;
                        width: max-content;
                        min-width: 100%;
                        box-sizing: border-box;
                        padding: var(--v-pad) 16px;
                        font-family: var(--code-font);
                        font-size: 13px;
                        line-height: var(--line-height);
                    }}
                    .toolbar {{
                        position: fixed;
                        top: 12px;
                        right: 16px;
                        z-index: 10;
                        display: flex;
                        align-items: center;
                        gap: 8px;
                        user-select: none;
                        -webkit-user-select: none;
                    }}
                    .copy-toast {{
                        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
                        font-size: 12px;
                        padding: 4px 8px;
                        border-radius: 6px;
                        background-color: var(--gutter-bg);
                        border: 1px solid var(--btn-border);
                        opacity: 0;
                        transition: opacity 0.15s ease;
                        pointer-events: none;
                    }}
                    .copy-toast.visible {{
                        opacity: 0.92;
                    }}
                    .copy-btn {{
                        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
                        font-size: 12px;
                        line-height: 1.4;
                        padding: 5px 10px;
                        border-radius: 6px;
                        border: 1px solid var(--btn-border);
                        background-color: var(--gutter-bg);
                        color: inherit;
                        cursor: pointer;
                        opacity: 0.8;
                        transition: opacity 0.15s ease, background-color 0.15s ease;
                    }}
                    .copy-btn:hover {{
                        opacity: 1;
                        background-color: var(--btn-hover-bg);
                    }}
                </style>
            </head>
            <body>
            <div class="toolbar">
                <span id="copy-toast" class="copy-toast" aria-live="polite"></span>
                <button id="clear-line-btn" class="copy-btn" type="button" title="Clear selected lines (Esc)" hidden></button>
                <button id="copy-raw-btn" class="copy-btn" type="button" title="Copy raw file content">Copy</button>
            </div>
            <div class="code-view">
                <div id="line-numbers" class="line-numbers" aria-hidden="true">
            {}    </div>
                <div class="code-area">
                    <div id="line-highlight" class="line-highlight" aria-hidden="true"></div>
                    <pre><code class="{}">{}</code></pre>
                </div>
            </div>
            <script>
                const LINE_HEIGHT = 20;
                const TOP_PAD = 12;
                const TOTAL_LINES = {};
                let anchorLine = null;
                let toastTimer = null;

                function syncTheme() {{
                    const codeBlock = document.querySelector('.hljs');
                    if (!codeBlock) return;

                    const style = window.getComputedStyle(codeBlock);
                    const bgColor = style.backgroundColor;
                    const textColor = style.color;

                    document.body.style.backgroundColor = bgColor;
                    document.body.style.color = textColor;
                    document.documentElement.style.backgroundColor = bgColor;
                    document.documentElement.style.setProperty('--gutter-bg', bgColor);
                }}

                function copyToClipboard(text) {{
                    if (navigator.clipboard && navigator.clipboard.writeText) {{
                        return navigator.clipboard.writeText(text).catch(() => fallbackCopy(text));
                    }}
                    return Promise.resolve(fallbackCopy(text));
                }}

                function fallbackCopy(text) {{
                    const ta = document.createElement('textarea');
                    ta.value = text;
                    ta.style.position = 'fixed';
                    ta.style.opacity = '0';
                    document.body.appendChild(ta);
                    ta.select();
                    try {{
                        document.execCommand('copy');
                    }} finally {{
                        document.body.removeChild(ta);
                    }}
                }}

                function showToast(message) {{
                    const toast = document.getElementById('copy-toast');
                    if (!toast) return;
                    toast.textContent = message;
                    toast.classList.add('visible');
                    if (toastTimer) clearTimeout(toastTimer);
                    toastTimer = setTimeout(() => toast.classList.remove('visible'), 1600);
                }}

                function parseLineRange(hash) {{
                    const match = /^#L(\d+)(?:-L?(\d+))?$/i.exec(hash || '');
                    if (!match) return null;
                    const rawStart = parseInt(match[1], 10);
                    const rawEnd = match[2] ? parseInt(match[2], 10) : rawStart;
                    if (!rawStart || !rawEnd) return null;
                    const start = Math.max(1, Math.min(TOTAL_LINES, Math.min(rawStart, rawEnd)));
                    const end = Math.max(1, Math.min(TOTAL_LINES, Math.max(rawStart, rawEnd)));
                    return [start, end];
                }}

                function scrollToLine(line) {{
                    const contextOffset = Math.max(140, Math.floor(window.innerHeight * 0.28));
                    const targetTop = Math.max(0, TOP_PAD + (line - 1) * LINE_HEIGHT - contextOffset);
                    window.scrollTo({{ top: targetTop, left: window.scrollX }});
                }}

                function clearLineSelection() {{
                    anchorLine = null;
                    history.replaceState(null, '', window.location.pathname + window.location.search);
                    applyLineHighlight(false);
                    showToast('Cleared line selection');
                }}

                function applyLineHighlight(shouldScroll) {{
                    const activeNums = document.querySelectorAll('.line-num.active');
                    activeNums.forEach((el) => el.classList.remove('active'));

                    const overlay = document.getElementById('line-highlight');
                    const clearBtn = document.getElementById('clear-line-btn');
                    const range = parseLineRange(window.location.hash);
                    if (!range) {{
                        if (overlay) overlay.style.display = 'none';
                        if (clearBtn) clearBtn.hidden = true;
                        return;
                    }}

                    const [start, end] = range;
                    if (anchorLine === null) anchorLine = start;

                    for (let i = start; i <= end; i++) {{
                        const el = document.getElementById('L' + i);
                        if (el) el.classList.add('active');
                    }}

                    if (overlay) {{
                        overlay.style.top = (TOP_PAD + (start - 1) * LINE_HEIGHT) + 'px';
                        overlay.style.height = ((end - start + 1) * LINE_HEIGHT) + 'px';
                        overlay.style.display = 'block';
                    }}

                    if (clearBtn) {{
                        const label = start === end ? '#L' + start : '#L' + start + '-L' + end;
                        clearBtn.textContent = label + ' \u00d7';
                        clearBtn.hidden = false;
                    }}

                    if (shouldScroll) {{
                        scrollToLine(start);
                    }}
                }}

                document.getElementById('line-numbers').addEventListener('click', (event) => {{
                    const link = event.target.closest('.line-num');
                    if (!link || event.metaKey || event.ctrlKey || event.altKey || event.button !== 0) return;
                    event.preventDefault();

                    const clickedLine = parseInt(link.dataset.line, 10);
                    if (!clickedLine) return;

                    const currentRange = parseLineRange(window.location.hash);
                    if (
                        !event.shiftKey &&
                        currentRange &&
                        currentRange[0] === clickedLine &&
                        currentRange[1] === clickedLine
                    ) {{
                        clearLineSelection();
                        return;
                    }}

                    let hash;
                    if (event.shiftKey && anchorLine !== null) {{
                        const start = Math.min(anchorLine, clickedLine);
                        const end = Math.max(anchorLine, clickedLine);
                        hash = start === end ? '#L' + start : '#L' + start + '-L' + end;
                    }} else {{
                        anchorLine = clickedLine;
                        hash = '#L' + clickedLine;
                    }}

                    history.replaceState(null, '', hash);
                    applyLineHighlight(false);
                    copyToClipboard(window.location.href).then(() => {{
                        showToast('Copied ' + hash + ' link');
                    }});
                }});

                document.getElementById('clear-line-btn').addEventListener('click', () => {{
                    clearLineSelection();
                }});

                document.addEventListener('keydown', (event) => {{
                    if (event.key === 'Escape' && parseLineRange(window.location.hash)) {{
                        clearLineSelection();
                    }}
                }});

                document.getElementById('copy-raw-btn').addEventListener('click', () => {{
                    const codeEl = document.querySelector('pre code');
                    if (!codeEl) return;
                    const btn = document.getElementById('copy-raw-btn');
                    copyToClipboard(codeEl.textContent || '').then(() => {{
                        btn.textContent = 'Copied!';
                        showToast('Copied raw content');
                        setTimeout(() => {{ btn.textContent = 'Copy'; }}, 1500);
                    }});
                }});

                window.addEventListener('DOMContentLoaded', () => applyLineHighlight(true));
                window.addEventListener('load', () => {{
                    syncTheme();
                    applyLineHighlight(true);
                    requestAnimationFrame(() => applyLineHighlight(true));
                }});
                window.addEventListener('hashchange', () => applyLineHighlight(true));
                window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', syncTheme);
            </script>
            </body>
            "#,
            light_theme,
            dark_theme,
            line_numbers,
            resolved_language,
            escape_text(content),
            line_count
        },
    )
        .into_response()
}

fn is_safe_markdown_url(url: &str) -> bool {
    let trimmed = url.trim();
    match trimmed.split_once(':') {
        Some((scheme, _)) if !scheme.contains(['/', '?', '#']) => matches!(
            scheme.to_ascii_lowercase().as_str(),
            "http" | "https" | "mailto"
        ),
        _ => true,
    }
}

fn render_markdown(content: &str) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_GFM;

    let parser = MarkdownParser::new_ext(content, options).map(|event| match event {
        Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(raw),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) if !is_safe_markdown_url(&dest_url) => Event::Start(Tag::Link {
            link_type,
            dest_url: CowStr::from("#"),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) if !is_safe_markdown_url(&dest_url) => Event::Start(Tag::Image {
            link_type,
            dest_url: CowStr::from(""),
            title,
            id,
        }),
        other => other,
    });

    let mut rendered = String::with_capacity(content.len());
    markdown_html::push_html(&mut rendered, parser);
    rendered
}

/// Renders Markdown `content` as a GitHub-flavored HTML page with highlight.js code blocks.
fn markdown_page(theme: &str, content: &str) -> Response {
    let (light_theme, dark_theme) = resolve_theme_pair(theme);
    let rendered = render_markdown(content);
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static(mime::TEXT_HTML_UTF_8.as_ref()),
        )],
        formatdoc! {
            r#"
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <meta name="color-scheme" content="light dark">
                <link rel="stylesheet" href="https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.9.0/build/styles/{}.css" media="(prefers-color-scheme: light), (prefers-color-scheme: no-preference)">
                <link rel="stylesheet" href="https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.9.0/build/styles/{}.css" media="(prefers-color-scheme: dark)">
                <script src="https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.9.0/build/highlight.min.js"></script>
                <script>hljs.highlightAll();</script>
                <style>
                    :root {{
                        color-scheme: light dark;
                        --bg-color: #ffffff;
                        --text-color: #1f2328;
                        --link-color: #0969da;
                        --code-bg: #f6f8fa;
                        --inline-code-bg: rgba(175, 184, 193, 0.2);
                        --border-color: #d0d7de;
                        --row-bg: #f6f8fa;
                        --quote-color: #59636e;
                    }}
                    @media (prefers-color-scheme: dark) {{
                        :root {{
                            --bg-color: #0d1117;
                            --text-color: #e6edf3;
                            --link-color: #58a6ff;
                            --code-bg: #161b22;
                            --inline-code-bg: rgba(110, 118, 129, 0.4);
                            --border-color: #30363d;
                            --row-bg: #161b22;
                            --quote-color: #8b949e;
                        }}
                    }}
                    html, body {{
                        margin: 0;
                        background-color: var(--bg-color);
                        color: var(--text-color);
                        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "Noto Sans", Helvetica, Arial, sans-serif;
                        line-height: 1.6;
                    }}
                    .markdown-body {{
                        max-width: 880px;
                        margin: 0 auto;
                        padding: 32px 24px;
                        box-sizing: border-box;
                        overflow-wrap: break-word;
                    }}
                    .markdown-body > *:first-child {{
                        margin-top: 0;
                    }}
                    .markdown-body h1,
                    .markdown-body h2,
                    .markdown-body h3,
                    .markdown-body h4,
                    .markdown-body h5,
                    .markdown-body h6 {{
                        margin-top: 24px;
                        margin-bottom: 16px;
                        font-weight: 600;
                        line-height: 1.25;
                    }}
                    .markdown-body h1,
                    .markdown-body h2 {{
                        padding-bottom: 0.3em;
                        border-bottom: 1px solid var(--border-color);
                    }}
                    .markdown-body p,
                    .markdown-body ul,
                    .markdown-body ol,
                    .markdown-body blockquote,
                    .markdown-body table,
                    .markdown-body pre {{
                        margin-top: 0;
                        margin-bottom: 16px;
                    }}
                    .markdown-body ul,
                    .markdown-body ol {{
                        padding-left: 2em;
                    }}
                    .markdown-body li + li {{
                        margin-top: 0.25em;
                    }}
                    .markdown-body a {{
                        color: var(--link-color);
                        text-decoration: none;
                    }}
                    .markdown-body a:hover {{
                        text-decoration: underline;
                    }}
                    .markdown-body code {{
                        font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace;
                        font-size: 85%;
                        padding: 0.2em 0.4em;
                        border-radius: 6px;
                        background-color: var(--inline-code-bg);
                    }}
                    .markdown-body pre {{
                        padding: 16px;
                        overflow-x: auto;
                        border-radius: 6px;
                        border: 1px solid var(--border-color);
                        background-color: var(--code-bg);
                    }}
                    .markdown-body pre code,
                    .markdown-body pre code.hljs {{
                        padding: 0;
                        background: transparent;
                        border-radius: 0;
                        font-size: 85%;
                        line-height: 1.45;
                        overflow-x: visible;
                    }}
                    .markdown-body blockquote {{
                        margin-left: 0;
                        margin-right: 0;
                        padding: 0 1em;
                        color: var(--quote-color);
                        border-left: 0.25em solid var(--border-color);
                    }}
                    .markdown-body table {{
                        display: block;
                        width: max-content;
                        max-width: 100%;
                        overflow-x: auto;
                        border-collapse: collapse;
                    }}
                    .markdown-body th,
                    .markdown-body td {{
                        padding: 6px 13px;
                        border: 1px solid var(--border-color);
                    }}
                    .markdown-body tr:nth-child(2n) {{
                        background-color: var(--row-bg);
                    }}
                    .markdown-body hr {{
                        height: 0.25em;
                        padding: 0;
                        margin: 24px 0;
                        background-color: var(--border-color);
                        border: 0;
                    }}
                    .markdown-body img {{
                        max-width: 100%;
                        box-sizing: border-box;
                    }}
                </style>
            </head>
            <body>
            <main class="markdown-body">{}</main>
            </body>
            "#,
            light_theme,
            dark_theme,
            rendered
        },
    )
        .into_response()
}

fn is_markdown_ext(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "md" | "markdown" | "mdown" | "mkd"
    )
}

/// The README doubles as the index page. Its API examples use this origin,
/// which is replaced with the public base URL when serving the page.
const INDEX_PAGE: &str = include_str!("../../README.md");
const INSTALL_DOC: &str = include_str!("../../docs/install.md");
const ARCHITECTURE_DOC: &str = include_str!("../../docs/architecture.md");
const INDEX_EXAMPLE_ORIGIN: &str = "http://localhost:3000";

fn negotiate_markdown_response(headers: &HeaderMap, theme: &str, content: String) -> Response {
    let accepts_html = headers
        .get_all(header::ACCEPT)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .any(|value| value.contains(mime::TEXT_HTML.as_ref()));

    let response = if accepts_html {
        markdown_page(theme, &content)
    } else {
        content.into_response()
    };

    ([(header::VARY, header::ACCEPT.as_str())], response).into_response()
}

/// Serves the index page: rendered Markdown HTML for clients that accept
/// HTML (browsers), plain text otherwise (e.g. curl).
pub async fn index_handler(
    base_url: Result<BaseUrl, TypedHeaderRejection>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Response {
    let content = match base_url {
        Ok(BaseUrl(base_url)) => INDEX_PAGE.replace(INDEX_EXAMPLE_ORIGIN, &base_url),
        Err(_) => INDEX_PAGE.to_string(),
    };

    negotiate_markdown_response(&headers, &state.args.syntax_theme, content)
}

/// Serves embedded project documentation under `/docs/{doc}`.
pub async fn doc_handler(
    Path(doc_name): Path<String>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    let content = match doc_name.as_str() {
        "install.md" => INSTALL_DOC,
        "architecture.md" => ARCHITECTURE_DOC,
        _ => return Err(AppError::NotFound),
    };

    Ok(negotiate_markdown_response(
        &headers,
        &state.args.syntax_theme,
        content.to_string(),
    ))
}

pub async fn get_handler(
    Path(file_hash): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    let (file_name, file_ext) = parse_filehash(file_hash.as_str());

    let dir = &state.args.file_path;
    let file_path = FilePath::new(dir).join(file_name);

    if file_path.exists() {
        match fs::read_to_string(&file_path).await {
            Ok(content) =>
            // 302 redirect if the content is a valid URL
            {
                if content.starts_with("http") && !content.contains([' ', '\n']) {
                    Ok(Redirect::temporary(&content).into_response())
                } else if file_ext.as_ref().is_none_or(|ext| ext == "txt") {
                    Ok(content.into_response())
                } else if file_ext.as_deref().is_some_and(is_markdown_ext) {
                    Ok(markdown_page(&state.args.syntax_theme, &content))
                } else {
                    Ok(highlighted_page(
                        &state.args.syntax_theme,
                        &file_ext.unwrap_or_default(),
                        &content,
                    ))
                }
            }
            _ => match TokioFile::open(&file_path).await {
                Ok(file) => {
                    let stream = ReaderStream::new(file);
                    let body = Body::from_stream(stream);
                    let content_type = mime_guess::from_ext(&file_ext.unwrap_or_default())
                        .first_or_octet_stream()
                        .to_string();

                    Ok(
                        (StatusCode::OK, [(header::CONTENT_TYPE, content_type)], body)
                            .into_response(),
                    )
                }
                Err(e) => Err(AppError::IoError(e)),
            },
        }
    } else {
        Err(AppError::NotFound)
    }
}

const HASHER: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
pub async fn put_handler(
    BaseUrl(base_url): BaseUrl,
    State(state): State<Arc<AppState>>,
    bytes: Bytes,
) -> Result<String, AppError> {
    use base64::prelude::*;

    let hash = &BASE64_URL_SAFE.encode(HASHER.checksum(&bytes).to_be_bytes())[0..4];

    let file_name = format!("{}.txt", hash);
    let file_path = FilePath::new(&state.args.file_path).join(file_name);
    let mut file = TokioFile::create(&file_path).await?;

    file.write_all(&bytes).await?;
    file.flush().await?;

    info!("File saved: hash: {} size: {} bytes", hash, bytes.len());

    let timestamp = file_to_timestamp(&file).await?;
    let secret = HASHER.checksum(timestamp.as_bytes()).to_string();

    Ok(formatdoc! {"
        url: {base_url}/{hash}
        short: {hash}
        size: {size} bytes
        secret: {secret}
        ",
        size = bytes.len(),
    })
}

pub async fn delete_handler(
    Path(file_hash): Path<String>,
    State(state): State<Arc<AppState>>,
    secret: String,
) -> Result<String, AppError> {
    let (file_name, _) = parse_filehash(file_hash.as_str());

    let dir = &state.args.file_path;
    let file_path = FilePath::new(dir).join(file_name);

    let file = TokioFile::open(&file_path).await?;

    let timestamp = file_to_timestamp(&file).await?;
    let file_secret = HASHER.checksum(timestamp.as_bytes()).to_string();

    if file_path.exists() {
        if secret == file_secret {
            fs::remove_file(file_path).await?;
            Ok(format!("File {} deleted successfully", file_hash))
        } else {
            Err(AppError::Forbidden)
        }
    } else {
        Err(AppError::NotFound)
    }
}
