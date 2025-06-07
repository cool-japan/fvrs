# Supported Archive Formats

FVRS supports a wide range of archive formats for viewing, extraction, and creation. Below is a comprehensive list of all supported formats and their capabilities.

## Format Support Matrix

| Format | List | Extract | Create | Status |
|--------|------|---------|--------|--------|
| ZIP | ✅ | ✅ | ✅ | Full support |
| LHA/LZH | ✅ | ✅ | ❌ | Extract only |
| TAR | ✅ | ✅ | ✅ | Full support |
| TAR.GZ | ✅ | ✅ | ✅ | Full support |
| **TAR.BZ2** | ✅ | ✅ | ✅ | **Newly added** |
| GZ | ✅ | ✅ | ❌ | Single file compression |
| 7Z | ✅ | ✅ | ❌ | Extract only |
| **RAR** | ✅ | ✅ | ❌ | **Newly added** |
| **CAB** | ✅ | ✅ | ❌ | **Newly added** |

## Legend

- ✅ **Supported**: Feature is fully implemented and tested
- ❌ **Not supported**: Feature is not available
- **Bold text**: Recently added formats

## Format Details

### ZIP
- **Library**: `zip` crate
- **Compression**: Various algorithms supported
- **Features**: Full support including encryption
- **Use case**: General-purpose archiving

### LHA/LZH
- **Library**: `delharc` crate
- **Compression**: LHA compression algorithms
- **Features**: Extract and list only (creation not supported by library)
- **Use case**: Legacy Japanese archive format

### TAR
- **Library**: `tar` crate
- **Compression**: Uncompressed
- **Features**: Full support for POSIX tar format
- **Use case**: Unix/Linux archiving

### TAR.GZ
- **Library**: `tar` + `flate2` crates
- **Compression**: GZIP compression
- **Features**: Full support
- **Use case**: Common Unix/Linux compressed archives

### TAR.BZ2 ⭐ *New*
- **Library**: `tar` + `bzip2` crates
- **Compression**: BZIP2 compression
- **Features**: Full support with high compression ratio
- **Use case**: High-compression Unix/Linux archives

### GZ
- **Library**: `flate2` crate
- **Compression**: GZIP compression
- **Features**: Single file compression/decompression
- **Use case**: Individual file compression

### 7Z
- **Library**: `sevenz-rust` crate
- **Compression**: LZMA and other algorithms
- **Features**: Extract and list only
- **Use case**: High-compression archiving

### RAR ⭐ *New*
- **Library**: `unrar` crate
- **Compression**: RAR proprietary algorithms
- **Features**: Extract and list only (creation restricted by license)
- **Use case**: Popular proprietary archive format

### CAB ⭐ *New*
- **Library**: `cab` crate
- **Compression**: Various Microsoft compression algorithms
- **Features**: Extract and list only
- **Use case**: Windows cabinet files

## Recent Additions

The following formats were recently added to expand FVRS's archive format support:

1. **TAR.BZ2**: Complete support for BZIP2-compressed TAR archives
2. **RAR**: Full extraction and listing support for RAR archives
3. **CAB**: Complete support for Microsoft Cabinet files

## Implementation Notes

### Why Some Formats Don't Support Creation

- **LHA/LZH**: The `delharc` library is extraction-only
- **RAR**: Creation is restricted by licensing terms
- **CAB**: Creation support is planned for future releases
- **7Z**: The current library primarily focuses on extraction
- **GZ**: Single-file format, creation would require different API

### Error Handling

Each format has optimized error handling with descriptive messages:
- Format-specific error messages
- Detailed failure reasons
- Graceful degradation when possible

## Future Plans

- CAB creation support
- Additional compression algorithms for existing formats
- Performance optimizations
- Enhanced metadata support

---

*Last updated: December 2024*
*FVRS Version: 0.1.0* 