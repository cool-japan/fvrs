use native_windows_gui as nwg;
use native_windows_derive as nwd;
use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;
use thiserror::Error;
use image::{self, DynamicImage, ImageBuffer};
use std::fs;
use std::io::Read;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::Result;

#[derive(Debug, Error)]
pub enum PreviewError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
    
    #[error("Unsupported file type: {0}")]
    UnsupportedFileType(String),
}

/// Preview panel structure
#[derive(Default, NwgUi)]
pub struct PreviewPanel {
    #[nwg_control(parent: window, size: (780, 60), position: (10, 530))]
    panel: nwg::Panel,
    
    #[nwg_control(parent: panel, text: "Preview:", size: (60, 20), position: (10, 10))]
    preview_label: nwg::Label,
    
    #[nwg_control(parent: panel, size: (760, 40), position: (10, 30))]
    preview_text: nwg::TextInput,
    
    // Internal state
    current_path: RefCell<Option<PathBuf>>,
    syntax_set: RefCell<SyntaxSet>,
    theme_set: RefCell<ThemeSet>,
}

impl PreviewPanel {
    pub fn new() -> Result<Self> {
        let mut panel = Self::default();
        panel.build()?;
        
        // Initialize syntax highlighting
        *panel.syntax_set.borrow_mut() = SyntaxSet::load_defaults_newlines();
        *panel.theme_set.borrow_mut() = ThemeSet::load_defaults();
        
        Ok(panel)
    }
    
    /// Update the preview for a file
    pub fn update_preview(&self, path: &PathBuf) -> Result<()> {
        *self.current_path.borrow_mut() = Some(path.clone());
        
        if path.is_dir() {
            self.show_directory_info(path)?;
        } else {
            match self.get_file_type(path) {
                FileType::Image => self.show_image_preview(path)?,
                FileType::Text => self.show_text_preview(path)?,
                FileType::Audio => self.show_audio_info(path)?,
                FileType::Video => self.show_video_info(path)?,
                FileType::Pdf => self.show_pdf_info(path)?,
                FileType::Unknown => self.show_file_info(path)?,
            }
        }
        
        Ok(())
    }
    
    /// Clear the preview
    pub fn clear(&self) {
        self.preview_text.set_text("");
        *self.current_path.borrow_mut() = None;
    }
    
    /// Show directory information
    fn show_directory_info(&self, path: &PathBuf) -> Result<()> {
        let mut file_count = 0;
        let mut dir_count = 0;
        let mut total_size = 0;
        
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            
            if metadata.is_dir() {
                dir_count += 1;
            } else {
                file_count += 1;
                total_size += metadata.len();
            }
        }
        
        let info = format!(
            "Directory: {}\nFiles: {}, Directories: {}, Total size: {}",
            path.display(),
            file_count,
            dir_count,
            format_size(total_size)
        );
        
        self.preview_text.set_text(&info);
        Ok(())
    }
    
    /// Show image preview
    fn show_image_preview(&self, path: &PathBuf) -> Result<()> {
        let img = image::open(path)?;
        let info = format!(
            "Image: {}\nSize: {}x{}, Format: {}",
            path.display(),
            img.width(),
            img.height(),
            img.color()
        );
        
        self.preview_text.set_text(&info);
        Ok(())
    }
    
    /// Show text preview with syntax highlighting
    fn show_text_preview(&self, path: &PathBuf) -> Result<()> {
        let mut content = String::new();
        fs::File::open(path)?.read_to_string(&mut content)?;
        
        let syntax = self.syntax_set.borrow().find_syntax_for_file(path)
            .unwrap_or_else(|| self.syntax_set.borrow().find_syntax_plain_text());
        let theme = &self.theme_set.borrow().themes["base16-ocean.dark"];
        let mut h = HighlightLines::new(syntax, theme);
        
        let mut highlighted = String::new();
        for line in LinesWithEndings::from(&content).take(10) {
            let ranges: Vec<(Style, &str)> = h.highlight_line(line, &self.syntax_set.borrow()).unwrap();
            for (style, text) in ranges {
                highlighted.push_str(&format!("{}", text));
            }
        }
        
        if content.lines().count() > 10 {
            highlighted.push_str("\n...");
        }
        
        self.preview_text.set_text(&highlighted);
        Ok(())
    }
    
    /// Show audio file information
    fn show_audio_info(&self, path: &PathBuf) -> Result<()> {
        let metadata = fs::metadata(path)?;
        let info = format!(
            "Audio: {}\nSize: {}, Duration: N/A",
            path.display(),
            format_size(metadata.len())
        );
        
        self.preview_text.set_text(&info);
        Ok(())
    }
    
    /// Show video file information
    fn show_video_info(&self, path: &PathBuf) -> Result<()> {
        let metadata = fs::metadata(path)?;
        let info = format!(
            "Video: {}\nSize: {}, Duration: N/A",
            path.display(),
            format_size(metadata.len())
        );
        
        self.preview_text.set_text(&info);
        Ok(())
    }
    
    /// Show PDF file information
    fn show_pdf_info(&self, path: &PathBuf) -> Result<()> {
        let metadata = fs::metadata(path)?;
        let info = format!(
            "PDF: {}\nSize: {}, Pages: N/A",
            path.display(),
            format_size(metadata.len())
        );
        
        self.preview_text.set_text(&info);
        Ok(())
    }
    
    /// Show basic file information
    fn show_file_info(&self, path: &PathBuf) -> Result<()> {
        let metadata = fs::metadata(path)?;
        let info = format!(
            "File: {}\nSize: {}, Type: {}",
            path.display(),
            format_size(metadata.len()),
            self.get_file_type(path)
        );
        
        self.preview_text.set_text(&info);
        Ok(())
    }
    
    /// Get the file type
    fn get_file_type(&self, path: &PathBuf) -> FileType {
        if let Some(ext) = path.extension() {
            match ext.to_str().unwrap().to_lowercase().as_str() {
                "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" => FileType::Image,
                "txt" | "rs" | "toml" | "json" | "md" | "html" | "css" | "js" => FileType::Text,
                "mp3" | "wav" | "ogg" | "flac" => FileType::Audio,
                "mp4" | "avi" | "mkv" | "webm" => FileType::Video,
                "pdf" => FileType::Pdf,
                _ => FileType::Unknown,
            }
        } else {
            FileType::Unknown
        }
    }
}

/// File type enum
#[derive(Debug, Clone, Copy, PartialEq)]
enum FileType {
    Image,
    Text,
    Audio,
    Video,
    Pdf,
    Unknown,
}

impl std::fmt::Display for FileType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileType::Image => write!(f, "Image"),
            FileType::Text => write!(f, "Text"),
            FileType::Audio => write!(f, "Audio"),
            FileType::Video => write!(f, "Video"),
            FileType::Pdf => write!(f, "PDF"),
            FileType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Format file size
fn format_size(size: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut size = size as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    format!("{:.1} {}", size, UNITS[unit_index])
} 