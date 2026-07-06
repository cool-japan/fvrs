# Supported Archive Formats

FVRS supports a wide range of archive formats for viewing, extraction, and creation. Since the OxiARC migration, the entire archive stack in the **default build is 100% Pure Rust** (no C/C++/Fortran code, no `*-sys` compression crates). As of oxiarc 0.3.4 the backend is plain `oxiarc-archive` — the interim in-repo compat codecs are gone — and the whole implementation lives in a single file, `crates/fvrs-gui-egui/src/archive.rs`.

## Format Support Matrix

| Format | List | Extract | Create | Status |
|--------|------|---------|--------|--------|
| ZIP | ✅ | ✅ | ✅ | Full support (Shift_JIS/CP437 name recovery) |
| LHA/LZH | ✅ | ✅ | ✅ | Full support (creation with `-lh5-` compression) |
| TAR | ✅ | ✅ | ✅ | Full support (PAX long names) |
| TAR.GZ | ✅ | ✅ | ✅ | Full support |
| TAR.BZ2 | ✅ | ✅ | ✅ | Full support (real-bzip2 compatible) |
| GZ | ✅ | ✅ | ❌ | Single file decompression |
| 7Z | ✅ | ✅ | ❌ | Real list + CRC-verified extract |
| CAB | ✅ | ✅ | ❌ | Extract only |
| RAR | ✅* | ✅* | ❌ | *Only with the non-default `rar` cargo feature |

## Legend

- ✅ **Supported**: Feature is implemented and covered by the archive test suite
- ❌ **Not supported**: Feature is not available
- ✅* **Feature-gated**: Requires an opt-in cargo feature (see RAR below)

## Format Details

### ZIP
- **Library**: `oxiarc-archive` 0.3.4 (`ZipReader`/`ZipWriter`)
- **Extensions**: `.zip`, `.jar`, `.war`, `.ear`
- **Features**: List, extract, create. Entry-name recovery is handled upstream: oxiarc decodes EFS → UTF-8 → Shift_JIS → CP437 in both the central-directory and local-header paths, so Japanese (Shift_JIS) filenames stay distinct and extract correctly (verified against bsdtar- and Info-ZIP-created archives, including archive comments and UT/ux extra fields). Extraction paths are sanitized against zip-slip traversal; symlink entries are not materialized.
- **Use case**: General-purpose archiving

### LHA/LZH
- **Library**: `oxiarc-archive` 0.3.4 (`LzhReader`/`LzhWriter`)
- **Features**: List and extract for header levels 0–3 (including level-1 extension chains), `-lhd-` directory entries, `-lh0-` stored data, and `-lh1-`/`-lh4-`…`-lh7-` compressed data; CRC-16 verified. **Creation uses real `-lh5-` compression** (`LzhWriter::add_file_raw`) with level-2 headers, Shift_JIS names, and preserved source mtimes, falling back to `-lh0-` storage only when compression does not shrink the data. Unsupported methods are listed but skipped with a warning during extraction. Known limitation: `-lz4-`/`-pm0-` stored entries (LArc/PMarc legacy) list but do not extract in oxiarc 0.3.4.
- **Use case**: Legacy Japanese archive format

### TAR
- **Library**: `oxiarc-archive` 0.3.4 (`TarReader`/`TarWriter`)
- **Features**: List, extract, create. Names longer than 100 bytes are handled automatically by oxiarc as PAX `path` records (readable by GNU tar and bsdtar — long Japanese paths round-trip full-length and do not panic). File permissions and mtimes are preserved. Symlink entries are skipped on extraction by design (safety).
- **Use case**: Unix/Linux archiving

### TAR.GZ
- **Library**: `oxiarc-archive` gzip module (decompression) + `oxiarc-deflate` `GzipStreamEncoder` (creation, level 6)
- **Extensions**: `.tar.gz`, `.tgz`
- **Features**: Full support
- **Use case**: Common Unix/Linux compressed archives

### TAR.BZ2
- **Library**: `oxiarc-archive` 0.3.4 (`oxiarc_archive::bzip2` + TAR)
- **Extensions**: `.tar.bz2`, `.tbz2`
- **Features**: Full support in both directions via upstream oxiarc-bzip2 0.3.4 (the earlier in-repo compat codec for 0.3.3 is gone). Validated against real libbz2 both ways: FVRS-created archives pass `bzip2 -t` and extract byte-exact with bsdtar, and real-bzip2-compressed tarballs list and extract byte-exact in FVRS.
- **Use case**: High-compression Unix/Linux archives

### GZ
- **Library**: `oxiarc-archive` (`GzipReader`)
- **Features**: Single-file decompression. The original filename stored in the gzip header is used for the output name when present (falling back to the archive's stem). Creation of standalone `.gz` files is not supported.
- **Use case**: Individual file decompression

### 7Z
- **Library**: `oxiarc-archive` 0.3.4 (`SevenZReader`)
- **Codecs**: Copy / LZMA / LZMA2 / BZip2 / Deflate / Delta / BCJ (x86); BCJ2 is rejected with a clear error
- **Features**: Real listing (substream sizes, empty files, directories, anti-items, encoded headers — **no temporary-directory side effects**) and CRC-verified extraction, including 0-byte entries; verified byte-exact against real liblzma-produced LZMA folders. Per-entry compressed sizes in the listing are approximate (folder-level attribution). Creation is not supported.
- **Use case**: High-compression archiving

### CAB
- **Library**: `oxiarc-archive` (`CabReader`)
- **Features**: List and extract with path sanitization. Creation is not supported (oxiarc has no CAB writer).
- **Use case**: Windows cabinet files

### RAR (feature-gated, non-default)
- **Library**: `unrar` crate (FFI bindings to the C++ unrar library)
- **Features**: List and extract with path sanitization, **only when built with `cargo build --features rar`**. Default builds exclude the C++ dependency entirely and return a graceful error explaining how to enable RAR support. Creation is restricted by the RAR license and is not supported.
- **Use case**: Popular proprietary archive format

## Implementation Notes

### Pure Rust policy

The default feature set contains no C, C++, or Fortran code in the archive stack: `zip`, `flate2` (as an archive dependency), `tar`, `bzip2`, `sevenz-rust`, `cab`, `delharc`, and `unrar` were all replaced by the `oxiarc-*` family (all 10 crates at 0.3.4). The interim in-repo compat codecs written against oxiarc 0.3.3 (7z/LZMA reader, bzip2 codec, LZH parser/LZHUF decoder, ZIP name recovery — 7 modules, 3,932 lines) were deleted once 0.3.4 covered them upstream. The only C++ dependency (`unrar`) is optional behind the non-default `rar` feature.

### Why some formats don't support creation

- **GZ**: Single-file container; FVRS creates `.tar.gz` instead
- **7Z**: oxiarc's 7z support is read-only (`SevenZReader`, no writer)
- **CAB**: No writer exists in oxiarc yet
- **RAR**: Creation is restricted by licensing terms

### Security

- All extraction paths are sanitized against path traversal (`../`, absolute paths, Windows drive prefixes, NUL) — covered by a zip-slip regression test
- TAR symlink entries are not materialized
- LZH and 7Z data are CRC-verified during extraction

### Error handling

Each format has descriptive, format-specific error messages with graceful degradation where possible (e.g. unsupported LZH compression methods are skipped with a warning rather than aborting the whole extraction).

## Testing

The workspace suite (24 tests, `cargo nextest run`, all green) includes 16 archive tests covering strict create → list → extract round-trips (name + size + byte-compare content, including Japanese filenames) for ZIP, TAR, TAR.GZ, TAR.BZ2, and LZH, plus: LZH `-lhd-`/`-lh1-`/Shift_JIS/`-lh2-`-skip fixtures, proof that LZH creation really compresses (`lzh_compresses_compressible_input`, a 128 KB compressible payload must shrink), 7Z byte-exact extraction of a real-liblzma LZMA-folder fixture and of empty file/dir substreams, 7z no-temp-dir-pollution, zip-slip regression, ZIP EFS Japanese-name round-trip, TAR/PAX long Japanese names, gzip header-filename handling, and the graceful RAR-disabled error. (The 14 unit tests of the deleted compat modules went with them: 37 → 24.)

## Future Plans

- CAB creation support
- 7Z creation
- LZH `-lz4-`/`-pm0-` extraction (pending upstream oxiarc support; they extracted under the deleted compat layer)
- Performance optimizations (streaming instead of whole-archive buffering for compressed TARs)

---

*Last updated: 2026-07-06*
*FVRS Version: 0.1.0*
