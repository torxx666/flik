# Flik (⚡) - High-Performance Log Analyzer

> **Ultra-fast log analyzer (Apache, Kubernetes, JSON...) written in Rust, built for multi-gigabyte log files with an intuitive Norton Commander-style interface.**

---

## Download without building

Prepared release targets: **macOS Apple Silicon / Intel** and **Linux ARM64 / x86-64**. GitHub Releases will provide versioned `.tar.gz` archives containing `flik` and `LICENSE`, plus `SHA256SUMS`. A public repository and first published release are still required; no download URL is invented here.

The installer in `scripts/install.sh` detects the platform and verifies the release archive before installing into `~/.local/bin`. Set `FLIK_REPO=OWNER/REPO` to the selected public repository. See [the release and installation guide](docs/RELEASING.md) for exact commands and the four-platform GitHub Actions workflow. Releases are created as drafts for maintainer review; the current macOS binaries are not Developer ID signed/notarized.

A product website with an original logo and three actual recordings on synthetic logs is prepared in [website/](website/README.md), for **flik.spedo.dev**. Its downloads activate when the configured public repository has matching release assets.

For an initial public source handoff, run `python3 scripts/package_open_source.py`. It creates a curated ZIP under `artifacts/`, excluding Git history, hosting identity, local logs, internal notes and build artifacts. The package uses the included **MIT license**. The script replaces placeholder demo domains with reserved `example.test` domains in the exported copy.

## 🚀 Key Features

- **Handle Massive Multi-Gigabyte Files without RAM bloat:**
  - Non-blocking asynchronous byte-offset indexing ($O(1)$ random line access via direct seeks).
  - Startup preview limited to 2,000 lines or 256 KiB, followed by background indexing.
  - Memory grows with line count: the byte-offset index uses at least 8 bytes per line, plus capacity and filter results.
- **Intuitive Norton Commander UI (Modern Dark Theme):**
  - **Top Menu Bar accessible via `ESC`, `F9`, or `m`** (Left/Right to switch menus, Enter to select).
  - 10-key function bar at the bottom with direct shortcuts and full mouse click support.
  - Sleek dark slate theme with high contrast and colored severity badges.
- **Mac-Friendly Navigation (No F-keys required!):**
  - Direct keys: `[1] Help`, `[2] Auto-Format`, `[3] +Include`, `[4] Table/Line`, `[5] Tail`, `[6] Fields`, `[7] Find`, `[8] -Exclude`, `[9] Menu`.
  - Paging: `Space` or `f` (Page Down), `b` (Page Up).
  - File Jumps: `g` (Top of file), `G` (End of file).
  - Double-tap quit: Press `q` twice (**`QQ`**) or capital `Q` to quit safely!
- **Rich JSON Syntax Highlighting:**
  - Distinct colored JSON keys (vibrant cyan with bold).
  - Syntax-highlighted values (strings in soft green, numbers in bright gold, booleans/null in lavender, colons/braces in slate).
- **Smart Field & Status Filters:**
  - `status:5xx` : Matches all HTTP 500-599 errors across JSON, Apache, or raw logs.
  - `status:404` : Matches exact HTTP status code 404.
  - `level:error` : Matches ERROR, ERR, FATAL, CRITICAL across any format.
  - `level:warn` : Matches WARN and WARNING levels.
  - `service:auth` / `key:value` : Matches structured fields in JSON and logfmt.
  - Stackable filter manager (`Ctrl+F`) to toggle, add, or delete filters on the fly.
- **⚡ Killer Features & Deep Analytics (Nouvelles fonctionnalités exclusives) :**
  - **Instant Stats & Top 10 Dashboard (`S` / `s`)**: Adaptive breakdown of HTTP 2xx/3xx/4xx/5xx status codes or detected severity levels (`ERROR`, `NOTICE`, etc.) when HTTP codes are absent, Top 10 client IPs, Top 10 endpoints, Top 10 errors, and p95/average response latency. Press `Enter` on any IP or endpoint to immediately filter!
  - **Filtered occurrences over time (`H` / Shift+H)**: A background report counts the active filter’s actual matches per hour, with dates, exact counts and bars. Context lines are excluded. Use `histogram minute|hour|day` in Ctrl+K to choose the interval.
  - **Log Pattern Clustering & Rare Pattern Detection (`P` / `p`)**: Automatically abstracts variable tokens (`<IP>`, `<NUM>`, `<UUID>`, `<TIME>`) into template signatures and isolates rare anomaly patterns (< 2%) with 1-click filtering.
  - **Bookmarks & Line Pinning (`B` / `'`)**: Star bookmark any line (`★` rendered in gutter). Press `'` to instantly cycle through all bookmarks.
  - **Split-Screen Side-by-Side Diff (`D` / `d`)**: Press `D` on line 1, navigate to line 2 and press `D` to open an aligned split-screen comparison with highlighted token differences in red and green!
  - **Trace & Correlation ID Discovery (`C` / Shift+C)**: Extracts `trace_id`, `request_id`, `account_id`, `order_id`, and UUIDs on the selected line. Press `Enter` to isolate all log lines across the distributed request!

- **Dual Display Modes (`Tab` or `4`):**
  - **Compact Line View**: Highlighted timestamp, colored level badge, status code, and colorized fields.
  - **Columns Table View**: Spreadsheet-like aligned columns for selected fields with horizontal scrolling.
  - **Field Selector (`6` or `f`)**: Toggle which structured fields you want visible.
- **Live Follow Mode (`tail -F`) & Live Activity Loupiote (🟢) :**
  - Chaque fichier s’ouvre directement à la fin, avec le suivi des nouvelles lignes activé par défaut.
  - **Petite Loupiote LIVE** : un voyant vert clignotant / pulsant (`🟢 LIVE`) s'allume automatiquement dès qu'un fichier est actif ou que de nouvelles données s'ajoutent en direct !
  - **Compteur de lignes en temps réel** : si le mode tail est désactivé, la loupiote affiche `🟢 LIVE: +X new lines! [Press 5/t to follow]` pour vous prévenir sans perturber votre lecture.
  - **Indexation continue sans blocage** : les nouvelles lignes sont indexées en tâche de fond même quand vous naviguez dans les anciennes lignes.
  - Basculez à tout moment en mode suivi avec `5`, `t`, ou en cliquant sur le bouton de fonction en bas.
- **Save / Export Extraction (`Ctrl+S`):**
  - Export all filtered lines or visible range into **Raw Lines**, **JSON Lines**, or **CSV**. The destination must be a new file; existing files are never overwritten.

- **Distinct Field Values Inspector (`v`):**
  - Instant facet analyzer: displays all unique values for any field (e.g. `account_id`, `merchant`, `status`).
  - Shows occurrence counts and percentage frequency with quick filter selection (`Enter` to filter by selected value).
- **Clipboard & Mouse Selection for Mac & Terminals:**
  - **Double-Click Word**: Double-click any word, IP, or error code to instantly copy it to clipboard.
  - **Native Selection**: Hold **`Option` (⌥)** while dragging to select text directly with mouse on macOS Terminal / iTerm2, then `Cmd+C` to copy.
  - **Yank Line (`y`)**: Copies the highlighted log line directly to your system clipboard (`pbcopy` & OSC 52 sequence).
  - **Toggle Mouse Capture (`M`)**: Disables TUI mouse grabbing so you can highlight text freely.
- **Piped Stream & Docker Logs Support (`-`):**
  - Pipe real-time streams directly into Flik: `docker logs -f <container> | ./flik.sh -` (or `cat access.log | flik -`).
  - Terminal raw mode and UI controls stay completely interactive via `/dev/tty`.

---

## ⌨️ Shortcuts Reference (Mac & Standard)

| Key | Action | Description |
|---|---|---|
| **`S` / `s`** | **Stats Dashboard** | HTTP status breakdown, Top 10 IPs, endpoints, errors, p95 latency |
| **`H` (Shift+H)** | **Timeline Histogram** | Visual volume & error spikes across time (Enter to jump to time) |
| **`P` / `p`** | **Pattern Clustering** | Abstract patterns & anomaly detection (< 2% frequency) |
| **`B`** | **Toggle Bookmark (★)** | Star pin current line (visible in gutter) |
| **`'` (Quote)** | **Next Bookmark** | Jump to next bookmarked line |
| **`D` / `d`** | **Line Diff** | Side-by-side split screen diff between 2 lines |
| **`C` (Shift+C)** | **Trace Correlation** | Discover `trace_id`, `request_id`, UUIDs & filter transaction |
| **`ESC` / `9` / `m`** | **Top Menu Bar** | Norton Commander menu bar |
| **`QQ` / `q` twice** | **Quit Flik** | Safe double-tap quit (or capital `Q`) |
| **`1` / `h` / `?`** | Help | In-app keyboard shortcuts guide |
| **`2` / `o`** | Auto-Detect Format | Detect JSON, Apache, K8s, Syslog |
| **`3` / `+` / `i`** | Add INCLUDE Filter | Filter by term, regex, or expression (`amount > 100`, `status:5xx`) |
| **`4` / `Tab`** | Switch View | Toggle **Compact Line** <-> **Columns Table** |
| **`5` / `t`** | Follow Mode | Toggle live `tail -F` |
| **`6` / `f`** | Field Selector | Pick visible columns |
| **`7` / `/`** | Quick Search | Search keyword in current view |
| **`8` / `-` / `e`** | Add EXCLUDE Filter | Filter out patterns |
| **`v`** | Field Values Inspector | Unique distinct values & percentages for any field |
| **`y`** | Yank Line | Copy current line to clipboard (`pbcopy` / OSC 52) |
| **`M`** | Toggle Mouse Capture | Enable/disable mouse capture for native text selection |
| **`x` / `X`** | Undo / Clear Filter | `x` pops last filter; `X` clears all filters |
| **`Ctrl+F`** | Filter Manager | Enable/disable/delete filter rules |
| **`Ctrl+S`** | Export | Export to CSV, JSON Lines, or Raw |
| **`Up` / `Down` / `k` / `j`** | Navigation | Scroll line-by-line |
| **`Space` / `f` / `PgDn`** | Page Down | Paging down |
| **`b` / `PgUp`** | Page Up | Paging up |
| **`g` / `Home`** | Jump Start | Jump to line #1 |
| **`G` / `End`** | Jump End | Jump to last line |

---

## Investigation workflows

The **Investigate** menu and **Ctrl+K** command dialog provide the six workflows below. Type a command and press Enter. Up/Down recalls command history; Ctrl+U clears the input. File paths can contain spaces without shell quoting.

| Workflow | Usage |
|---|---|
| Context around a filtered result | `Ctrl+G`, or `context 20` for 20 lines before/after |
| Boolean filters | Add an include/exclude filter using `NOT`, `AND`, `OR` and parentheses |
| Time navigation | `Ctrl+T`, then `time START..END`, `jump TIME`, or `around 5` |
| Chronology and cross-file traces | `Ctrl+L` / `merge`; `Ctrl+R` / `trace req-42` |
| Saved views and sessions | `Ctrl+P` / `save investigation.json`; `load investigation.json` |
| Redacted incident bundle | `Ctrl+E` / `export incident.jsonl`; optional `redact email,customer_id` |

### Context without losing the events around an error

Filter first, select a result, then press Ctrl+G. The context view reads the original file, so filtered-out neighbors remain visible in gray (`·`); matching lines carry `*`. Use Up/Down, Page Up/Down and Left/Right to browse, or `n`/`p` to move the context window to the next/previous filtered result. Esc returns to the panes with their filters intact. Enter reveals an event in its original pane; if that event was hidden, this action clears that pane's filters, as indicated in the footer. `context N` supports 0–1000 lines on each side.

### Composable include/exclude filters

In the normal Add Filter dialog (3/+ or 8/-), leave regex mode disabled and enter, for example:

```text
(level:error OR status>=500) AND service:payment AND NOT path:/health
"connection reset" OR "upstream timeout"
```

Precedence is `NOT`, then `AND`, then `OR`. Parentheses override it. Operators are uppercase; quoting makes operator words and parentheses literal. Terms retain the existing smart filters and numeric comparisons. Regex mode continues to accept regex syntax separately. Invalid expressions report an error without adding a rule.

### Time ranges, jumps and incident windows

In Ctrl+K:

```text
time 2026-10-21T10:00:00Z..2026-10-21T10:15:00Z
jump 2026-10-21T12:07:00+02:00
around 5
untime
```

Ranges are inclusive and apply to the active pane; `around 5` uses the selected event's timestamp ±5 minutes. `untime` removes only the time constraint. `jump` finds the nearest matching timestamp in the active file, even when timestamps are out of order. Use RFC3339 with an explicit timezone for command input. Log timestamps use the existing ISO/Apache extractor; timezone-less ISO log timestamps are interpreted as UTC, and unrecognized timestamps cannot participate in a range/jump.

### Merged chronology and correlation

`merge` collects matching events from both panes into a chronological **snapshot**, with filename and original line number on every row. Each pane's existing filters remain in effect. Events without recognized timestamps appear at the end, with a count in the title. Enter reveals a result in its source pane; `c` opens its context; Esc returns to the split; `r` rebuilds the snapshot to include new events.

`trace ID` adds a literal correlation-ID filter to both panes and opens the merged chronology. `trace` without an ID discovers the first non-IP identifier on the selected event. Existing filters remain active; clear them first if they hide parts of a request. This is an event chronology, not a reconstructed distributed span graph.

Merge, timestamp search and incident export run in the background with a scanned-line counter; Esc cancels the current job. Merged snapshots are limited to 500,000 matching events or 256 MiB of event text and report an error when that limit is exceeded; refine the filters or time range. A timestamp jump scans incrementally without retaining the complete file. Context and merge require initial file indexing to finish.

### Saved sessions and views

`save PATH.json` stores both file paths, the active pane, enabled/disabled filters, selected fields, forced format, line/table mode, scrolling, selection, bookmarks, reverse order, indentation, theme, follow settings, search text, command history and extra redaction keys. `load PATH.json` prepares all files and rules before replacing the current workspace: a missing file or invalid rule preserves your current session.

Session files are written atomically with private permissions. Saving may replace an existing valid Flik session, but refuses logs or arbitrary files. Sessions store source paths and filter text, not copies of the logs. Keep the source files available; temporary stdin streams disappear when their original process exits. Positions/bookmarks refer to the original line numbering and may need adjustment if the logs changed. A saved session can be reused as a project view.

### Exporting an incident safely

`export PATH.jsonl` exports the current context/merged investigation, or the filtered results from both files if no investigation view is open. It adds the configured before/after context, deduplicates overlapping windows, and writes a metadata record followed by event records containing `source`, original `line` (one-based), `match`, `timestamp` and `text`. Events in the bundle are grouped by source and original line; timestamps allow downstream chronological sorting. Source paths and filter descriptions are included.

Built-in redaction masks password/secret/token/API-key/authorization/cookie keys, including nested JSON, root arrays, embedded container JSON, text assignments and authorization headers. `redact email,customer_id` adds custom keys; `redact default` resets custom keys. Redaction is based on keys and known header patterns; arbitrary secrets in free text or unnamed personal data require review before sharing.

Export publication is atomic and refuses existing destinations. Cancellation, unreadable lines or detected source replacement/truncation leave no partial destination. Files that change during a snapshot can require a refresh; this is not a filesystem-level transaction. Original log files are never redacted or rewritten.

Validation commands:

```bash
cargo test --all-targets
cargo build
python3 scripts/smoke_investigation.py
```

## Two-file split screen

```bash
./flik.sh api.log worker.log
./flik.sh -f api.log worker.log
```

Each pane has its own filters, search, fields, view mode, bookmarks, scrolling, exports and live follow setting. Commands act on the highlighted pane. Both files continue to be monitored while you work in either pane.

- `Ctrl+N`: add a second file, or replace the other pane when two are already open.
- `Ctrl+O`: open a file in the active pane.
- `Ctrl+]` or `F12`: switch panes. `Ctrl+Left` focuses the first pane; `Ctrl+Right` focuses the second when the terminal transmits those keys.
- Click anywhere in the other pane, including its title, to focus it. A pane-local menu closes on blur; unfinished filter input is kept and resumes when you return. Only the active pane shows its dialog. Global open-file/command dialogs must be completed or cancelled first.
- If mouse capture was disabled with `M`, press `M` again to restore clicks.
- `Ctrl+W`: close the active pane and expand the remaining pane.
- The **File** menu provides the same actions. `Tab` still switches line/table view.
- In the path dialog, use Enter to open, Esc to cancel, or Ctrl+U to clear. Absolute paths, relative paths and `~/` are supported. A failed open preserves existing sessions.

Wide terminals show the files side by side; narrower terminals stack them vertically. Very small terminals show the active pane only. The current limit is two panes. `Q` / double `q` exits the whole application. Startup flags apply to both initial files; all files open at the end with automatic format detection and live follow enabled.

## 🐳 Docker Usage & Live Piping

### 1. Direct File Analysis
```bash
./flik.sh ../pizd/samples/api_batch1.json
```

### 2. Live Pipe from Docker Logs
```bash
docker logs -f my_container | ./flik.sh -
```

### 3. Continuous Stream Generation for `tail -F` Testing
Use the built-in generator script to simulate live traffic:
```bash
# Terminal 1: generate continuous mixed stream (Apache, K8s, JSON, Nginx)
./generate_live_logs.py live_stream.log

# Terminal 2: inspect live with auto-follow tail mode
./flik.sh -f live_stream.log
```

---

## ⚖️ How Flik Compares to Other Tools ("Et ils ont quoi les autres ?")

| Feature | **Flik** ⚡ | **lnav** | **k9s** | **stern** | **less / bat** |
|---|---|---|---|---|---|
| **Interface Style** | **Norton Commander** (Menus, F-keys, popups, mouse) | Vim/modal (steep learning curve) | K8s terminal dashboard | CLI pipe output | Unix pager (CLI) |
| **RAM usage on 10GB+ files** | **Minimal** ($O(1)$ random access seek) | Low/Medium (SQLite index) | In-memory stream buffer | Memory buffer | Minimal |
| **Startup Time** | Bounded preview; benchmark required | 1-5s (indexes into SQLite) | Fast | Fast | Instant |
| **Dual View (Line vs Columns Table)**| **Yes** (1-key toggle `Tab`/`4`) | Line view only | Table only | Line view only | Line view only |
| **Instant Stats & Top 10 (`S`)** | **Yes** (Status codes, IPs, Endpoints, Latency) | No | No | No | No |
| **Timeline Histogram with Jump (`H`)** | **Yes** (Bar chart + Enter to jump to time) | Histogram only | No | No | No |
| **Pattern Clustering & Anomalies (`P`)** | **Yes** (Signature templates & <2% anomaly flag) | No | No | No | No |
| **Star Bookmarks & Jump (`B` / `'`)** | **Yes** (`★` in gutter + `'` cycle) | Bookmarks | No | No | Basic marks |
| **Side-by-Side Line Diff (`D`)** | **Yes** (Split-screen colored diff) | No | No | No | No |
| **Trace & Request ID Discovery (`C`)** | **Yes** (Extracts trace_id, request_id, UUID) | No | No | No | No |
| **JSON Key Coloration** | **Yes** (keys distinct from values) | Partial | None | None | Bat only |
| **Smart Filters (`status:5xx`, `level:error`)** | **Yes** (built-in field awareness) | SQL queries | Label filters | Regex pod filters | Regex only |
| **Stackable Filters** | **Yes** (Include/Exclude stack manager) | Basic filters | No | No | No |
| **Export to CSV / JSON Lines** | **Yes** (`Ctrl+S` dialog) | SQL export | No | Pipe redirect | Pipe redirect |
| **Live Follow (`tail -F`)** | **Yes** (`5`/`t`) | Yes | Yes (pod tail) | Yes (multi-pod) | `F` in less |

Smart find: type `//error` then Enter to show matching lines and the immediately preceding line from the file. Use `//10:error` for ten preceding lines or `//10,3:error` for ten before and three after (0–1000 per side). `/error` keeps the usual search. Context stays in file order, without duplicates, and follows incoming logs, including after-context arriving later. Context-only lines have a gray `·` marker; they are not counted as matches in reports.

### Field and value completion

In quick search, type `serv` then Tab to complete `service:`. Type `service:` then Tab to cycle through observed values, or `service:b` to narrow them. Ctrl+Space does the same in the Add Filter dialog (Tab still toggles include/exclude there). Suggestions use at most 250 lines at the start and 250 at the live edge, plus detected field names; they are sampled suggestions rather than an exhaustive index. Boolean prefixes such as `level:ERROR AND service:` are preserved.

### Filtered frequency and before/after comparison

After applying a filter, press **H** to count its matches by hour. In **Ctrl+K**, run:

```text
histogram minute
histogram hour
histogram day
compare 2026-10-25T12:00:00+02:00 15
compare files
```

Frequency scans the indexed active file in bounded batches in the background; it does not sample or retain all matching text. It groups by calendar minute/hour/day in **UTC**, includes the date, fills empty intervals for ranges below 10,000 buckets, and reports how many matches lack a recognized timestamp. ISO, Apache access and Apache error timestamps are recognized; timestamps without a timezone are interpreted as UTC. Reports are snapshots: close with Esc and press H again to refresh. Up/Down and Page Up/Page Down scroll; Esc cancels an in-progress report.

`compare TIME [MINUTES]` compares equal windows around the given RFC3339 timestamp in the active file (15 minutes per side by default). Before is `[TIME − duration, TIME)` and after is `[TIME, TIME + duration)`. `compare files` compares the first pane (A) with the second (B). Each source’s active filters apply; smart-find context is excluded. Results group normalized message patterns and show counts, changes in percentage points, and patterns newly present or disappeared **within the chosen snapshots**, sorted by absolute change in share. This reports changes, not a diagnosis of their cause. Up to 100 patterns are displayed; aggregation is limited to 100,000 distinct patterns. File comparisons use shares rather than assuming equal file durations. New appends are excluded; replacement/truncation invalidates the report.

The S dashboard analyzes up to the first 5,000 lines. Apache error logs show detected levels instead of empty HTTP status groups; select a level and press Enter to filter. Frequency and comparison reports scan all indexed lines matching their filters.
