use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Context as _;
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
    /// Traffic/expiry advertised by the provider (`subscription-userinfo`).
    #[serde(default)]
    pub subscription: Option<SubscriptionInfo>,
    /// Unix seconds of the last successful download, for remote profiles.
    #[serde(default)]
    pub updated_at: Option<i64>,
    /// Provider-suggested refresh interval (`profile-update-interval`).
    #[serde(default)]
    pub update_interval_hours: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionInfo {
    pub upload: u64,
    pub download: u64,
    pub total: u64,
    /// Unix seconds; absent or 0 means no expiry.
    pub expire: Option<i64>,
}

impl SubscriptionInfo {
    /// Parse `upload=1; download=2; total=3; expire=4`.
    pub fn parse_header(raw: &str) -> Option<Self> {
        let mut info = Self::default();
        let mut seen = false;
        for part in raw.split(';') {
            let Some((key, value)) = part.split_once('=') else {
                continue;
            };
            // Some providers send floats or empty values; tolerate both.
            let value = value.trim();
            let number = value.parse::<f64>().ok().map(|n| n.max(0.0) as u64);
            match key.trim().to_ascii_lowercase().as_str() {
                "upload" => info.upload = number.unwrap_or(0),
                "download" => info.download = number.unwrap_or(0),
                "total" => info.total = number.unwrap_or(0),
                "expire" => info.expire = number.filter(|n| *n > 0).map(|n| n as i64),
                _ => continue,
            }
            seen = true;
        }
        seen.then_some(info)
    }
}

/// A downloaded subscription, ready to be stored by [`ProfileStore`].
#[derive(Debug, Clone)]
pub struct FetchedSubscription {
    pub url: String,
    pub body: String,
    pub info: Option<SubscriptionInfo>,
    pub suggested_name: Option<String>,
    pub update_interval_hours: Option<u32>,
}

/// Download a subscription URL and check it is a Clash config.
pub async fn fetch_subscription(url: &str) -> anyhow::Result<FetchedSubscription> {
    let url = url.trim();
    let parsed = reqwest::Url::parse(url).map_err(|_| anyhow::anyhow!("订阅链接格式不正确"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        anyhow::bail!("订阅链接需要以 http:// 或 https:// 开头");
    }

    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?
        .get(parsed.clone())
        // Providers pick the output format from the UA; this gets Clash YAML.
        .header(
            reqwest::header::USER_AGENT,
            "clash.meta clash-rs-desktop/0.1",
        )
        .send()
        .await
        .map_err(|error| anyhow::anyhow!("无法下载订阅：{error}"))?;

    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("订阅服务器返回错误（{status}）");
    }

    let headers = response.headers().clone();
    let body = normalise_config(response.text().await?)?;

    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let suggested_name = header("profile-title")
        .and_then(decode_profile_title)
        .or_else(|| header("content-disposition").and_then(filename_from_disposition))
        .or_else(|| parsed.host_str().map(str::to_owned));

    Ok(FetchedSubscription {
        url: url.to_owned(),
        body,
        info: header("subscription-userinfo").and_then(SubscriptionInfo::parse_header),
        suggested_name,
        update_interval_hours: header("profile-update-interval")
            .and_then(|v| v.trim().parse::<u32>().ok())
            .filter(|h| *h > 0),
    })
}

/// Accept a Clash YAML config as-is, or turn share links / a base64 link
/// list (what many providers serve without a Clash UA) into one.
pub fn normalise_config(body: String) -> anyhow::Result<String> {
    match validate_clash_yaml(&body) {
        Ok(()) => Ok(body),
        Err(error) => match crate::links::config_from_links(&body) {
            Some(config) => Ok(serde_yaml::to_string(&config)?),
            None => Err(error),
        },
    }
}

fn validate_clash_yaml(body: &str) -> anyhow::Result<()> {
    let value: serde_yaml::Value = serde_yaml::from_str(body)
        .map_err(|_| anyhow::anyhow!("订阅内容不是 Clash 配置，请确认链接支持 Clash"))?;
    let has_nodes = ["proxies", "proxy-providers"]
        .iter()
        .any(|key| value.get(*key).is_some());
    if !has_nodes {
        anyhow::bail!("订阅里没有找到任何节点");
    }
    Ok(())
}

fn decode_profile_title(raw: &str) -> Option<String> {
    use base64::Engine as _;
    let title = match raw.strip_prefix("base64:") {
        Some(encoded) => String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(encoded.trim())
                .ok()?,
        )
        .ok()?,
        None => raw.to_owned(),
    };
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_owned())
}

fn filename_from_disposition(raw: &str) -> Option<String> {
    // Prefer RFC 5987 `filename*=UTF-8''...`, then plain `filename=`.
    let pick = |prefix: &str| {
        raw.split(';')
            .map(str::trim)
            .find_map(|part| part.strip_prefix(prefix))
            .map(|v| v.trim_matches('"').to_owned())
    };
    let name = pick("filename*=")
        .and_then(|v| v.split_once("''").map(|(_, rest)| percent_decode(rest)))
        .or_else(|| pick("filename="))?;
    let name = name
        .trim_end_matches(".yaml")
        .trim_end_matches(".yml")
        .trim();
    (!name.is_empty()).then(|| name.to_owned())
}

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3])
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileManifest {
    pub active_profile_id: Option<ProfileId>,
    pub profiles: Vec<UserProfile>,
    /// Extra profiles whose nodes are merged into the active one.
    #[serde(default)]
    pub merged_ids: Vec<ProfileId>,
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
            subscription: None,
            updated_at: None,
            update_interval_hours: None,
        };

        self.upsert(profile);
        self.set_active(&id)?;
        Ok(id)
    }

    /// Import a config file (Clash YAML, or a text file of share links).
    pub fn import_file(&mut self, paths: &AppPaths, file: &Path) -> anyhow::Result<ProfileId> {
        let text = fs::read_to_string(file).context("无法读取这个文件")?;
        let body = normalise_config(text)
            .map_err(|_| anyhow::anyhow!("这个文件不是 Clash 配置，也没有识别到节点链接"))?;
        let name = file
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "本地配置".to_owned());
        let source = ProfileSource::LocalFile {
            path: file.to_path_buf(),
        };
        self.import_content(paths, &name, source, &body)
    }

    /// Import pasted share links (`ss://`, `vmess://`, ...) as a profile.
    pub fn import_links(&mut self, paths: &AppPaths, text: &str) -> anyhow::Result<ProfileId> {
        let config = crate::links::config_from_links(text)
            .ok_or_else(|| anyhow::anyhow!("没有识别到支持的节点链接"))?;
        let count = config["proxies"].as_sequence().map_or(0, Vec::len);
        let name = if count == 1 {
            config["proxies"][0]["name"]
                .as_str()
                .unwrap_or("我的节点")
                .to_owned()
        } else {
            format!("我的节点（{count} 个）")
        };
        self.import_content(
            paths,
            &name,
            ProfileSource::InlineTemplate,
            &serde_yaml::to_string(&config)?,
        )
    }

    fn import_content(
        &mut self,
        paths: &AppPaths,
        name: &str,
        source: ProfileSource,
        body: &str,
    ) -> anyhow::Result<ProfileId> {
        let id = self.unique_profile_id(name);
        let profile_dir = paths.profiles_dir.join(&id.0);
        fs::create_dir_all(&profile_dir)?;
        let source_path = profile_dir.join("source.yaml");
        fs::write(&source_path, body)?;

        self.upsert(UserProfile {
            id: id.clone(),
            name: name.to_owned(),
            source,
            source_path,
            generated_path: profile_dir.join("generated.yaml"),
            profile_override_path: Some(profile_dir.join("override.yaml")),
            active: false,
            subscription: None,
            updated_at: Some(unix_now()),
            update_interval_hours: None,
        });
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
            subscription: None,
            updated_at: None,
            update_interval_hours: None,
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

    /// Store a freshly downloaded subscription as a new profile. The first
    /// profile becomes active automatically.
    pub fn import_remote(
        &mut self,
        paths: &AppPaths,
        fetched: FetchedSubscription,
    ) -> anyhow::Result<ProfileId> {
        let name = fetched
            .suggested_name
            .clone()
            .unwrap_or_else(|| "订阅".to_owned());
        let id = self.unique_profile_id(&name);
        let profile_dir = paths.profiles_dir.join(&id.0);
        fs::create_dir_all(&profile_dir)?;

        let source_path = profile_dir.join("source.yaml");
        fs::write(&source_path, &fetched.body)?;

        let profile = UserProfile {
            id: id.clone(),
            name,
            source: ProfileSource::RemoteUrl {
                url: fetched.url,
                headers: Vec::new(),
            },
            source_path,
            generated_path: profile_dir.join("generated.yaml"),
            profile_override_path: Some(profile_dir.join("override.yaml")),
            active: false,
            subscription: fetched.info,
            updated_at: Some(unix_now()),
            update_interval_hours: fetched.update_interval_hours,
        };

        self.upsert(profile);
        if self.manifest.active_profile_id.is_none() {
            self.set_active(&id)?;
        }
        Ok(id)
    }

    /// Replace a remote profile's content with a new download. The user's
    /// override file is untouched, so their edits survive the update.
    pub fn apply_refresh(
        &mut self,
        id: &ProfileId,
        fetched: FetchedSubscription,
    ) -> anyhow::Result<()> {
        let profile = self
            .manifest
            .profiles
            .iter_mut()
            .find(|profile| &profile.id == id)
            .ok_or_else(|| anyhow::anyhow!("profile {} does not exist", id.0))?;

        // Write-then-rename so a crash mid-write can't leave a torn config.
        let staging = profile.source_path.with_extension("yaml.new");
        fs::write(&staging, &fetched.body)?;
        fs::rename(&staging, &profile.source_path)?;

        profile.subscription = fetched.info.or(profile.subscription.take());
        profile.updated_at = Some(unix_now());
        if fetched.update_interval_hours.is_some() {
            profile.update_interval_hours = fetched.update_interval_hours;
        }
        Ok(())
    }

    /// Remote profiles whose refresh interval (provider's, else 12h) elapsed.
    pub fn due_for_refresh(&self) -> Vec<(ProfileId, String)> {
        let now = unix_now();
        self.manifest
            .profiles
            .iter()
            .filter_map(|profile| {
                let ProfileSource::RemoteUrl { url, .. } = &profile.source else {
                    return None;
                };
                let hours = i64::from(profile.update_interval_hours.unwrap_or(12));
                let due = profile.updated_at.is_none_or(|at| now - at >= hours * 3600);
                due.then(|| (profile.id.clone(), url.clone()))
            })
            .collect()
    }

    pub fn profile(&self, id: &ProfileId) -> Option<&UserProfile> {
        self.manifest
            .profiles
            .iter()
            .find(|profile| &profile.id == id)
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
        self.manifest.merged_ids.retain(|merged| merged != id);

        Some(removed)
    }

    /// Profiles merged into the active one (never includes the active one).
    pub fn merged_profiles(&self) -> Vec<&UserProfile> {
        let active = self.manifest.active_profile_id.as_ref();
        self.manifest
            .merged_ids
            .iter()
            .filter(|id| Some(*id) != active)
            .filter_map(|id| self.profile(id))
            .collect()
    }

    /// Which profile is active and which are merged — to roll a change back.
    pub fn selection(&self) -> (Option<ProfileId>, Vec<ProfileId>) {
        (
            self.manifest.active_profile_id.clone(),
            self.manifest.merged_ids.clone(),
        )
    }

    pub fn restore_selection(&mut self, (active, merged): (Option<ProfileId>, Vec<ProfileId>)) {
        if let Some(id) = &active
            && self.profile(id).is_some()
        {
            let _ = self.set_active(id);
        }
        self.manifest.merged_ids = merged;
    }

    pub fn is_merged(&self, id: &ProfileId) -> bool {
        self.manifest.merged_ids.contains(id)
    }

    pub fn set_merged(&mut self, id: &ProfileId, merged: bool) {
        self.manifest.merged_ids.retain(|existing| existing != id);
        if merged {
            self.manifest.merged_ids.push(id.clone());
        }
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

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
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

    use super::{
        ProfileId, ProfileSource, ProfileStore, SubscriptionInfo, UserProfile,
        decode_profile_title, filename_from_disposition,
    };

    #[test]
    fn parses_subscription_userinfo() {
        let info = SubscriptionInfo::parse_header(
            "upload=1024; download=2048.0; total=10737418240; expire=1767225600",
        )
        .unwrap();
        assert_eq!(info.upload, 1024);
        assert_eq!(info.download, 2048);
        assert_eq!(info.total, 10_737_418_240);
        assert_eq!(info.expire, Some(1_767_225_600));

        let no_expiry =
            SubscriptionInfo::parse_header("upload=0; download=0; total=1; expire=").unwrap();
        assert_eq!(no_expiry.expire, None);
        assert!(SubscriptionInfo::parse_header("garbage").is_none());
    }

    #[test]
    fn derives_profile_names_from_headers() {
        assert_eq!(
            decode_profile_title("base64:5rWL6K+V5py65Zy6").as_deref(),
            Some("测试机场")
        );
        assert_eq!(decode_profile_title("Plain").as_deref(), Some("Plain"));
        assert_eq!(
            filename_from_disposition("attachment; filename*=UTF-8''%E6%9C%BA%E5%9C%BA.yaml")
                .as_deref(),
            Some("机场")
        );
        assert_eq!(
            filename_from_disposition(r#"attachment; filename="My Sub.yml""#).as_deref(),
            Some("My Sub")
        );
    }

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
            update_interval_hours: None,
            profile_override_path: None,
            active: false,
            subscription: None,
            updated_at: None,
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

    #[test]
    fn remote_profiles_are_due_after_their_interval() {
        let temp = unique_temp_dir();
        let mut store = ProfileStore::load(temp.join("profiles.json")).unwrap();
        let now = super::unix_now();
        let remote = |id: &str, updated_at: Option<i64>, hours: Option<u32>| UserProfile {
            id: ProfileId(id.to_owned()),
            name: id.to_owned(),
            source: ProfileSource::RemoteUrl {
                url: format!("https://example.com/{id}"),
                headers: Vec::new(),
            },
            source_path: temp.join(id),
            generated_path: temp.join(id),
            profile_override_path: None,
            active: false,
            subscription: None,
            updated_at,
            update_interval_hours: hours,
        };
        store.upsert(remote("fresh", Some(now - 3600), None));
        store.upsert(remote("stale", Some(now - 13 * 3600), None));
        store.upsert(remote("hourly", Some(now - 2 * 3600), Some(1)));
        store.upsert(remote("never", None, None));

        let mut due: Vec<String> = store
            .due_for_refresh()
            .into_iter()
            .map(|(id, _)| id.0)
            .collect();
        due.sort();
        assert_eq!(due, ["hourly", "never", "stale"]);
    }

    fn unique_temp_dir() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(Path::new(&format!("clash-rs-profile-test-{nonce}")))
    }
}
