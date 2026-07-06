# FVRS — Master TODO

**Date: 2026-07-06**

## Current state

- ~8.4k lines of Rust (7.0k code SLoC per tokei) across 5 crates: `fvrs-core`, `fvrs-cli`, `fvrs-gui-egui` (live), `fvrs-plugin-api`, `fvrs-plugins` (both outside the workspace, do not build).
- **What works:** `fvrs-gui-egui` builds and runs on macOS (verified 2026-07-06): explorer tree + file list (Details/List/Grid), navigation history, address bar, text/hex viewer-editor, archive list/extract for 9 formats and create for 4, ~10 dialogs, one-touch A–Z shortcuts, status bar, state persistence.
- **What doesn't:** `fvrs-core` does not compile on non-Windows (un-gated `windows` imports); its file watcher never delivers events (watcher dropped); GUI copy/move never completes (no paste), sorting is silently overridden, ZIP/CAB/LZH extraction is zip-slip vulnerable, and ~1,900 lines across 10 GUI files are dead never-compiled NWG-prototype code. 11 banned archive deps (COOLJAPAN OxiARC policy), 18 production `unwrap()`, zero tests.
- **Standing GUI rule (applies to every P2 item):** a GUI feature is only checked off after launching the app (`cargo run -p fvrs-gui-egui`), exercising the feature, capturing a screenshot (`screencapture -x` on macOS), and visually confirming it. Note: the terminal must be granted Screen Recording permission (System Settings → Privacy & Security), the 2026-07-06 baseline capture failed on that TCC denial.

---

## P0 — OxiARC full adoption

Replace the entire archive stack with `oxiarc-archive` (0.3.3 on crates.io; bump to 0.3.4 when published), optionally plus `oxiarc-deflate` for streaming gzip Read/Write adapters. Hoist both into `[workspace.dependencies]`. Unless noted, work happens in `crates/fvrs-gui-egui/src/archive.rs` (821 lines, `ArchiveHandler`).

### fvrs-gui-egui dependency replacements

- [x] Remove `zip = "2.1"` (`crates/fvrs-gui-egui/Cargo.toml`) → `oxiarc_archive::zip::ZipReader::new(file)?` — list via `.entries() -> &[Entry]` (replaces the `by_index` loop at archive.rs:97), extract via `.extract(&entry)` writing to `entry.sanitized_name()` (**fixes the zip-slip traversal at archive.rs:459**); create via `ZipWriter::new(file)` (`Write`-only, no `Seek` bound) with `add_file`/`add_directory`/`finish` replacing `start_file` + `io::copy` in `add_to_zip` (archive.rs:705-725 — this rewrite also removes 4 `unwrap()` calls there).
- [x] Remove `flate2 = "1.0"` (`crates/fvrs-gui-egui/Cargo.toml`) → gzip via oxiarc: `.tar.gz` list/extract with `gzip::GzipReader::new(file)?.decompress()?` → `Cursor` → `TarReader`, or streaming `oxiarc_deflate::GzipStreamDecoder` + `tar::TarStreamReader::next_entry()`; `.tar.gz` create with `TarWriter::new(oxiarc_deflate::GzipStreamEncoder::new(file, 6))` (level 6 == flate2 default) or buffer route `gzip::compress(&tar_bytes, 6)`; single `.gz` via `gzip::decompress` — and use `GzipReader::header().filename` for the output name instead of the current file-stem guess (archive.rs:555).
- [x] Remove `tar = "0.4"` (`crates/fvrs-gui-egui/Cargo.toml`) → `oxiarc_archive::tar::TarReader::new(file)?` — `.entries()` kills the copy-to-sink listing hack (archive.rs:174); extraction is a loop over `entries()` with `.extract(&entry, &mut File::create(out)?)` and `entry.sanitized_name()` (no one-shot `unpack`, but 8 KB chunked streaming); create via `TarWriter` `add_file`/`add_directory` (recurse directories manually, same pattern as the existing `add_to_zip` helper; `add_entry_from_header(&TarHeader, data)` when mtime/mode must be preserved).
- [x] Remove `bzip2 = "0.4"` (`crates/fvrs-gui-egui/Cargo.toml`; links C libbz2) → `.tar.bz2` list/extract via `oxiarc_archive::bzip2::decompress_reader(file)?` → `Cursor` → `TarReader`; create via `TarWriter::new(Vec::new())` → `into_inner()` → `bzip2::compress_with_level(&tar_bytes, 9)` → `fs::write` (oxiarc's bzip2 encoder is block-API, buffer route is the clean mapping).
- [x] Remove `sevenz-rust = "0.6"` (deprecated upstream; `crates/fvrs-gui-egui/Cargo.toml`) → `oxiarc_archive::sevenz::SevenZReader::new(file)?` — `.entries()` gives a **real 7z listing**; delete the fake `list_7z_contents` that extracts the whole archive into `std::env::temp_dir()` as a "listing" side effect (archive.rs:329-346); extract via `.extract(index) -> Vec<u8>`.
- [x] Remove `cab = "0.6"` (`crates/fvrs-gui-egui/Cargo.toml`) → `oxiarc_archive::cab::CabReader::new(file)?` — `.entries()` flattens the folder/file double loop (archive.rs:390) and a single reader instance kills the double-open borrow workaround (archive.rs:627); extract via `.extract(&entry)`. CAB creation stays unsupported (oxiarc has no CabWriter; FVRS already returns an error at archive.rs:684 — keep it).
- [x] Remove `delharc = "0.6"` (`crates/fvrs-gui-egui/Cargo.toml`) → `oxiarc_archive::lzh::LzhReader::new(file)?` — `.entries()` replaces the `next_file()` loop (archive.rs:139); `.extract(&entry, &mut writer)` verifies CRC internally (archive.rs:482, also zip-slip-guard the output path). Bonus (optional): `LzhWriter` makes LZH **creation** possible, replacing the polite error stub at archive.rs:682.
- [x] **RAR decision** (`unrar = "0.5"`, `crates/fvrs-gui-egui/Cargo.toml`): no pure-Rust path exists — oxiarc-archive has no RAR module and `unrar` links the C++ unrar library. Per Pure Rust policy: make `unrar` `optional = true` behind a **non-default** cargo feature `rar` and cfg-gate the `ArchiveType::Rar` arms (archive.rs:349-386 list, :581-623 extract), or drop RAR entirely. The default build must be 100% C/C++-free. RAR creation was never supported, nothing else lost.

### fvrs-core dead dependencies

- [x] Delete `flate2`, `tar`, `xz2` from `crates/fvrs-core/Cargo.toml` — all three declared but referenced nowhere in code; `xz2` is additionally a C-FFI liblzma binding. If xz support is ever needed: `oxiarc_archive::xz::XzReader`/`XzWriter`.
- [x] Delete `aes` and `cipher` from `crates/fvrs-core/Cargo.toml` plus the commented `// use aes;` `// use cipher;` at `crates/fvrs-core/src/lib.rs:41-42` and the commented `# zip`/`# bzip2` Cargo.toml remnants. **ZIP-AES decision:** no separate crypto crates needed — oxiarc's `zip` module ships WinZip AE-2 AES-256 built in (`ZipWriter::add_encrypted_file`, `ZipReader::extract_encrypted` auto-detecting AES vs ZipCrypto); use that if/when an encryption UI is added.

### Acceptance criteria

- [ ] `cargo tree` (default features, whole workspace) shows **none** of: zip, flate2, tar, xz2, bzip2, sevenz-rust, cab, delharc, unrar, zstd, lz4, snap, brotli, miniz_oxide.
  - Residual (2026-07-06): `flate2 v1.1.9` + `miniz_oxide v0.8.9` (both pure Rust) remain solely via the GUI image stack — `png`/`tiff` ← `image` ← `arboard`/`eframe` — for window icons/clipboard image decoding, unrelated to archive handling; removing them requires dropping or de-featuring eframe/arboard image support. Every archive-stack banned crate is out of the default build.
- [x] `cargo tree -i` finds no `*-sys` compression crates; default build compiles with zero C/C++ build scripts. (blake3 `pure` feature enabled in `[workspace.dependencies]` 2026-07-06: `find target/debug/build -name "*.a"` no longer matches libblake3_neon.a and the blake3 build-script out dirs are empty — no cc invocation; digests unchanged, hash tests green.)
- [x] All archive tests pass: new round-trip tests (create → list → extract, per supported format, in `std::env::temp_dir()`) plus a zip-slip regression test using an archive containing a `../evil` entry that must land inside the extraction dir.
- [x] No 7z "listing" writes anything to the temp dir anymore.

---

## P1 — COOLJAPAN policy compliance

### Build & correctness prerequisites

- [x] Fix the fvrs-core non-Windows build (fixed 2026-07-06; confirmed broken by `cargo check -p fvrs-core`): cfg-gate `use std::os::windows::ffi::OsStrExt` and the other `windows` imports (`crates/fvrs-core/src/lib.rs:31-32,39`); add `use std::os::unix::fs::PermissionsExt` inside the `cfg(unix)` branches and make the permissions binding `mut` (lib.rs:206, 235, 249).
- [x] Fix the fatally broken watcher (done 2026-07-06): `RecommendedWatcher` now stored inside `FileSystem` (`fs.rs: watcher: Option<RecommendedWatcher>`), events flow over a `tokio::sync::mpsc` unbounded channel with async `next_event(&mut self)`, non-blocking `try_next_event`, and `stop_watching`; `event.paths.first()` guards replace the `paths[0]` panic (monitor.rs); CLI monitor loop migrated from 100 ms polling to async recv; verified by the new `watcher_delivers_events` integration test. (Follow-up resolved 2026-07-06: bounded channel landed — capacity 4096, notify callback uses `try_send` and drops overflow events with a `tracing::warn!`; public API `next_event`/`try_next_event`/`stop_watching` unchanged.)

### Workspace policy

- [ ] Add `crates/fvrs-plugin-api` and `crates/fvrs-plugins` to root `[workspace] members` (partial: DECISION deferred — both crates left orphaned outside the workspace pending the P3 plugin-system decision; neither added nor deleted on 2026-07-06).
- [x] Convert package metadata to inheritance (done 2026-07-06): all three member crates (`fvrs-core`, `fvrs-cli`, `fvrs-gui-egui`) now inherit `version/edition/rust-version/authors/license/repository/description` from `[workspace.package]`. `fvrs-plugin-api` left untouched — it is a non-member and cannot inherit until the P3 plugin decision lands.
- [x] Unify edition (done 2026-07-06). **DECISION: the whole workspace moved to edition 2024** — `[workspace.package]` sets `edition = "2024"` + `rust-version = "1.85"`, and every member uses `edition.workspace = true`; no mismatch remains.
- [x] Hoist all inline dep versions to `[workspace.dependencies]` (done 2026-07-06): every dep of the three member crates is now `{ workspace = true }`; tokio unified at workspace 1.52 (the gui `"1.0"` / cli `"1.36"` divergence is gone). Last inline holdout `unrar` hoisted to `[workspace.dependencies]` 2026-07-06 (gui references it as `{ workspace = true, optional = true }` behind the still-non-default `rar` feature); zero inline dep versions remain.
- [x] `windows` dep dedup + cfg-gating (done 2026-07-06): single workspace entry at 0.62.2, consumed only by fvrs-core under `[target.'cfg(windows)'.dependencies]`; unused gui `windows = "0.61"` and `notify = "6.1"` deleted (gui only uses std MetadataExt).
- [x] Remove unused `[workspace.dependencies]` entries `slint` and `fluent-bundle` (done 2026-07-06). **DECISION: removed as dead weight — this does not close the i18n question; the fluent-vs-ja-only decision stays open in P3**, and fluent-bundle can be re-added if that decision picks it.

### Latest crates upgrades (current → latest per 2026-07 audit)

- [x] eframe/egui/egui_extras 0.31 → **0.35.0 landed** (done 2026-07-06): migration confined to `ui/app_shell.rs` — `App::update(ctx)` → new `App::ui(&mut Ui)`, panels → unified `egui::Panel::{top,bottom,left,right}` (status bar moved before CentralPanel, fixing its previous invisibility), `egui::menu::bar` → `egui::MenuBar`, 81× `ui.close_menu()` → `ui.close()`; eframe keeps `default-features = false` + `default_fonts/glow/wayland/x11`; `cargo tree` shows a single egui 0.35.0; GUI smoke-verified live (fonts, tree nav, hex viewer).
- [x] rfd 0.14 → **0.17.2 landed** (workspace entry, 2026-07-06).
- [x] notify 6.1 → **8.2.0 landed** in fvrs-core via workspace; deleted from gui as unused (2026-07-06).
- [x] thiserror 1.0 → **2.0.18 landed** (root `[workspace.dependencies]`, 2026-07-06; a transitive thiserror 1.x remains in the lock from third-party deps, not ours).
- [x] windows 0.52/0.61 → **0.62.2 landed**, single workspace entry, cfg(windows)-gated in fvrs-core only (2026-07-06).
- [x] sha2 0.10 → **0.11.0 landed**; whole RustCrypto digest family bumped alongside: sha1 0.11.0, md-5 0.11.0, ripemd 0.2.0, blake3 1.8.5, plus walkdir 2.5 / regex 1.12 / open 5.3 (2026-07-06).
- [x] libloading 0.8 → **0.9 landed as workspace entry only** (2026-07-06); no member consumes it yet — kept solely for the pending P3 plugin decision (the 0.8.x in Cargo.lock is transitive via the GUI stack).
- [x] Replace unmaintained `md5 = "0.7"` (done 2026-07-06). **DECISION: switched to RustCrypto `md-5` 0.11** (`Md5` via the `Digest` trait, including the directory-hash arm); MD5 kept in `HashAlgorithm` for interop, BLAKE3/SHA-256 remain the promoted defaults; known-vector round-trip test proves digests unchanged.

### No-unwrap cleanup (18 production `unwrap()` + 1 `expect()`, zero test code exists)

- [x] fvrs-core (3) — done 2026-07-06: `duration_since().unwrap()` → `Duration::ZERO` fallback, both `strip_prefix().unwrap()` in `compare_directories` → mapped to `FsError::Comparison`. Grep-audited: zero `unwrap()`/`expect()` in fvrs-core src.
- [x] fvrs-gui-egui live code — done 2026-07-06: the dead tokio `Runtime` + `FileSystem` fields were removed outright (killing the `expect`), cache `.get()` got a fallback, `lock().unwrap()` and `parent().unwrap()` replaced with if-let, and the four `archive.rs` `file_name().unwrap()`s died with the P0 rewrite. Grep-audited: zero `unwrap()`/`expect()` across all three crates' production src.
- [x] The remaining 8 unwraps lived in dead NWG files — gone with the dead-code deletion below (2026-07-06).

### Dead code & no-warnings

- [x] Delete the orphaned NWG-prototype files (done 2026-07-06): all 11 files (~2,150 lines incl. `file_ops.rs`) purged — `preview.rs`, `search.rs`, `filter.rs`, `clipboard.rs`, `drag_drop.rs`, `menu.rs`, `plugin_manager.rs`, `plugin_dialog.rs`, `plugin_watcher.rs`, `ui/main.rs`, `file_ops.rs`; each verified as a true orphan at HEAD (never in any module tree). Wanted behavior gets fresh egui reimplementations per P2.
- [x] Delete the orphaned `crates/fvrs-core/src/core/mod.rs` (done 2026-07-06): deleted after porting recursive directory copy into the live `FileSystem::copy` (iterative stack over `tokio::fs`, no longer `NotSupported`); covered by a new recursive-copy integration test.
- [x] Reconcile `lib.rs` vs `main.rs` (done 2026-07-06): the `eframe::App` impl moved into the lib (`ui/app_shell.rs`), `main.rs` is now a 19-line thin launcher over `fvrs_gui_egui` — double compilation ended; `lib.rs` still exposes `pub mod archive` (required by tests/archive_roundtrip.rs); `file_ops.rs` dropped with the purge above. **DECISION note:** the unused `FileOperation` variants (`Move`/`Copy`/`Rename`/`CreateFolder`) and the unread `path` field were **removed** rather than wired up — only `Delete` is ever constructed today; the payloads get redesigned with the P2 copy/move engine.
- [x] Zero warnings across the workspace (verified 2026-07-06): `cargo check --workspace --all-targets` = 0 errors / 0 warnings on default features (all 7 gui dead-code + 3 core unused-import warnings cleared). (Residual cleared 2026-07-06: the 2 rar-gated `useless_conversion` warnings removed; `cargo clippy --workspace --all-targets --all-features` is fully clean.)

### File-size policy (2,000-line limit, splitrs)

- [x] Split `crates/fvrs-core/src/lib.rs` (done 2026-07-06): the 1,289-line inline `mod core` is now 10 focused modules — `error.rs` (39), `fs.rs` (160), `permissions.rs` (150), `search.rs` (145), `hash.rs` (163), `compare.rs` (244), `monitor.rs` (314), `config.rs` (23), `plugin.rs` (13), `lib.rs` (109, incl. a `pub mod core` facade preserving every historical `fvrs_core::core::*` path — fvrs-cli/gui/tests compile unchanged). The six copy-pasted per-algorithm hash loops were deduplicated into one generic `StreamHasher` abstraction (`DigestHasher<D: Digest>` + `Blake3StreamHasher` via `HashAlgorithm::new_hasher()`), digest-identity proven by known-vector tests. Note: `splitrs` was tried first (two dry-runs) but could not produce this domain layout, so the split was finished by hand along the same lines. All files <2000 lines; re-check with `rslines 50` after P2 growth.

### Hardcoded paths & portability

- [x] Remove hardcoded Windows paths (done 2026-07-06): all three sites replaced by dependency-free runtime mount enumeration in `src/utils.rs` (`available_mounts()`/`mount_label()`: Windows A:–Z: probe, macOS `/` + `/Volumes`, Linux `/` + `/mnt` + `/media` + `/run/media/<user>`); `state.rs` default path falls back cwd → home → first enumerated mount → root; explorer tree iterates the enumerated mounts. (Follow-up resolved 2026-07-06: TTL mount cache landed — `MountCache` in `src/utils.rs` (3 s TTL) held on `FileVisorApp`; the explorer tree no longer stats volumes every frame.)
- [x] Cross-platform CJK fonts: `src/utils.rs` `setup_japanese_fonts()` probes only `C:/Windows/Fonts/*`; add macOS (Hiragino) and Linux (Noto CJK) probes so the Japanese UI stops rendering as tofu off-Windows.
- [x] Replace deprecated `std::env::home_dir()` — resolved 2026-07-06 by bumping `[workspace.package]` rust-version to 1.87, where `std::env::home_dir()` is un-deprecated (fixed behavior); the call sites at `src/state.rs:122` and `src/ui/app_shell.rs:119,388` are warning-free on the declared MSRV.

---

## P2 — FileVisor parity (GUI)

**Standing rule (screenshot verification protocol):** every item in this section may only be flipped to `[x]` after (1) `cargo run -p fvrs-gui-egui`, (2) exercising the feature by hand, (3) capturing a screenshot — `screencapture -x <file>.png` on macOS — and (4) visually confirming the result in the image. Screen Recording permission must be granted to the terminal first (the 2026-07-06 baseline attempt failed with "could not create image from display" / TCC denial). Pre-existing `[x]` marks below come from the code survey and should be re-confirmed by screenshot once capture works.

### Signature features (FileVisor's identity — do these first)

- [ ] One-key A–Z command operation (partial: ~16 keys work — C/D/E/I/K/L/M/N/O/Q/R/S/T/V/X/Z in `src/ui/shortcuts.rs` — but shortcuts fire while typing in the search/address bar; add a `ctx.wants_keyboard_input()` guard at shortcuts.rs:23-155 and remove the duplicated un-gated U/P/V/R handling at :158-183 that makes Ctrl+V open the archive viewer)
- [ ] Copy/Move via destination dialog (partial: C/M only set `state.clipboard`, which is never consumed — no paste exists; implement a destination dialog with history dropdown + an actual copy/move engine with recursive dir copy and cross-device fallback, ideally on fvrs-core)
- [ ] Multi-mode batch rename — sequential numbering, substring replace, case conversion, extension change (missing; single rename dialog exists — the feature reviewers remember FileVisor for)
- [ ] Tab bar: one folder per tab, Ctrl+Tab switching, close-others, reorder, reopen-closed, lock, detach (missing; Window-menu items are no-ops in `src/main.rs`)
- [ ] Tab groups: save/restore named tab sessions (missing; depends on tabs)
- [ ] Bookmark panel (しおり): register folders, open in current tab/new tab/new window, reorder, inline rename (missing)
- [ ] Wildcard/regex filter on the file list with mass-select (missing; dead `filter.rs` prototype was deleted in P1 — reimplement in egui)
- [ ] Search Finder: incremental filename extraction/highlight as you type, Space/Shift+Space next/prev, Enter opens (missing; note the toolbar search box currently only triggers the shortcut bug)
- [ ] Grep (search string in files) with `<file>:<line>:<text>` results and tag-jump into the editor (missing; Tools-menu item is a no-op)
- [ ] Browse archives like folders: open members in associated apps, text/image preview without manual extraction (partial: archive viewer window lists entries on double-click; no member open/preview; 7z listing fake and entry sizes unformatted — P0 fixes the listing)
- [ ] Archive create/extract UX parity: member-selective extraction, destination history, integrity check (partial: U/P dialogs exist for full extract + ZIP/TAR/TGZ create; pack dialog omits TAR.BZ2 although the backend supports it; zip-slip fix lands in P0)
- [x] Built-in text viewer / editor (V=view, E=edit, Ctrl+S save, modified indicator — `src/ui/file_viewer.rs`) — follow-ups: [ ] surface open/save errors as dialogs (4 `TODO` sites at file_viewer.rs:168,191,215,279), [ ] sync line-number scrolling, [ ] stop loading whole files into RAM (chunked/hex-paged views)
- [ ] Binary (hex) editor (partial: hex *viewer* exists in file_viewer.rs; no editing, whole file loaded into one String)
- [ ] Customizable keymap: assign any command to A–Z/0–9/F1–F12 with modifiers (missing; keys are hardcoded in shortcuts.rs)
- [ ] Folder synchronization with regex filters and saved definition sets + batch sync lists (missing; fvrs-core `compare_directories` is a starting point once its bugs are fixed)
- [ ] Object Panel launcher with group tabs (missing)
- [ ] Smart bar compact launcher row (missing)
- [x] Explorer-superset layout: folder tree + single file list pane (`src/ui/explorer_tree.rs` + `src/ui/file_list.rs`, Tab pane switching, active-pane highlight) — follow-up: [ ] tree Up/Down keyboard navigation is a TODO stub (explorer_tree.rs:244-252) and drives are hardcoded (P1)

### Window & layout

- [x] Status bar: path, folder/file counts, selected count (`src/main.rs`) — follow-up: [ ] stop calling `load_directory` + cloning the entry Vec every frame (main.rs:479-485)
- [ ] Multiple top-level windows + tile/cascade + save/restore positions (missing; menu no-ops)
- [ ] Configurable panels/bars: toolbar, drive bar, navi bar, function-key bar show/hide (partial: toolbar exists; View-menu toggles are no-ops)

### Navigation

- [ ] Navi bar with per-character autocomplete and recent-folder dropdown (partial: address bar with Enter-to-navigate exists at main.rs:249-301; no autocomplete/history dropdown)
- [ ] Drive bar with one-click switching and drop-to-copy (missing; replace the hardcoded C:–H: tree entries with enumerated mounts)
- [ ] Folder history menu with clear command (partial: 100-entry back/forward history works with Alt+arrows; no jump menu)
- [x] Home / up / back / forward / refresh with enabled-state logic (`src/app.rs`, toolbar)
- [ ] Open specified folder / system-folders submenu (menu no-ops)

### File list & display

- [ ] View modes incl. thumbnails (partial: Details/List/Grid work in `src/ui/file_list.rs`; thumbnail view missing — `_thumbnail_cache` is dead; add selectable thumbnail sizes)
- [ ] Sorting by name/ext/size/time/attr, asc/desc, numeric-aware option (partial: sort logic + header buttons + S-key cycle exist, but `src/main.rs:384-391` re-sorts by name every frame, discarding the chosen order — **fix this first**, then add numeric-aware compare)
- [ ] Hidden/system display filtering (partial: dot-prefix only in app.rs:114-118 even on Windows; use FILE_ATTRIBUTE_HIDDEN like file_info.rs does, and wire fvrs-core's unused `show_hidden`/`SortBy` config)
- [ ] File info / recursive folder size / drive info views (partial: I-key file-info dialog exists, but PE version, owner, disk space, and associations are hardcoded fakes at `src/file_info.rs:269-346`; fix the wrong byte-count in `format_size` at :370; implement real recursive folder totals and drive capacity)
- [ ] Attribute-based coloring + per-pane font settings (missing)

### Selection & filtering

- [ ] Selection commands: select all / deselect / inverse (partial: click, Ctrl+click, Shift+range, Z=select-all work; select-all wrongly includes the ".." pseudo-entry — shortcuts.rs:289-300, which makes Z-then-D offer to delete the current dir; inverse selection missing; also fix the duplicate ".." rows from app.rs:98-108 vs main.rs:370-381)

### File operations

- [x] New file / new folder dialogs with name validation and auto-select (`src/ui/dialogs.rs`)
- [ ] Delete with recycle-bin routing + batch delete by filter + empty recycle bin (partial: permanent `fs::remove_dir_all` only, per-file errors merely logged; route through the OS trash and surface error dialogs)
- [x] Single rename (R key, dialog) — follow-up: [ ] fix the fires-on-any-Enter/IME bug at dialogs.rs:582 and the rarely-true focus condition at :568
- [ ] Attribute & timestamp batch editing (stub logging only, shortcuts.rs:367)
- [ ] File split & merge with reassembly script (missing)
- [ ] Hash calculation UI: MD5/SHA1/SHA256 (+BLAKE3 as an FVRS extension) for selected files (partial: fvrs-core has the full hashing engine; zero GUI wiring)
- [ ] Formatted list output to HTML/XML/CSV (missing; menu no-op)
- [ ] Shredder / secure erase (missing)
- [x] Copy full path to clipboard (T key via arboard)
- [ ] Background copy/move that keeps the UI responsive — improve on FileVisor with a serialized job queue (missing; do after the copy/move engine)

### Built-in tools

- [ ] Photo viewer with Exif pane (missing)
- [ ] Image editor (missing — deprioritized; decide scope in P3)
- [ ] Open terminal at current folder (menu no-op; easy win — `open -a Terminal` / `cmd /K cd` / `$SHELL` spawn)

### Keyboard & customization

- [ ] Hotkey menu (Ctrl+Shift+A–Z bindings) and customizable right-click shortcut menu (missing; no context menu exists at all after the dead menu.rs deletion — implement an egui context menu first)
- [ ] Resident tray mode with global hotkey (missing; platform-heavy — decide in P3)
- [ ] Startup options + window position save/restore (partial: whole AppState persists via eframe storage, but it wrongly persists dialog-open flags and stale archive state — main.rs:657-661; persist only layout/session fields) (note 2026-07-06: eframe's `persistence` feature had been OFF, so `cc.storage` was always `None` and save/restore silently never ran; feature now enabled in the workspace eframe entry — persistence, including the flag-persistence bug above, is now live behavior)

### Explicit non-goals (FileVisor scope facts)

FileVisor7 has **no** FTP client, no migemo, no macro/scripting engine, and no per-file color labels. Parity does not require them; each would be an FVRS extension to be argued separately (P3).

---

## P3 — Later / decisions needed

- [ ] **Plugin system direction (decision):** everything is scaffolding today — `fvrs-plugin-api` compiles only in isolation, `fvrs-plugins` is a cargo-new template, three divergent `Plugin` traits exist (fvrs-plugin-api, fvrs-core `plugin` module, and the phantom `info()/execute()` API the dead plugin_manager.rs assumed). Either (a) do it properly: add both crates to the workspace, unify on one trait, replace the FFI-unsafe `extern "C" fn() -> Box<dyn Plugin>` with a `#[no_mangle]` C-ABI factory + API-version handshake, use `std::env::consts::DLL_EXTENSION` (loader was .dll-only), ship a sample cdylib plugin in `fvrs-plugins`, write an egui management dialog, and add a notify-based hot-reload watcher — or (b) delete both crates and the fvrs-core `plugin` module until post-1.0. Decide before P1 workspace-members work.
- [ ] **CLI scope (decision):** `crates/fvrs-cli/src/main.rs` is 153 lines. Define real subcommands over fvrs-core (list/copy/move/hash/verify/compare/search/watch) so the core crate has a second consumer and a test surface, or park the crate explicitly.
- [ ] **i18n (decision):** all GUI strings are hardcoded Japanese; `fluent-bundle` sits unused in workspace deps. Either adopt fluent with ja + en bundles, or declare ja-only and remove the dep. (Cross-platform font loading is already in P1.)
- [ ] **README + SUPPORTED_FORMATS rewrite:** `README.md` currently describes a non-existent Slint/native-windows-gui layout — rewrite it for the actual egui application, real keybindings, and build instructions; update `SUPPORTED_FORMATS.md` to the post-OxiARC matrix (per format: list/extract/create, RAR behind the non-default `rar` feature). No new scattered .md files.
- [ ] **Tests & CI:** zero tests exist anywhere (the only `#[test]` is the template `it_works` in non-member fvrs-plugins). Add: fvrs-core unit tests (hash round-trip, verify, compare_files/directories, search filters, watcher event delivery) and the P0 archive round-trip suite — all using `std::env::temp_dir()`. CI policy allows only pypi-publish.yml/npm-publish.yml workflows, so codify a local gate instead: `cargo nextest run --all-features` + `cargo clippy -- -D warnings` before any release commit.
- [ ] **fvrs-core deep rework backlog** (post-P1 correctness, pre-parity): streaming chunked hashing via `tokio::fs` (currently blocking whole-file reads in async fns), fix `compare_binary` short-read desynchronization (`src/lib.rs:989-1020`) and report trailing bytes, redesign `calculate_directory_hash` (sort entries, include relative paths in the hash, one generic loop instead of six), record which side is missing in `compare_directories`, implement recursive directory copy + cross-device move fallback, unify the two duplicate `Config` structs and actually load/consume them, fix the extension-filter no-extension pass-through (lib.rs:719-725, :554-562) and unescaped regex in `find_files_by_extension` (:786), evaluate `MonitoringFilter` min_size/max_size/event_types in `matches()`.
- [ ] **GUI-on-core adoption (decision):** the GUI reimplements directory listing/FileEntry instead of using fvrs-core (`FileSystem`, watcher, hashing, compare). After the core rework, migrate the GUI onto fvrs-core so features like sync, hash UI, and a real file watcher (auto-refresh instead of the manual F5 cache) come from one engine.
- [ ] **FileVisor extension candidates (explicitly beyond parity, rank later):** migemo incremental search, per-file labels/colors, scripting hooks, serialized background job queue UI, SFTP — each needs a pure-Rust dependency audit before adoption.
