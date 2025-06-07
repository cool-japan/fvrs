#[derive(Debug, Clone)]
pub struct FileSystem {
    pub root_path: PathBuf,
}

impl Default for FileSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystem {
    pub fn new() -> Self {
        Self {
            root_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }

    pub fn change_directory(&mut self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if path.exists() && path.is_dir() {
            self.root_path = path.to_path_buf();
            Ok(())
        } else {
            Err(format!("Invalid directory: {}", path.display()).into())
        }
    }

    pub fn list_directory(&self, path: Option<&Path>) -> Result<Vec<FileInfo>, Box<dyn std::error::Error>> {
        let target_path = path.unwrap_or(&self.root_path);
        let mut files = Vec::new();

        for entry in fs::read_dir(target_path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            let path = entry.path();

            let file_info = FileInfo {
                name: path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                path: path.clone(),
                size: metadata.len(),
                is_directory: metadata.is_dir(),
                modified: metadata.modified().ok(),
            };

            files.push(file_info);
        }

        files.sort_by(|a, b| {
            // ディレクトリを先に、ファイルを後に
            match (a.is_directory, b.is_directory) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });

        Ok(files)
    }

    pub fn create_directory(&self, name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let new_dir = self.root_path.join(name);
        fs::create_dir(&new_dir)?;
        Ok(new_dir)
    }

    pub fn delete_file(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn copy_file(&self, from: &Path, to: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if from.is_dir() {
            self.copy_dir_all(from, to)?;
        } else {
            fs::copy(from, to)?;
        }
        Ok(())
    }

    fn copy_dir_all(&self, src: &Path, dst: &Path) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            if ty.is_dir() {
                self.copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
            } else {
                fs::copy(entry.path(), dst.join(entry.file_name()))?;
            }
        }
        Ok(())
    }

    pub fn move_file(&self, from: &Path, to: &Path) -> Result<(), Box<dyn std::error::Error>> {
        fs::rename(from, to)?;
        Ok(())
    }

    pub fn get_current_path(&self) -> &Path {
        &self.root_path
    }
} 