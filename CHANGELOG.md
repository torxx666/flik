# Changelog

All notable changes to [Flik](https://github.com/torxx666/flik) are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.3.0] - 2026-11-12

### Added
* **Live Multi-File Merge Mode (`-m`, `--merge`)**:
  * Combines multiple log files into a source-tagged stream; sorts existing events and each live polling batch by timestamp (`flik -m api.log worker.log auth.log`).
  * Continuous background round-robin tailing with support for append events and size-decreasing truncation. Replacement files that are as large as the previous offset are not reliably detected.
  * Visual source tag `[filename]` with distinct per-file color coding for immediate source identification.
  * Full integration with existing filters, quick search, statistics, and follow mode.
* **Long-Split 3-Column View (`-l`, `--long-split`)**:
  * Newspaper-style 3-column layout displaying consecutive pages: Left (`p-2`), Middle (`p-1`), and Right (`Current`).
  * Works on both single files and merged multi-file streams.
  * Upward continuous cascade: new incoming logs arrive at the bottom of the right column and push content upward; lines overflowing the top of Column 3 enter at the bottom of Column 2 (`p-1`), and overflow from Column 2 enters at the bottom of Column 1 (`p-2`).
  * Seamless chronological continuity across columns without duplicate or missing lines.
  * Three-way view mode toggle via <kbd>4</kbd> or <kbd>Tab</kbd>: `CompactLine` &rarr; `ColumnsTable` &rarr; `LongSplit`.
  * Multi-column mouse support: clicking any column selects the exact line and supports double-click copy.
* **Automatic Preferences Persistence**:
  * Toggling theme (<kbd>T</kbd>) or view mode (<kbd>4</kbd> / <kbd>Tab</kbd>) in the TUI automatically updates and saves `theme` and `default_view` in `~/.config/flik/config.toml` (or `%APPDATA%\flik\config.toml` on Windows).
* **Comprehensive Test Coverage**:
  * Added `tests/test_live_merge.rs` testing multi-source multiplexing, format parsing with source prefixes, long-split 3-page math, and AZERTY/standard key toggles.

### Changed
* Updated CLI documentation and `--help` to feature `--merge` and `--long-split`.
* Enhanced `config.toml` schema with `default_view`.

---

## [0.2.0] - 2026-11-05

### Added
* **Expanded Cloud & Enterprise Format Support**:
  * **AWS CloudWatch**: Native support for `@message`, `@logStream`, `@timestamp`, and epoch millisecond conversion.
  * **Google Cloud Logging (Stackdriver)**: Automatic `severity` level mapping, `textPayload`, and nested `jsonPayload` flattening.
  * **Logfmt (`key=value`)**: Auto-detection, key-value extraction, log levels, and HTTP status codes.
  * **Syslog & Auth Logs**: Dedicated enrichment for `/var/log/auth.log`, `secure`, `sshd`, `sudo`, and `PAM` with structured fields (`user`, `ip`, `action`, `auth_status`, `command`).
  * **Extended HTTP Access Logs**: Support for virtual hosts (`vhost`) and custom trailing latency metrics (`request_time`).
  * **Custom Regex Formats**: Pass `--format-regex '(?P<name>...)'` on CLI or define formats in `~/.config/flik/formats/*.toml`.
* **Investigation & Correlation Tools**:
  * Trace ID and correlation identifier extraction (W3C `traceparent`, UUID, IP, username) via <kbd>C</kbd> or <kbd>Ctrl+R</kbd>.
  * JSON Beautify inspector modal accessible with <kbd>J</kbd>.
  * Real-time stream alerting when error rates exceed threshold in live tail mode (<kbd>5</kbd> / <kbd>t</kbd>).
  * Full-file analytics scan mode (<kbd>S</kbd> then <kbd>a</kbd>) scanning 100% of the indexed file.
  * Nested JSON tab completion in search and filter editors.
* **Global Configuration**:
  * Automatic configuration loading from `~/.config/flik/config.toml` and modular format definitions in `~/.config/flik/formats/*.toml`.
* **Cross-Platform Distribution**:
  * Native Windows 64-bit MSVC binaries (`flik.exe`).
  * Official Homebrew tap (`brew install torxx666/tap/flik`).
  * Static Linux musl binaries for x86_64 and ARM64.

### Fixed
* Fixed horizontal scroll syntax color phase rendering.
* Fixed piped stdin terminal initialization on macOS kqueue.
* Fixed Windows MSVC build paths and line endings handling.

---

## [0.1.0] - 2026-10-26

### Added
* **Initial Open Source Release**:
  * $O(1)$ direct byte-offset indexing engine for multi-gigabyte log files.
  * Non-blocking background indexing with responsive 60 fps TUI.
  * Smart search with contextual before/after match expansion (`//error`, `//3,2:error`).
  * Stackable Boolean include/exclude filters (`AND`, `OR`, `NOT`, quotes, groupings).
  * Split-pane workspace with independent filters, views, and navigation.
  * Auto-detection for standard JSON, Apache/Nginx access and error logs, Kubernetes CRI-O logs.
  * Interactive statistics dashboards, pattern clustering, and hourly timeline histograms.
  * Source-aware incident export and session save/restore.

