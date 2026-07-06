//! File search: name/content regex search with depth, extension and result limits

use chrono::DateTime;
use regex::Regex;
use tokio::fs;
use walkdir::WalkDir;

use crate::error::{FsError, FsResult};
use crate::fs::{FileEntry, FileSystem};

/// Search options for file search
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// Search pattern (regex)
    pub pattern: String,
    /// Search in file contents
    pub search_contents: bool,
    /// Case sensitive search
    pub case_sensitive: bool,
    /// Maximum depth for recursive search
    pub max_depth: Option<usize>,
    /// File extensions to include (None means all)
    pub extensions: Option<Vec<String>>,
    /// Maximum number of results
    pub max_results: Option<usize>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            search_contents: false,
            case_sensitive: true,
            max_depth: None,
            extensions: None,
            max_results: None,
        }
    }
}

impl FileSystem {
    /// Search for files matching the given pattern
    pub async fn search_files(&self, options: SearchOptions) -> FsResult<Vec<FileEntry>> {
        let pattern = if options.case_sensitive {
            Regex::new(&options.pattern)
        } else {
            Regex::new(&format!("(?i){}", options.pattern))
        }
        .map_err(|e| FsError::InvalidRegex(e.to_string()))?;

        let mut results = Vec::new();
        let mut walker = WalkDir::new(&self.current_dir).min_depth(1);

        if let Some(max_depth) = options.max_depth {
            walker = walker.max_depth(max_depth);
        }

        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            if let Some(max_results) = options.max_results {
                if results.len() >= max_results {
                    break;
                }
            }

            let path = entry.path();
            let name = entry.file_name().to_string_lossy();

            // Check file extension if specified
            if let Some(ref extensions) = options.extensions {
                if let Some(ext) = path.extension() {
                    if !extensions.contains(&ext.to_string_lossy().to_lowercase()) {
                        continue;
                    }
                }
            }

            // Check filename match
            if pattern.is_match(&name) {
                if let Ok(metadata) = fs::metadata(path).await {
                    results.push(FileEntry {
                        name: name.into_owned(),
                        path: path.to_path_buf(),
                        size: metadata.len(),
                        is_dir: metadata.is_dir(),
                        created: DateTime::from(metadata.created()?),
                        modified: DateTime::from(metadata.modified()?),
                        extension: path.extension().and_then(|e| e.to_str()).map(String::from),
                    });
                }
                continue;
            }

            // Check file contents if requested
            if options.search_contents && !path.is_dir() {
                if let Ok(contents) = fs::read_to_string(path).await {
                    if pattern.is_match(&contents) {
                        if let Ok(metadata) = fs::metadata(path).await {
                            results.push(FileEntry {
                                name: name.into_owned(),
                                path: path.to_path_buf(),
                                size: metadata.len(),
                                is_dir: metadata.is_dir(),
                                created: DateTime::from(metadata.created()?),
                                modified: DateTime::from(metadata.modified()?),
                                extension: path
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .map(String::from),
                            });
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// Find files by pattern (simplified search)
    pub async fn find_files(&self, pattern: &str) -> FsResult<Vec<FileEntry>> {
        let options = SearchOptions {
            pattern: pattern.to_string(),
            search_contents: false,
            case_sensitive: false,
            max_depth: None,
            extensions: None,
            max_results: None,
        };
        self.search_files(options).await
    }

    /// Find files by extension
    pub async fn find_files_by_extension(&self, extension: &str) -> FsResult<Vec<FileEntry>> {
        let options = SearchOptions {
            pattern: format!("\\.{}$", extension),
            search_contents: false,
            case_sensitive: false,
            max_depth: None,
            extensions: Some(vec![extension.to_string()]),
            max_results: None,
        };
        self.search_files(options).await
    }
}
