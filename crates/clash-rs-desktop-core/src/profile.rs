use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::paths::AppPaths;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileSource {
    LocalFile {
        path: PathBuf,
    },
    RemoteUrl {
        url: String,
        headers: Vec<(String, String)>,
    },
    InlineTemplate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: ProfileId,
    pub name: String,
    pub source: ProfileSource,
    pub source_path: PathBuf,
    pub generated_path: PathBuf,
    pub profile_override_path: Option<PathBuf>,
    pub active: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileManifest {
    pub active_profile_id: Option<ProfileId>,
    pub profiles: Vec<UserProfile>,
}

#[derive(Debug, Clone)]
pub struct ProfileStore {
    manifest_path: PathBuf,
    manifest: ProfileManifest,
}

impl ProfileStore {
    pub fn load(manifest_path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let manifest_path = manifest_path.into();
        let manifest = if manifest_path.exists() {
            let contents = fs::read_to_string(&manifest_path)?;
            serde_json::from_str(contents.trim_start_matches('\u{feff}'))?
        } else {
            ProfileManifest::default()
        };

        Ok(Self {
            manifest_path,
            manifest,
        })
    }

    pub fn manifest(&self) -> &ProfileManifest {
        &self.manifest
    }

    pub fn profiles(&self) -> &[UserProfile] {
        &self.manifest.profiles
    }

    pub fn active_profile(&self) -> Option<&UserProfile> {
        let active_id = self.manifest.active_profile_id.as_ref()?;
        self.manifest
            .profiles
            .iter()
            .find(|profile| &profile.id == active_id)
    }

    pub fn ensure_default_profile(&mut self, paths: &AppPaths) -> anyhow::Result<ProfileId> {
        if let Some(profile) = self.active_profile() {
            return Ok(profile.id.clone());
        }

        if let Some(profile) = self.manifest.profiles.first() {
            let id = profile.id.clone();
            self.set_active(&id)?;
            return Ok(id);
        }

        let id = ProfileId("default".to_owned());
        let profile_dir = paths.profiles_dir.join(&id.0);
        fs::create_dir_all(&profile_dir)?;

        let source_path = profile_dir.join("source.yaml");
        fs::write(&source_path, default_profile_yaml())?;

        let profile = UserProfile {
            id: id.clone(),
            name: "Default".to_owned(),
            source: ProfileSource::InlineTemplate,
            source_path,
            generated_path: profile_dir.join("generated.yaml"),
            profile_override_path: Some(profile_dir.join("override.yaml")),
            active: false,
        };

        self.upsert(profile);
        self.set_active(&id)?;
        Ok(id)
    }

    pub fn import_local_file(
        &mut self,
        paths: &AppPaths,
        name: impl AsRef<str>,
        source: impl AsRef<Path>,
    ) -> anyhow::Result<ProfileId> {
        let source = source.as_ref();
        let id = self.unique_profile_id(name.as_ref());
        let profile_dir = paths.profiles_dir.join(&id.0);
        fs::create_dir_all(&profile_dir)?;

        let source_path = profile_dir.join("source.yaml");
        fs::copy(source, &source_path)?;

        let profile = UserProfile {
            id: id.clone(),
            name: name.as_ref().to_owned(),
            source: ProfileSource::LocalFile {
                path: source.to_path_buf(),
            },
            source_path,
            generated_path: profile_dir.join("generated.yaml"),
            profile_override_path: Some(profile_dir.join("override.yaml")),
            active: false,
        };

        self.upsert(profile);
        if self.manifest.active_profile_id.is_none() {
            self.set_active(&id)?;
        }

        Ok(id)
    }

    pub fn upsert(&mut self, mut profile: UserProfile) {
        profile.active = self
            .manifest
            .active_profile_id
            .as_ref()
            .is_some_and(|active_id| active_id == &profile.id);

        match self
            .manifest
            .profiles
            .iter()
            .position(|existing| existing.id == profile.id)
        {
            Some(index) => self.manifest.profiles[index] = profile,
            None => self.manifest.profiles.push(profile),
        }
    }

    pub fn remove(&mut self, id: &ProfileId) -> Option<UserProfile> {
        let index = self
            .manifest
            .profiles
            .iter()
            .position(|profile| &profile.id == id)?;
        let removed = self.manifest.profiles.remove(index);

        if self.manifest.active_profile_id.as_ref() == Some(id) {
            self.manifest.active_profile_id = None;
        }

        Some(removed)
    }

    pub fn set_active(&mut self, id: &ProfileId) -> anyhow::Result<()> {
        if !self
            .manifest
            .profiles
            .iter()
            .any(|profile| &profile.id == id)
        {
            anyhow::bail!("profile {} does not exist", id.0);
        }

        self.manifest.active_profile_id = Some(id.clone());
        for profile in &mut self.manifest.profiles {
            profile.active = &profile.id == id;
        }

        Ok(())
    }

    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.manifest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let contents = serde_json::to_string_pretty(&self.manifest)?;
        fs::write(&self.manifest_path, contents)?;
        Ok(())
    }

    fn unique_profile_id(&self, name: &str) -> ProfileId {
        let base = sanitize_profile_id(name);
        if !self
            .manifest
            .profiles
            .iter()
            .any(|profile| profile.id.0 == base)
        {
            return ProfileId(base);
        }

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        ProfileId(format!("{base}-{suffix}"))
    }
}

pub fn default_profile_yaml() -> &'static str {
    "log-level: info\nmode: rule\nallow-lan: false\nmixed-port: 7890\nrules:\n  - MATCH,DIRECT\n"
}

fn sanitize_profile_id(name: &str) -> String {
    let mut sanitized = String::new();
    for ch in name.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            sanitized.push(ch);
        } else if matches!(ch, '-' | '_' | ' ') && !sanitized.ends_with('-') {
            sanitized.push('-');
        }
    }

    let sanitized = sanitized.trim_matches('-');
    if sanitized.is_empty() {
        "profile".to_owned()
    } else {
        sanitized.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{ProfileId, ProfileSource, ProfileStore, UserProfile};

    #[test]
    fn profile_store_persists_active_profile() {
        let temp = unique_temp_dir();
        let manifest_path = temp.join("profiles.json");

        let mut store = ProfileStore::load(&manifest_path).unwrap();
        let profile = UserProfile {
            id: ProfileId("default".to_owned()),
            name: "Default".to_owned(),
            source: ProfileSource::InlineTemplate,
            source_path: temp.join("source.yaml"),
            generated_path: temp.join("generated.yaml"),
            profile_override_path: None,
            active: false,
        };

        store.upsert(profile);
        store.set_active(&ProfileId("default".to_owned())).unwrap();
        store.save().unwrap();

        let loaded = ProfileStore::load(&manifest_path).unwrap();
        assert_eq!(loaded.active_profile().unwrap().name, "Default");
        assert!(loaded.active_profile().unwrap().active);
    }

    #[test]
    fn ensure_default_profile_creates_tryable_yaml() {
        let temp = unique_temp_dir();
        let paths = super::AppPaths::from_program_data(&temp);
        let manifest_path = temp.join("profiles.json");
        let mut store = ProfileStore::load(&manifest_path).unwrap();

        let id = store.ensure_default_profile(&paths).unwrap();
        assert_eq!(id.0, "default");
        assert!(store.active_profile().unwrap().source_path.exists());
        assert!(store.active_profile().unwrap().active);
    }

    fn unique_temp_dir() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(Path::new(&format!("clash-rs-profile-test-{nonce}")))
    }
}
