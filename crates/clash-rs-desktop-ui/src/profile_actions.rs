
use std::path::Path;

use clash_rs_desktop_core::{CoreState, ProfileId, ProfileSource, ProfileStore, UserProfile};

use crate::{
    paths::reactor_paths,
    runtime_actions::managed_runtime,
};

#[derive(Clone, Debug)]
pub(crate) struct ProfileSummary {
    pub(crate) id: ProfileId,
    name: String,
    source: ProfileSource,
    source_path: String,
    override_path: Option<String>,
    active: bool,
}

impl ProfileSummary {
    pub(crate) fn display_label(&self) -> String {
        let marker = if self.active { "*" } else { " " };
        format!("{marker} {} ({})", self.name, self.id.0)
    }
}

pub(crate) fn reactor_profile_summaries() -> anyhow::Result<Vec<ProfileSummary>> {
    let paths = reactor_paths()?;
    let store = ProfileStore::load(paths.profile_manifest_path())?;

    Ok(store
        .profiles()
        .iter()
        .map(|profile| ProfileSummary {
            id: profile.id.clone(),
            name: profile.name.clone(),
            source: profile.source.clone(),
            source_path: profile.source_path.display().to_string(),
            override_path: profile
                .profile_override_path
                .as_ref()
                .map(|path| path.display().to_string()),
            active: profile.active,
        })
        .collect())
}

pub(crate) fn normalize_profile_index(index: i32, len: usize) -> i32 {
    if len == 0 {
        -1
    } else if index < 0 || index as usize >= len {
        0
    } else {
        index
    }
}

fn selected_profile_id(index: i32) -> anyhow::Result<ProfileId> {
    let profiles = reactor_profile_summaries()?;
    let index = normalize_profile_index(index, profiles.len());
    if index < 0 {
        anyhow::bail!("no profile is selected");
    }

    Ok(profiles[index as usize].id.clone())
}

pub(crate) fn reactor_profile_details_text(index: i32) -> String {
    let profiles = match reactor_profile_summaries() {
        Ok(profiles) => profiles,
        Err(error) => return format!("profiles: failed to load ({error})"),
    };

    let index = normalize_profile_index(index, profiles.len());
    if index < 0 {
        return "profiles: no profiles configured".to_owned();
    }

    let profile = &profiles[index as usize];
    let mut lines = vec![
        format!("profile: {}", profile.name),
        format!("id: {}", profile.id.0),
        format!("active: {}", profile.active),
        format!("source: {}", describe_profile_source(&profile.source)),
        format!("managed source path: {}", profile.source_path),
    ];

    if let Some(path) = &profile.override_path {
        lines.push(format!("override path: {path}"));
    }

    lines.join("\n")
}

pub(crate) fn reactor_import_profile_text(name: &str, source_path: &str) -> String {
    let name = name.trim();
    let source_path = source_path.trim().trim_matches('"');
    if name.is_empty() {
        return "import profile: profile name is required".to_owned();
    }
    if source_path.is_empty() {
        return "import profile: YAML file path is required".to_owned();
    }

    let source_path = Path::new(source_path);
    if !source_path.exists() {
        return format!(
            "import profile: file does not exist ({})",
            source_path.display()
        );
    }
    if !source_path.is_file() {
        return format!(
            "import profile: path is not a file ({})",
            source_path.display()
        );
    }

    match import_profile(name, source_path) {
        Ok(id) => format!(
            "import profile: imported {name} as {}\n{}",
            id.0,
            profiles_text()
        ),
        Err(error) => format!("import profile failed: {error}"),
    }
}

pub(crate) fn reactor_download_profile_text(name: &str, url: &str) -> String {
    let name = name.trim();
    let url = url.trim();
    if name.is_empty() {
        return "download profile: profile name is required".to_owned();
    }
    if url.is_empty() {
        return "download profile: subscription URL is required".to_owned();
    }
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return "download profile: URL must start with http:// or https://".to_owned();
    }

    match download_profile(name, url) {
        Ok(id) => format!(
            "download profile: downloaded {name} as {}\n{}",
            id.0,
            profiles_text()
        ),
        Err(error) => format!("download profile failed: {error}"),
    }
}

pub(crate) fn reactor_save_profile_override_form_text(
    index: i32,
    mode: &str,
    log_level: &str,
    mixed_port: &str,
    allow_lan: bool,
    ipv6: bool,
) -> String {
    let mut override_yaml = serde_yaml::Mapping::new();
    insert_yaml_string(&mut override_yaml, "mode", mode);
    insert_yaml_string(&mut override_yaml, "log-level", log_level);
    if let Some(port) = parse_optional_port(mixed_port) {
        override_yaml.insert("mixed-port".into(), serde_yaml::Value::Number(port.into()));
    }
    override_yaml.insert("allow-lan".into(), serde_yaml::Value::Bool(allow_lan));
    override_yaml.insert("ipv6".into(), serde_yaml::Value::Bool(ipv6));

    let contents = match serde_yaml::to_string(&override_yaml) {
        Ok(contents) => contents,
        Err(error) => return format!("save profile override failed: {error}"),
    };

    reactor_save_profile_override_text(index, &contents)
}

pub(crate) fn reactor_profile_override_form(index: i32) -> ProfileOverrideForm {
    let contents = load_profile_override(index).unwrap_or_default();
    let value = serde_yaml::from_str::<serde_yaml::Value>(&contents).unwrap_or_default();
    ProfileOverrideForm {
        mode: yaml_string(&value, "mode"),
        log_level: yaml_string(&value, "log-level"),
        mixed_port: yaml_i64(&value, "mixed-port"),
        allow_lan: yaml_bool(&value, "allow-lan"),
        ipv6: yaml_bool(&value, "ipv6"),
        raw_yaml: contents,
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ProfileOverrideForm {
    pub(crate) mode: String,
    pub(crate) log_level: String,
    pub(crate) mixed_port: String,
    pub(crate) allow_lan: bool,
    pub(crate) ipv6: bool,
    pub(crate) raw_yaml: String,
}

fn import_profile(name: &str, source_path: &Path) -> anyhow::Result<ProfileId> {
    let paths = reactor_paths()?;
    let mut store = ProfileStore::load(paths.profile_manifest_path())?;
    let id = store.import_local_file(&paths, name, source_path)?;
    store.save()?;
    Ok(id)
}

fn download_profile(name: &str, url: &str) -> anyhow::Result<ProfileId> {
    let contents = download_profile_yaml(url)?;

    let paths = reactor_paths()?;
    let mut store = ProfileStore::load(paths.profile_manifest_path())?;
    let id = unique_profile_id(store.profiles(), name);
    let profile_dir = paths.profiles_dir.join(&id.0);
    std::fs::create_dir_all(&profile_dir)?;

    let source_path = profile_dir.join("source.yaml");
    std::fs::write(&source_path, contents)?;

    let profile = UserProfile {
        id: id.clone(),
        name: name.to_owned(),
        source: ProfileSource::RemoteUrl {
            url: url.to_owned(),
            headers: Vec::new(),
        },
        source_path,
        generated_path: profile_dir.join("generated.yaml"),
        profile_override_path: Some(profile_dir.join("override.yaml")),
        active: false,
    };

    store.upsert(profile);
    if store.manifest().active_profile_id.is_none() {
        store.set_active(&id)?;
    }
    store.save()?;

    Ok(id)
}

fn download_profile_yaml(url: &str) -> anyhow::Result<String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let contents = runtime.block_on(async {
        let response = reqwest::Client::new()
            .get(url)
            .header(reqwest::header::USER_AGENT, "clash-rs-desktop")
            .send()
            .await?
            .error_for_status()?;
        anyhow::Ok(response.text().await?)
    })?;

    serde_yaml::from_str::<serde_yaml::Value>(&contents)
        .map_err(|error| anyhow::anyhow!("downloaded profile is not valid YAML: {error}"))?;

    Ok(contents)
}

fn unique_profile_id(profiles: &[UserProfile], name: &str) -> ProfileId {
    let base = sanitize_profile_id(name);
    if !profiles.iter().any(|profile| profile.id.0 == base) {
        return ProfileId(base);
    }

    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    ProfileId(format!("{base}-{suffix}"))
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

pub(crate) fn reactor_set_active_profile_text(index: i32) -> String {
    if let Some(message) = reject_profile_mutation_while_running("set active profile") {
        return message;
    }

    let id = match selected_profile_id(index) {
        Ok(id) => id,
        Err(error) => return format!("set active profile failed: {error}"),
    };

    match set_active_profile(&id) {
        Ok(()) => format!("set active profile: {}\n{}", id.0, profiles_text()),
        Err(error) => format!("set active profile failed: {error}"),
    }
}

pub(crate) fn reactor_update_profile_source_text(index: i32) -> String {
    if let Some(message) = reject_profile_mutation_while_running("update profile source") {
        return message;
    }

    match update_profile_source(index) {
        Ok(name) => format!("update profile source: {name}\n{}", profiles_text()),
        Err(error) => format!("update profile source failed: {error}"),
    }
}

fn update_profile_source(index: i32) -> anyhow::Result<String> {
    let paths = reactor_paths()?;
    let store = ProfileStore::load(paths.profile_manifest_path())?;
    let id = selected_profile_id(index)?;
    let profile = store
        .profiles()
        .iter()
        .find(|profile| profile.id == id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("profile {} does not exist", id.0))?;

    match &profile.source {
        ProfileSource::RemoteUrl { url, .. } => {
            let contents = download_profile_yaml(url)?;
            std::fs::write(&profile.source_path, contents)?;
        }
        ProfileSource::LocalFile { path } => {
            std::fs::copy(path, &profile.source_path)?;
        }
        ProfileSource::InlineTemplate => {
            std::fs::write(
                &profile.source_path,
                clash_rs_desktop_core::default_profile_yaml(),
            )?;
        }
    }

    Ok(profile.name)
}

fn set_active_profile(id: &ProfileId) -> anyhow::Result<()> {
    let paths = reactor_paths()?;
    let mut store = ProfileStore::load(paths.profile_manifest_path())?;
    store.set_active(id)?;
    store.save()?;
    Ok(())
}

pub(crate) fn reactor_delete_profile_text(index: i32) -> String {
    if let Some(message) = reject_profile_mutation_while_running("delete profile") {
        return message;
    }

    let id = match selected_profile_id(index) {
        Ok(id) => id,
        Err(error) => return format!("delete profile failed: {error}"),
    };

    match delete_profile(&id) {
        Ok(()) => format!("delete profile: {}\n{}", id.0, profiles_text()),
        Err(error) => format!("delete profile failed: {error}"),
    }
}

fn delete_profile(id: &ProfileId) -> anyhow::Result<()> {
    let paths = reactor_paths()?;
    let mut store = ProfileStore::load(paths.profile_manifest_path())?;
    let removed = store
        .remove(id)
        .ok_or_else(|| anyhow::anyhow!("profile {} does not exist", id.0))?;

    if store.profiles().is_empty() {
        store.ensure_default_profile(&paths)?;
    } else if store.active_profile().is_none() {
        let next_id = store
            .profiles()
            .first()
            .expect("profiles is not empty")
            .id
            .clone();
        store.set_active(&next_id)?;
    }
    store.save()?;

    if let Some(profile_dir) = removed.source_path.parent()
        && profile_dir.starts_with(&paths.profiles_dir)
        && profile_dir != paths.profiles_dir
    {
        let _ = std::fs::remove_dir_all(profile_dir);
    }

    Ok(())
}

pub(crate) fn reactor_ensure_default_profile_text() -> String {
    match ensure_default_profile() {
        Ok(id) => format!("ensure default profile: {}\n{}", id.0, profiles_text()),
        Err(error) => format!("ensure default profile failed: {error}"),
    }
}

fn ensure_default_profile() -> anyhow::Result<ProfileId> {
    let paths = reactor_paths()?;
    let mut store = ProfileStore::load(paths.profile_manifest_path())?;
    let id = store.ensure_default_profile(&paths)?;
    store.save()?;
    Ok(id)
}

pub(crate) fn reactor_load_profile_override_text(index: i32) -> String {
    match load_profile_override(index) {
        Ok(contents) if contents.is_empty() => "profile override is empty".to_owned(),
        Ok(contents) => contents,
        Err(error) => format!("load profile override failed: {error}"),
    }
}

pub(crate) fn reactor_save_profile_override_text(index: i32, contents: &str) -> String {
    if let Some(message) = reject_profile_mutation_while_running("save profile override") {
        return message;
    }

    match save_profile_override(index, contents) {
        Ok(path) => format!("profile override saved\npath: {}", path.display()),
        Err(error) => format!("save profile override failed: {error}"),
    }
}

fn load_profile_override(index: i32) -> anyhow::Result<String> {
    let paths = reactor_paths()?;
    let store = ProfileStore::load(paths.profile_manifest_path())?;
    let id = selected_profile_id(index)?;
    let profile = store
        .profiles()
        .iter()
        .find(|profile| profile.id == id)
        .ok_or_else(|| anyhow::anyhow!("profile {} does not exist", id.0))?;

    let Some(path) = &profile.profile_override_path else {
        return Ok(String::new());
    };
    if !path.exists() {
        return Ok(String::new());
    }

    Ok(std::fs::read_to_string(path)?)
}

fn save_profile_override(index: i32, contents: &str) -> anyhow::Result<std::path::PathBuf> {
    let paths = reactor_paths()?;
    let id = selected_profile_id(index)?;
    let mut store = ProfileStore::load(paths.profile_manifest_path())?;
    let mut profile = store
        .profiles()
        .iter()
        .find(|profile| profile.id == id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("profile {} does not exist", id.0))?;

    let profile_dir = profile
        .source_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("profile source has no parent directory"))?
        .to_path_buf();
    std::fs::create_dir_all(&profile_dir)?;
    let path = profile_dir.join("profile-override.yaml");
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        profile.profile_override_path = None;
    } else {
        std::fs::write(&path, contents)?;
        profile.profile_override_path = Some(path.clone());
    }
    store.upsert(profile);
    store.save()?;
    Ok(path)
}

fn profiles_text() -> String {
    match reactor_profile_summaries() {
        Ok(profiles) if profiles.is_empty() => "profiles: none".to_owned(),
        Ok(profiles) => profiles
            .iter()
            .map(ProfileSummary::display_label)
            .collect::<Vec<_>>()
            .join("\n"),
        Err(error) => format!("profiles: failed to load ({error})"),
    }
}

fn insert_yaml_string(map: &mut serde_yaml::Mapping, key: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        map.insert(key.into(), serde_yaml::Value::String(value.to_owned()));
    }
}

fn parse_optional_port(value: &str) -> Option<i64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    value
        .parse::<i64>()
        .ok()
        .filter(|port| (1..=65535).contains(port))
}

fn yaml_string(value: &serde_yaml::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn yaml_i64(value: &serde_yaml::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_yaml::Value::as_i64)
        .map(|value| value.to_string())
        .unwrap_or_default()
}

fn yaml_bool(value: &serde_yaml::Value, key: &str) -> bool {
    value
        .get(key)
        .and_then(serde_yaml::Value::as_bool)
        .unwrap_or(false)
}

fn describe_profile_source(source: &ProfileSource) -> String {
    match source {
        ProfileSource::LocalFile { path } => format!("local file ({})", path.display()),
        ProfileSource::RemoteUrl { url, .. } => format!("remote URL ({url})"),
        ProfileSource::InlineTemplate => "inline template".to_owned(),
    }
}

fn reject_profile_mutation_while_running(action: &str) -> Option<String> {
    let mut runtime = managed_runtime().lock().ok()?;
    let manager = runtime.as_mut()?;
    let state = manager.state();
    if matches!(
        state,
        CoreState::Starting { .. } | CoreState::Running { .. } | CoreState::Stopping
    ) {
        Some(format!(
            "{action}: stop the managed core before changing profiles"
        ))
    } else {
        None
    }
}
