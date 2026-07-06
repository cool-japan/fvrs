# Supported Archive Formats

FVRS supports a wide range of archive formats for viewing, extraction, and creation. Since the OxiARC migration, the entire archive stack in the **default build is 100% Pure Rust** (no C/C++/Fortran code, no `*-sys` compression crates). The implementation lives in `crates/fvrs-gui-egui/src/archive.rs` and its `archive/` submodules.

## Format Support Matrix

| Format | List | Extract | Create | Status |
|--------|------|---------|--------|--------|
| ZIP | ✅ | ✅ | ✅ | Full support (Shift_JIS/CP437 name recovery) |
| LHA/LZH | ✅ | ✅ | ✅ | Full support (creation newly added) |
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
- **Library**: `oxiarc-archive` (`ZipReader`/`ZipWriter`) + in-repo name recovery (`archive/zip_names.rs`, `archive/name_codec.rs` with `encoding_rs`)
- **Extensions**: `.zip`, `.jar`, `.war`, `.ear`
- **Features**: List, extract, create. Entry names are decoded EFS → UTF-8 → Shift_JIS → CP437 from the raw central directory, so Japanese (Shift_JIS) filenames stay distinct and extract correctly. Extraction paths are sanitized against zip-slip traversal.
- **Use case**: General-purpose archiving

### LHA/LZH
- **Library**: In-repo header parser and writer (`archive/lzh.rs`, header levels 0–3) with `-lh1-` decoding via a faithful LZHUF implementation (`archive/lzhuf1.rs`) and `-lh4-`…`-lh7-` via `oxiarc-lzhuf`
- **Features**: List and extract including `-lhd-` directory entries and `-lh0-`/`-lz4-`/`-pm0-` stored data; CRC-16 verified. **Creation is now supported** via a level-2 header writer with Shift_JIS names (data stored as `-lh0-` until the upstream oxiarc lh5 encoder round-trip issue is fixed). Genuinely unsupported methods are skipped with a warning during extraction.
- **Use case**: Legacy Japanese archive format

### TAR
- **Library**: `oxiarc-archive` (`TarReader`/`TarWriter`) + in-repo PAX long-name handling
- **Features**: List, extract, create. Names longer than 99 bytes are written as PAX `path` records (readable by GNU tar, bsdtar, and oxiarc itself), with char-boundary-safe fallback names — Japanese long paths do not panic. File permissions and mtimes are preserved. Symlink entries are skipped on extraction by design (safety).
- **Use case**: Unix/Linux archiving

### TAR.GZ
- **Library**: `oxiarc-archive` gzip module (decompression) + `oxiarc-deflate` `GzipStreamEncoder` (creation, level 6)
- **Extensions**: `.tar.gz`, `.tgz`
- **Features**: Full support
- **Use case**: Common Unix/Linux compressed archives

### TAR.BZ2
- **Library**: `oxiarc-archive` TAR + in-repo spec-correct bzip2 decoder/encoder (`archive/bzip2_compat.rs`)
- **Extensions**: `.tar.bz2`, `.tbz2`
- **Features**: Full support. The in-repo codec is validated against real libbz2 streams in both directions: real-world `.tar.bz2` files extract correctly, and FVRS-created archives are readable by system `tar` and Python's `bz2`. (oxiarc-bzip2 0.3.3 is bidirectionally incompatible with real bzip2, hence the in-repo codec.)
- **Use case**: High-compression Unix/Linux archives

### GZ
- **Library**: `oxiarc-archive` (`GzipReader`)
- **Features**: Single-file decompression. The original filename stored in the gzip header is used for the output name when present (falling back to the archive's stem). Creation of standalone `.gz` files is not supported.
- **Use case**: Individual file decompression

### 7Z
- **Library**: In-repo reader (`archive/sevenz.rs`) with a spec-correct LZMA1/LZMA2 decoder (`archive/lzma_compat.rs`); Deflate and BZip2 folders delegate to oxiarc
- **Codecs**: Copy / LZMA / LZMA2 / BZip2 / Deflate / Delta / BCJ (x86)
- **Features**: Real listing (parses substream sizes, empty files, directories, anti-items, and encoded headers — **no temporary-directory side effects**) and CRC-verified extraction, including 0-byte entries. Creation is not supported.
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

The default feature set contains no C, C++, or Fortran code in the archive stack: `zip`, `flate2` (as an archive dependency), `tar`, `bzip2`, `sevenz-rust`, `cab`, `delharc`, and `unrar` were all replaced by the `oxiarc-*` family plus in-repo, spec-correct codec implementations. The only C++ dependency (`unrar`) is optional behind the non-default `rar` feature.

### Why some formats don't support creation

- **GZ**: Single-file container; FVRS creates `.tar.gz` instead
- **7Z**: The in-repo implementation is read-only
- **CAB**: No writer exists in oxiarc yet
- **RAR**: Creation is restricted by licensing terms

### Security

- All extraction paths are sanitized against path traversal (`../`, absolute paths, Windows drive prefixes, NUL) — covered by a zip-slip regression test
- TAR symlink entries are not materialized
- LZH and 7Z data are CRC-verified during extraction

### Error handling

Each format has descriptive, format-specific error messages with graceful degradation where possible (e.g. unsupported LZH compression methods are skipped with a warning rather than aborting the whole extraction).

## Testing

The archive suite (45 tests, `cargo nextest run`) covers create → list → extract round-trips for ZIP, TAR, TAR.GZ, TAR.BZ2, LZH (`-lhd-`/`-lh1-`), and 7Z (LZMA folders, empty file/dir substreams), plus zip-slip regression, 7z no-temp-dir-pollution, gzip header-filename handling, Shift_JIS/CP437 name codecs, and bzip2/LZMA compatibility against real libbz2/liblzma streams.

## Future Plans

- CAB creation support
- LZH `-lh5-` compression on create (pending upstream oxiarc-lzhuf encoder fix)
- 7Z creation
- Performance optimizations (streaming instead of whole-archive buffering for compressed TARs)

---

*Last updated: 2026-07-06*
*FVRS Version: 0.1.0*
