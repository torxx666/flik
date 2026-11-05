# Flik

**Interactive log analysis in your terminal.**

Open local files or piped streams, inspect structured fields, stack filters and recover the context around an error. Flik is written in Rust and runs on macOS and Linux.

[Website and demonstrations](https://flik.spedo.dev) · [Download binaries](https://github.com/torxx666/flik/releases/latest) · [Release guide](docs/RELEASING.md)

## Install without building

### Homebrew (macOS & Linux)

The fastest and recommended way to install and update Flik:

```sh
brew install torxx666/tap/flik
```

### Prebuilt Binaries

Download the archive matching your platform from [GitHub Releases](https://github.com/torxx666/flik/releases/latest), verify its hash against `SHA256SUMS`, then extract it. Each archive contains `flik` (or `flik.exe`) and `LICENSE`.

| Platform | Release target |
| --- | --- |
| macOS Apple Silicon | `aarch64-apple-darwin` |
| macOS Intel | `x86_64-apple-darwin` |
| Linux ARM64 | `aarch64-unknown-linux-musl` |
| Linux x86-64 | `x86_64-unknown-linux-musl` |
| Windows x86-64 | `x86_64-pc-windows-msvc` |

An installer can detect the platform and install into `~/.local/bin`:

```sh
curl -fsSL https://raw.githubusercontent.com/torxx666/flik/main/scripts/install.sh | bash
```

The installer checks the download's SHA-256 and executable version. It uses no sudo and refuses existing files by default. To intentionally update an existing installation, set `FLIK_FORCE=1`. See [installation details](docs/RELEASING.md) for pinned versions and custom destinations.

macOS binaries require macOS 11 or later and are currently not Developer ID signed. If Gatekeeper displays a warning on macOS, unquarantine the binary with:
```sh
xattr -d com.apple.quarantine ./flik
```
Windows builds run natively on 64-bit Windows 10/11 (`flik.exe`) or via WSL2.

## Performance & Scalability

Flik is designed for multi-gigabyte log analysis without memory bloat:
* **$O(1)$ Direct Seeks**: Instant random navigation across millions of lines via a lightweight byte-offset index (~8 bytes per line).
* **Zero UI Blocking**: Asynchronous background indexing ensures the TUI opens instantly and remains 60 fps responsive.
* **Bounded Resource Usage**: 10 million lines consume ~80 MB of RAM, leaving memory free for your workloads.

## Start an investigation

```sh
flik app.log
flik api.log worker.log
flik /var/log/auth.log
flik --format logfmt app.log
flik --format-regex '^(?P<ip>\S+) (?P<user>\S+) \[(?P<timestamp>[^\]]+)\] "(?P<request>[^"]*)" (?P<status>\d{3}) (?P<latency>[\d\.]+)$' access.log
flik --table --fields timestamp,service,message app.jsonl
docker logs -f demo-api | flik -
kubectl logs -f deployment/demo-api | flik -
aws logs tail /aws/lambda/demo-api --follow | flik -
gcloud logging tail "resource.type=k8s_container" --format=json | flik -
```

Files open at the end with live following enabled. Each of the two available panes has independent filters, fields and navigation.

Supported formats and detection:
* **JSON / NDJSON**: Automatic field indexing, nested JSON expansion, and coloration.
* **AWS CloudWatch**: Native support for `@message`, `@logStream`, `@timestamp`, epoch milliseconds conversion, and `aws logs tail ... | flik -`.
* **Google Cloud Logging (Stackdriver)**: Automatic `severity` mapping, `textPayload`, nested `jsonPayload` field flattening, and `gcloud logging tail ... | flik -`.
* **Logfmt (`key=value`)**: Key-value pair extraction, log level and HTTP status recognition.
* **Syslog & Auth (`/var/log/auth.log`, `secure`, `sshd`, `sudo`, `PAM`)**: Enriched security fields (`user`, `ip`, `action`, `auth_status`, `command`).
* **HTTP Access Logs (Apache & Nginx)**: Combined, Common, virtual hosts (`vhost`), and custom trailing latency metrics (`request_time`).
* **Custom Regex Patterns**: Pass `--format-regex` with named capture groups `(?P<name>...)` to map any custom log structure into interactive columns and filters.
* **Kubernetes / CRI-O**: Automatic embedded JSON extraction with `w`.

Flik focuses on interactive incident cockpit analysis, fast temporal correlation (`compare`, `trace`), and split-pane navigation. Keep tools such as `jq` for batch JSON scripting, and `lnav` for complex SQL database queries across files.

## Search, stack filters, undo

From the main view, type `/error` and press Enter to add a search filter. Smart find also retains surrounding events:

```text
//error          one line before each match
//3,2:error      three lines before and two after
```

Press `3` to add an include filter or `8` to add an exclude filter. Examples:

```text
service:payment
level:error
status:5xx
(level:error OR status>=500) AND NOT path:/health
"connection reset" OR "upstream timeout"
```

Use `x` to remove the most recent rule, `X` to clear filters and restore fields, and `Ctrl+F` to manage the stack. `Tab` in search and `Ctrl+Space` in a filter prompt complete field names and sampled values.

For Kubernetes logs, `w` expands embedded JSON. `Tab` switches line/table view; `6` selects fields. `v` inspects distinct field values and their frequencies.

## Count, correlate and compare

Press `S` for a statistics dashboard. HTTP access logs show status groups; error logs without HTTP codes show detected severity levels such as ERROR, NOTICE and WARN. This dashboard samples up to 5,000 lines.

Press `H` to count the active filter's occurrences per hour across the full indexed file. Context-only lines are excluded. Reports use UTC and report matches without recognized timestamps separately.

Open `Ctrl+K` for investigation commands:

| Command | Purpose |
| --- | --- |
| `context 20` | Read 20 original lines before and after the selected event |
| `histogram minute` / `hour` / `day` | Count matching events by interval; use the full command, e.g. `histogram day` |
| `time START..END` | Filter an inclusive RFC3339 time range |
| `jump TIME` / `around 5` / `untime` | Jump to a timestamp, select a ±5-minute window, or remove the time filter |
| `merge` | Merge matching events from both files chronologically |
| `trace req-demo-42` | Follow a correlation ID across both files |
| `compare 2026-01-15T10:00:00Z 15` | Compare equal 15-minute windows before and after a timestamp |
| `compare files` | Compare normalized patterns in the two open files |
| `save session.json` / `load session.json` | Save or restore the workspace |
| `redact email,customer_id` | Add field names to the built-in redaction keys |
| `export incident.jsonl` | Export source-aware events with context and key-based redaction |

Time inputs need an explicit timezone. Timezone-less recognized log timestamps are interpreted as UTC. Use a timestamp present in your data. Reports and merged investigations are snapshots; rerun to refresh. Escape cancels a background investigation.

Comparisons identify changes in observed pattern counts and shares; they do not prove a cause. Existing filters remain active for trace and merge. Redaction is based on known fields and patterns; review exported data before sharing.

## Keyboard reference

Shortcuts are case-sensitive. These apply to the main log view unless noted. `1`, `?` or `F1` opens the in-app guide.

| Key | Action |
| --- | --- |
| Up/Down or `k`/`j` | Move through lines |
| Space, `f`, Page Down / `b`, Page Up | Page down / up |
| `g` / `G` | Beginning / end and resume following |
| Left/Right | Horizontal scrolling |
| `/` / `7` | Quick search |
| `3`, `+`, `i` / `8`, `-`, `e` | Include / exclude filter |
| `x` / `X` / `Ctrl+F` | Undo / clear / manage filters |
| `Tab` / `4` | Line / table view |
| `6` / `R` / `v` | Select fields / field range / inspect values |
| `w` | Toggle embedded JSON expansion |
| `5` / `t` | Toggle live following & real-time stream alerting |
| `S` / `H` / `P` | Statistics (live auto-refresh) / hourly counts / pattern groups |
| `C` / `Ctrl+R` | Correlate trace ID, UUID, IP or user (`Enter`: Include, `e`: Exclude, `t`: Trace) |
| `B` / `'` | Bookmark / next bookmark |
| `D` / `d` | Select two lines for a difference view |
| `Ctrl+G` / `Ctrl+T` / `Ctrl+L` | Context / time range / merged chronology |
| `Ctrl+K` | Investigation command prompt |
| `Ctrl+O` / `Ctrl+N` | Open in active / other pane |
| `Ctrl+Left` / `Ctrl+Right`, `F12`, `Ctrl+]` | Focus / switch panes |
| `Ctrl+W` / `Ctrl+P` | Close pane / save workspace |
| `Ctrl+S` / `Ctrl+E` | Export lines / export an incident bundle |
| `y` / `M` | Copy line / toggle mouse capture |
| `I` / `T` / `c` / `n` | Newest-first / theme / colors / line numbers |
| `2`, `o`, `F2` / `r` | Detect format / reload file |
| `9`, `m`, Escape, `F9` | Menu |
| `Q`, twice `q`, `F10` | Quit |

Filter prompts: `F2` or `Ctrl+R` toggles regex; `F3` or `Ctrl+C` toggles case sensitivity. In a filter prompt, Tab changes include/exclude mode. In a context view, `n`/`p` move between matches.

## Build and test

```sh
cargo build --locked --release
cargo test --locked --all-targets
python3 scripts/test_install.py
python3 website/scripts/check_site.py
```

The release workflow uses Rust 1.94.0. `Cargo.lock` is tracked. Log files are indexed in background batches; the byte-offset index uses memory proportional to the number of lines (at least eight bytes per line, plus capacity and filter results).

The website is a standalone static site in [website/dist](website/README.md). Its demonstrations use only the included synthetic fixtures. No private logs or hosting credentials belong in this repository.

## License

[MIT](LICENSE).
