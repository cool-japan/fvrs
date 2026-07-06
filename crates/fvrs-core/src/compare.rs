//! File and directory comparison (binary and text modes)

use std::cmp::min;
use std::path::PathBuf;

use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader as TokioBufReader};
use walkdir::WalkDir;

use crate::error::{FsError, FsResult};
use crate::fs::FileSystem;

/// Comparison type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonType {
    /// Binary comparison
    Binary,
    /// Text comparison
    Text,
    /// Text comparison ignoring whitespace
    TextIgnoreWhitespace,
    /// Text comparison ignoring case
    TextIgnoreCase,
}

/// Difference type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Difference {
    /// Different bytes at offset
    BinaryDiff { offset: u64, left: u8, right: u8 },
    /// Different lines
    TextDiff {
        line: usize,
        left: String,
        right: String,
    },
    /// File size difference
    SizeDiff { left_size: u64, right_size: u64 },
    /// File type difference
    TypeDiff {
        left_type: String,
        right_type: String,
    },
}

/// Comparison result
#[derive(Debug, Clone)]
pub struct ComparisonResult {
    /// Whether files are identical
    pub identical: bool,
    /// List of differences
    pub differences: Vec<Difference>,
    /// Total number of differences
    pub total_differences: usize,
    /// Comparison time in milliseconds
    pub time_ms: u64,
}

impl FileSystem {
    /// Compare two files
    pub async fn compare_files(
        &self,
        left: &PathBuf,
        right: &PathBuf,
        comparison_type: ComparisonType,
    ) -> FsResult<ComparisonResult> {
        let start_time = std::time::Instant::now();
        let mut differences = Vec::new();

        // Check file sizes
        let left_metadata = fs::metadata(left).await?;
        let right_metadata = fs::metadata(right).await?;

        if left_metadata.len() != right_metadata.len() {
            differences.push(Difference::SizeDiff {
                left_size: left_metadata.len(),
                right_size: right_metadata.len(),
            });
        }

        match comparison_type {
            ComparisonType::Binary => {
                self.compare_binary(left, right, &mut differences).await?;
            }
            ComparisonType::Text
            | ComparisonType::TextIgnoreWhitespace
            | ComparisonType::TextIgnoreCase => {
                self.compare_text(left, right, comparison_type, &mut differences)
                    .await?;
            }
        }

        let time_ms = start_time.elapsed().as_millis() as u64;

        let diff_clone = differences.clone();
        Ok(ComparisonResult {
            identical: diff_clone.is_empty(),
            differences,
            total_differences: diff_clone.len(),
            time_ms,
        })
    }

    async fn compare_binary(
        &self,
        left: &PathBuf,
        right: &PathBuf,
        differences: &mut Vec<Difference>,
    ) -> FsResult<()> {
        let mut left_file = fs::File::open(left).await?;
        let mut right_file = fs::File::open(right).await?;

        let mut left_buffer = [0u8; 8192];
        let mut right_buffer = [0u8; 8192];
        let mut offset = 0u64;

        loop {
            let left_read = left_file.read(&mut left_buffer).await?;
            let right_read = right_file.read(&mut right_buffer).await?;

            if left_read == 0 && right_read == 0 {
                break;
            }

            let min_read = min(left_read, right_read);
            for i in 0..min_read {
                if left_buffer[i] != right_buffer[i] {
                    differences.push(Difference::BinaryDiff {
                        offset: offset + i as u64,
                        left: left_buffer[i],
                        right: right_buffer[i],
                    });
                }
            }

            offset += min_read as u64;
        }

        Ok(())
    }

    async fn compare_text(
        &self,
        left: &PathBuf,
        right: &PathBuf,
        comparison_type: ComparisonType,
        differences: &mut Vec<Difference>,
    ) -> FsResult<()> {
        let left_file = fs::File::open(left).await?;
        let right_file = fs::File::open(right).await?;

        let mut left_reader = TokioBufReader::new(left_file);
        let mut right_reader = TokioBufReader::new(right_file);

        let mut left_line = String::new();
        let mut right_line = String::new();
        let mut line_number = 1;

        loop {
            left_line.clear();
            right_line.clear();

            let left_read = left_reader.read_line(&mut left_line).await?;
            let right_read = right_reader.read_line(&mut right_line).await?;

            if left_read == 0 && right_read == 0 {
                break;
            }

            let left_processed = match comparison_type {
                ComparisonType::TextIgnoreWhitespace => left_line.trim().to_string(),
                ComparisonType::TextIgnoreCase => left_line.to_lowercase(),
                _ => left_line.clone(),
            };

            let right_processed = match comparison_type {
                ComparisonType::TextIgnoreWhitespace => right_line.trim().to_string(),
                ComparisonType::TextIgnoreCase => right_line.to_lowercase(),
                _ => right_line.clone(),
            };

            if left_processed != right_processed {
                differences.push(Difference::TextDiff {
                    line: line_number,
                    left: left_line.clone(),
                    right: right_line.clone(),
                });
            }

            line_number += 1;
        }

        Ok(())
    }

    /// Compare two directories recursively
    pub async fn compare_directories(
        &self,
        left: &PathBuf,
        right: &PathBuf,
        comparison_type: ComparisonType,
    ) -> FsResult<ComparisonResult> {
        let start_time = std::time::Instant::now();
        let mut differences = Vec::new();

        let mut left_entries = Vec::new();
        let mut right_entries = Vec::new();

        for entry in WalkDir::new(left).into_iter().filter_map(|e| e.ok()) {
            left_entries.push(entry.path().to_path_buf());
        }

        for entry in WalkDir::new(right).into_iter().filter_map(|e| e.ok()) {
            right_entries.push(entry.path().to_path_buf());
        }

        // Compare file lists
        for left_path in &left_entries {
            let relative_path = left_path.strip_prefix(left).map_err(|e| {
                FsError::Comparison(format!(
                    "Failed to strip prefix {:?} from {:?}: {}",
                    left, left_path, e
                ))
            })?;
            let right_path = right.join(relative_path);

            if !right_path.exists() {
                differences.push(Difference::TypeDiff {
                    left_type: "file".to_string(),
                    right_type: "missing".to_string(),
                });
                continue;
            }

            if left_path.is_file() && right_path.is_file() {
                let result = self
                    .compare_files(left_path, &right_path, comparison_type)
                    .await?;
                differences.extend(result.differences);
            }
        }

        // Check for files in right that don't exist in left
        for right_path in &right_entries {
            let relative_path = right_path.strip_prefix(right).map_err(|e| {
                FsError::Comparison(format!(
                    "Failed to strip prefix {:?} from {:?}: {}",
                    right, right_path, e
                ))
            })?;
            let left_path = left.join(relative_path);

            if !left_path.exists() {
                differences.push(Difference::TypeDiff {
                    left_type: "missing".to_string(),
                    right_type: "file".to_string(),
                });
            }
        }

        let time_ms = start_time.elapsed().as_millis() as u64;

        let diff_clone = differences.clone();
        Ok(ComparisonResult {
            identical: diff_clone.is_empty(),
            differences,
            total_differences: diff_clone.len(),
            time_ms,
        })
    }
}
