# Folder Explorer

A fast file explorer that shows the real size of every folder. Written in Rust with a [Slint](https://slint.dev) interface. Windows is the primary platform; macOS is supported as a native port (see [Platform support](#platform-support)).

Browse like you would in File Explorer, and every folder shows its true size, file count and folder count, so you can see at a glance what is filling your disk.

## Features

- **Folder sizes everywhere**: size, file count and folder count for every folder, filling in live as the scan runs
- **Views**: extra large, large, medium and small icons, list, details and tiles, with sizes shown in all of them
- **Thumbnails and real icons**: image and video thumbnails plus the same icons Windows shows, loaded in the background
- **File management**: new folder, inline rename, cut, copy and paste that work with File Explorer
- **Instant search**: searches the scanned tree in memory, with filters for size and for modified, created or accessed date
- **Summary pane**: see what fills a folder by file type or by age, then double-click a row to list those files
- **Checksums**: MD5, SHA-1, SHA-256 and SHA-512 columns
- **Size units**: automatic, or fixed bytes, KB, MB, GB or TB
- **Export**: save the list as comma, tab, semicolon, pipe or tilde separated text
- **File actions**: open, copy path, move to Recycle Bin, properties, show in File Explorer
- **Drive overview**: usage for every drive, plus a usage bar for the current drive
- **Honest totals**: folders that could not be fully read are marked with `≥`, and the unreadable items are listed
- **Light and dark mode** that follow Windows

## Performance

Measured with a 12-thread CPU and an NVMe SSD on a tree of 1.96 million files in 225,000 folders. Scan times are for a warm disk cache; the first scan after a restart is slower because it waits on the disk.

| | Time |
| --- | --- |
| Scan, parallel with a global lock per file (baseline) | 8.3 s |
| Scan, Folder Explorer | 2.7 s |
| Search all 1.96 million file names | 12 ms |

How it stays fast:

- Each scan thread builds its own part of the tree, so workers share no locks
- File metadata comes straight from the directory listing, with no extra call per file
- One scan feeds everything: sizes, search and summaries all read the same in-memory tree
- The list is virtual, so a million search results cost nothing until they scroll into view
- Icons and thumbnails load only for what is on screen, newest first, on their own threads
- Results from a scan are kept, so browsing into a scanned folder needs no rescan

Keeping the tree in memory has a cost: about 115 bytes per file, or 225 MB for 2 million files. Very large search results add more on top of that until you leave the results.

Sizes are logical file sizes. Files with several hard links are counted once per link. Junctions and symbolic links are listed but never followed, so nothing is counted twice.

## Download

Grab the latest `folder-explorer-*-windows-x64.zip` from the [Releases](https://github.com/KidiXDev/folder-explorer/releases) page and run `folder-explorer.exe`. Releases are built from `v*` tags.

## Build

You need [Rust](https://rustup.rs) 1.92 or newer. On Windows you also need the MSVC build tools.

```bash
cargo run --release
```

Open a specific folder at startup (Windows):

```bash
cargo run --release -- "C:\Users"
```

Open a specific folder at startup (macOS):

```bash
cargo run --release -- "/Users"
```

Run the tests (Windows only; see [Platform support](#platform-support)):

```bash
cargo test
```

## Platform support

The Windows build uses the Win32 APIs directly (shell icons and thumbnails,
Explorer-compatible clipboard, Recycle Bin, custom DWM window frame) and is the
full-featured target.

macOS is supported as a native port. All OS integration lives behind
`cfg(windows)` / `cfg(unix)`: the macOS side is in `src/platform_unix.rs` and
`src/icons_unix.rs`, with a shared metadata shim in `src/fsmeta.rs`. On macOS:

- Scanning, folder sizes, all views, search, filters, summaries, checksums and
  export work the same as on Windows.
- Drives list the root volume and everything under `/Volumes`; disk usage comes
  from `statfs`.
- File actions map to macOS: open and "reveal in Finder" via `open`, "Get Info"
  via Finder, move to Trash via the `trash` crate, and rename that refuses to
  overwrite. Dialogs are native (via `rfd`).
- Quick access lists your home folders (Desktop, Documents, Downloads,
  Pictures, Music, Movies).

Current limitations on macOS (first usable port):

- File and app icons and thumbnails fall back to generic folder/file glyphs.
- Cut / copy / paste of files works inside the app but does not interoperate
  with Finder.
- The window uses the app's own borderless title bar (no native traffic-light
  buttons).
- `cargo test` is Windows-only; the inline test modules use Win32 APIs.

## Keyboard shortcuts

| Keys | Action |
| --- | --- |
| Alt+Left, Alt+Right | Back, forward |
| Alt+Up, Backspace | Up one level |
| Enter | Open |
| Alt+Enter | Properties |
| Letters and digits | Jump to the next matching name |
| Menu key, Shift+F10 | Context menu |
| F5, Ctrl+R | Refresh |
| Esc | Stop scanning, or leave search results |
| Ctrl+F, Ctrl+E, F3 | Search |
| Ctrl+L, Alt+D, F4 | Edit the path |
| Ctrl+A | Select all |
| F2 | Rename |
| Ctrl+Shift+N | New folder |
| Ctrl+X, Ctrl+C, Ctrl+V | Cut, copy, paste |
| Ctrl+Shift+C | Copy path |
| Delete, Ctrl+D | Move to Recycle Bin |
| Shift+Delete | Delete permanently |

## Project layout

The crate is organized in layers (each depends only on the ones below it):
`util` → `platform` / `icons` → `model` → `app` → `ui`. Every module is kept
small (roughly ≤ 300 lines), with its tests in a sibling `tests.rs`.

| Path | Purpose |
| --- | --- |
| `src/util/` | Leaf helpers: `format`, `fsmeta` (cross-platform file metadata), `hash`, `names`, `export` |
| `src/platform/` | OS integration — `mod.rs` dispatcher over `windows/` and `unix/`, each split by concern (time, window, drives, shell/fileops, clipboard, dialogs) |
| `src/icons/` | Icon loading — `windows/` (shell icons + thumbnails: `source`, `image`, `store`) and `unix.rs` (stub) |
| `src/model/` | In-memory data, no UI: `scan/` (parallel scanner + cache), `query/` (search + summaries), `list/` (virtual list model), `item/` (rows + columns), `chart/` |
| `src/sidebar/` | Navigation pane: the tree plus pinned quick-access folders |
| `src/app/` | Application state and background jobs, one `impl App` split by concern (`navigation`, `scanning`, `search`, `summary`, `hashing`, `view`, `places`, `fileops`, `status`, `wiring`, `lifecycle`) |
| `ui/` | Slint interface and icons |

## Settings

Settings are stored in:

- Windows: `%APPDATA%\FolderExplorer\settings.ini`
- macOS: `~/Library/Application Support/FolderExplorer/settings.ini`

## License

[MIT](LICENSE)
