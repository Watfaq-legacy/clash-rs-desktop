use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppPaths {
    pub program_data: PathBuf,
    pub core_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub profiles_dir: PathBuf,
    pub updates_dir: PathBuf,
}

impl AppPaths {
    pub fn from_program_data(program_data: impl AsRef<Path>) -> Self {
        let program_data = program_data.as_ref().to_path_buf();
        Self {
            core_dir: program_data.join("core"),
            runtime_dir: program_data.join("runtime"),
            profiles_dir: program_data.join("profiles"),
            updates_dir: program_data.join("updates"),
            program_data,
        }
    }

    pub fn settings_path(&self) -> PathBuf {
        self.program_data.join("settings.json")
    }

    pub fn active_core_metadata_path(&self) -> PathBuf {
        self.core_dir.join("active.json")
    }

    pub fn profile_manifest_path(&self) -> PathBuf {
        self.profiles_dir.join("profiles.json")
    }

    pub fn core_pid_path(&self) -> PathBuf {
        self.runtime_dir.join("core.pid")
    }
}
