//! File and directory hashing over six algorithms via one generic streaming helper

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::time::SystemTime;

use md5::Md5;
use ripemd::Ripemd160;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use tokio::fs;
use tokio::io::AsyncReadExt;
use walkdir::WalkDir;

use crate::error::FsResult;
use crate::fs::FileSystem;

/// Hash algorithm type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HashAlgorithm {
    /// MD5 hash
    MD5,
    /// SHA-1 hash
    SHA1,
    /// SHA-256 hash
    SHA256,
    /// SHA-512 hash
    SHA512,
    /// BLAKE3 hash
    BLAKE3,
    /// RIPEMD-160 hash
    RIPEMD160,
}

/// Hash result structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashResult {
    /// Hash algorithm used
    pub algorithm: HashAlgorithm,
    /// Hash value in hexadecimal
    pub hash: String,
    /// File size in bytes
    pub size: u64,
    /// Time taken to compute hash in milliseconds
    pub time_ms: u64,
}

/// Incremental hasher abstraction unifying RustCrypto digests and BLAKE3.
///
/// This replaces the previous six copy-pasted per-algorithm loops with a single
/// generic code path: callers obtain a boxed hasher from
/// [`HashAlgorithm::new_hasher`], feed it bytes with [`StreamHasher::update`],
/// and finish with [`StreamHasher::finalize_hex`].
trait StreamHasher {
    /// Feed a chunk of bytes into the hash state
    fn update(&mut self, data: &[u8]);
    /// Consume the hasher and return the lowercase hexadecimal digest
    fn finalize_hex(self: Box<Self>) -> String;
}

/// Adapter for any RustCrypto `Digest` implementation
struct DigestHasher<D: Digest>(D);

impl<D: Digest> StreamHasher for DigestHasher<D> {
    fn update(&mut self, data: &[u8]) {
        Digest::update(&mut self.0, data);
    }

    fn finalize_hex(self: Box<Self>) -> String {
        hex::encode(self.0.finalize())
    }
}

/// Adapter for the BLAKE3 hasher (not part of the RustCrypto `Digest` family)
struct Blake3StreamHasher(blake3::Hasher);

impl StreamHasher for Blake3StreamHasher {
    fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }

    fn finalize_hex(self: Box<Self>) -> String {
        hex::encode(self.0.finalize().as_bytes())
    }
}

impl HashAlgorithm {
    /// Create a fresh streaming hasher for this algorithm
    fn new_hasher(self) -> Box<dyn StreamHasher> {
        match self {
            HashAlgorithm::MD5 => Box::new(DigestHasher(Md5::new())),
            HashAlgorithm::SHA1 => Box::new(DigestHasher(sha1::Sha1::new())),
            HashAlgorithm::SHA256 => Box::new(DigestHasher(Sha256::new())),
            HashAlgorithm::SHA512 => Box::new(DigestHasher(Sha512::new())),
            HashAlgorithm::BLAKE3 => Box::new(Blake3StreamHasher(blake3::Hasher::new())),
            HashAlgorithm::RIPEMD160 => Box::new(DigestHasher(Ripemd160::new())),
        }
    }
}

impl FileSystem {
    /// Calculate hash of a file
    pub async fn calculate_hash(&self, path: &PathBuf, algorithm: HashAlgorithm) -> FsResult<HashResult> {
        let start_time = SystemTime::now();
        let mut file = File::open(path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        let size = buffer.len() as u64;

        let mut hasher = algorithm.new_hasher();
        hasher.update(&buffer);
        let hash = hasher.finalize_hex();

        let end_time = SystemTime::now();
        let duration = end_time.duration_since(start_time).unwrap_or(std::time::Duration::ZERO);
        let time_ms = duration.as_millis() as u64;

        Ok(HashResult {
            algorithm,
            hash,
            size,
            time_ms,
        })
    }

    /// Verify file hash
    pub async fn verify_hash(&self, path: &PathBuf, expected_hash: &str, algorithm: HashAlgorithm) -> FsResult<bool> {
        let result = self.calculate_hash(path, algorithm).await?;
        Ok(result.hash == expected_hash)
    }

    /// Calculate hash of a directory (recursive)
    pub async fn calculate_directory_hash(&self, path: &PathBuf, algorithm: HashAlgorithm) -> FsResult<HashResult> {
        let start_time = std::time::Instant::now();

        let mut hasher = algorithm.new_hasher();
        for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
            let entry_path = entry.path();
            if entry_path.is_file() {
                let mut file = fs::File::open(entry_path).await?;
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer).await?;
                hasher.update(&buffer);
            }
        }
        let hash = hasher.finalize_hex();

        let time_ms = start_time.elapsed().as_millis() as u64;
        let size = WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
            .sum();
        Ok(HashResult {
            algorithm,
            hash,
            size,
            time_ms,
        })
    }
}
