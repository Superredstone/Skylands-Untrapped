use log::{error, info};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExtractorError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ROM file not found: {0}")]
    RomNotFound(PathBuf),
    #[error("Extractor tool (wit) failed with status: {0}")]
    WitFailed(String),
    #[error("Validation failed: missing required file '{0}' in {1}")]
    ValidationMissingFile(String, PathBuf),
    #[error("Corrupt file: {0}")]
    CorruptFile(String),
}

pub struct Extractor {
    rom_path: PathBuf,
    dest_path: PathBuf,
}

impl Extractor {
    pub fn new<P1: AsRef<Path>, P2: AsRef<Path>>(rom_path: P1, dest_path: P2) -> Self {
        Self {
            rom_path: rom_path.as_ref().to_path_buf(),
            dest_path: dest_path.as_ref().to_path_buf(),
        }
    }

    pub fn is_extracted(&self) -> bool {
        let main_dol = self.dest_path.join("sys").join("main.dol");
        let global_arc = self
            .dest_path
            .join("files")
            .join("permanent")
            .join("global.arc");
        let global_bld = self
            .dest_path
            .join("files")
            .join("permanent")
            .join("global.bld");
        main_dol.exists() && global_arc.exists() && global_bld.exists()
    }

    pub fn extract_if_needed(&self) -> Result<PathBuf, ExtractorError> {
        if self.is_extracted() {
            info!("Assets already extracted at {:?}", self.dest_path);
            self.verify_assets()?;
            return Ok(self.dest_path.clone());
        }

        if !self.rom_path.exists() {
            return Err(ExtractorError::RomNotFound(self.rom_path.clone()));
        }

        info!(
            "Starting initial extraction from {:?} to {:?}",
            self.rom_path, self.dest_path
        );
        fs::create_dir_all(&self.dest_path)?;

        let status = Command::new("wit")
            .arg("extract")
            .arg(&self.rom_path)
            .arg(&self.dest_path)
            .arg("--overwrite")
            .status();

        match status {
            Ok(s) if s.success() => {
                info!("wit extraction finished successfully.");
            }
            Ok(s) => {
                let err_msg = format!("wit exited with code: {:?}", s.code());
                error!("{}", err_msg);
                return Err(ExtractorError::WitFailed(err_msg));
            }
            Err(e) => {
                // If direct wit invocation failed (e.g. NixOS PATH), attempt nix-shell fallback
                info!(
                    "Direct 'wit' not found in PATH, attempting nix-shell fallback: {}",
                    e
                );
                let nix_cmd = format!(
                    "wit extract \"{}\" \"{}\" --overwrite",
                    self.rom_path.display(),
                    self.dest_path.display()
                );
                let nix_status = Command::new("nix-shell")
                    .args(["-p", "wiimms-iso-tools", "--run", &nix_cmd])
                    .status()?;
                if !nix_status.success() {
                    return Err(ExtractorError::WitFailed(
                        "nix-shell wit extraction failed".into(),
                    ));
                }
            }
        }

        self.verify_assets()?;
        Ok(self.dest_path.clone())
    }

    pub fn verify_assets(&self) -> Result<(), ExtractorError> {
        let req_files = [
            ("sys/main.dol", 0x100),
            ("files/permanent/global.arc", 4),
            ("files/permanent/global.bld", 4),
        ];

        for (rel, min_size) in req_files {
            let path = self.dest_path.join(rel);
            if !path.exists() {
                return Err(ExtractorError::ValidationMissingFile(
                    rel.into(),
                    self.dest_path.clone(),
                ));
            }
            let meta = fs::metadata(&path)?;
            if meta.len() < min_size {
                return Err(ExtractorError::CorruptFile(format!("{} is too small", rel)));
            }
        }

        info!("Extraction integrity verified.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extractor_check() {
        let ext = Extractor::new("rom.eu.wbfs", "/tmp/nonexistent-skylands-test");
        assert!(!ext.is_extracted());
    }
}
