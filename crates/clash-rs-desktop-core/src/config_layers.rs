use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigLayers {
    pub base_profile_path: PathBuf,
    pub global_override_path: Option<PathBuf>,
    pub profile_override_path: Option<PathBuf>,
    pub shell_runtime_override: Value,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfigBuilder {
    layers: Vec<Value>,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfigManager {
    pub layers: ConfigLayers,
    pub output_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GeneratedRuntimeConfig {
    pub path: PathBuf,
    pub value: Value,
}

impl RuntimeConfigBuilder {
    pub fn new(base_profile: Value) -> Self {
        Self {
            layers: vec![base_profile],
        }
    }

    pub fn push_override(mut self, value: Value) -> Self {
        self.layers.push(value);
        self
    }

    pub fn build(self) -> Value {
        let mut merged = Value::Mapping(Mapping::new());
        for layer in self.layers {
            merge_yaml(&mut merged, layer);
        }
        merged
    }
}

impl RuntimeConfigManager {
    pub fn new(layers: ConfigLayers, output_path: PathBuf) -> Self {
        Self {
            layers,
            output_path,
        }
    }

    pub fn generate(&self) -> anyhow::Result<GeneratedRuntimeConfig> {
        let base_profile = read_yaml_file(&self.layers.base_profile_path)?;
        let mut builder = RuntimeConfigBuilder::new(base_profile);

        if let Some(path) = &self.layers.global_override_path {
            builder = builder.push_override(read_yaml_file(path)?);
        }

        if let Some(path) = &self.layers.profile_override_path {
            builder = builder.push_override(read_yaml_file(path)?);
        }

        let value = builder
            .push_override(self.layers.shell_runtime_override.clone())
            .build();

        if let Some(parent) = self.output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let yaml = serde_yaml::to_string(&value)?;
        fs::write(&self.output_path, yaml)?;

        Ok(GeneratedRuntimeConfig {
            path: self.output_path.clone(),
            value,
        })
    }
}

pub fn shell_runtime_override(pipe_path: &str) -> Value {
    let mut root = Mapping::new();
    root.insert(
        Value::String("external-controller-pipe".to_owned()),
        Value::String(pipe_path.to_owned()),
    );
    Value::Mapping(root)
}

pub fn merge_yaml(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Mapping(base_map), Value::Mapping(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                match base_map.get_mut(&key) {
                    Some(base_value) => merge_yaml(base_value, overlay_value),
                    None => {
                        base_map.insert(key, overlay_value);
                    }
                }
            }
        }
        (base_value, overlay_value) => {
            *base_value = overlay_value;
        }
    }
}

fn read_yaml_file(path: &PathBuf) -> anyhow::Result<Value> {
    let contents = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&contents)?)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use serde_yaml::Value;

    use super::{ConfigLayers, RuntimeConfigBuilder, RuntimeConfigManager, shell_runtime_override};

    #[test]
    fn later_layers_override_earlier_layers() {
        let base: Value = serde_yaml::from_str("mixed-port: 7890\nmode: rule\n").unwrap();
        let global: Value = serde_yaml::from_str("mode: global\nallow-lan: false\n").unwrap();
        let runtime = shell_runtime_override(r"\\.\pipe\clash-rs-desktop");

        let merged = RuntimeConfigBuilder::new(base)
            .push_override(global)
            .push_override(runtime)
            .build();

        assert_eq!(merged["mixed-port"], Value::from(7890));
        assert_eq!(merged["mode"], Value::from("global"));
        assert_eq!(merged["allow-lan"], Value::from(false));
        assert_eq!(
            merged["external-controller-pipe"],
            Value::from(r"\\.\pipe\clash-rs-desktop")
        );
    }

    #[test]
    fn runtime_config_manager_writes_merged_yaml() {
        let temp = unique_temp_dir();
        fs::create_dir_all(&temp).unwrap();

        let base = temp.join("base.yaml");
        let global = temp.join("global.yaml");
        let profile = temp.join("profile.yaml");
        let output = temp.join("runtime").join("generated.yaml");

        fs::write(
            &base,
            "mixed-port: 7890\nmode: rule\ntun:\n  enable: false\n",
        )
        .unwrap();
        fs::write(&global, "mode: global\nallow-lan: false\n").unwrap();
        fs::write(&profile, "tun:\n  enable: true\n  route-all: true\n").unwrap();

        let manager = RuntimeConfigManager::new(
            ConfigLayers {
                base_profile_path: base,
                global_override_path: Some(global),
                profile_override_path: Some(profile),
                shell_runtime_override: shell_runtime_override(r"\\.\pipe\clash-rs-desktop-core"),
            },
            output.clone(),
        );

        let generated = manager.generate().unwrap();
        let written = fs::read_to_string(&output).unwrap();
        let written_value: Value = serde_yaml::from_str(&written).unwrap();

        assert_eq!(generated.path, output);
        assert_eq!(generated.value, written_value);
        assert_eq!(written_value["mode"], Value::from("global"));
        assert_eq!(written_value["tun"]["enable"], Value::from(true));
        assert_eq!(written_value["tun"]["route-all"], Value::from(true));
        assert_eq!(
            written_value["external-controller-pipe"],
            Value::from(r"\\.\pipe\clash-rs-desktop-core")
        );
    }

    fn unique_temp_dir() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(Path::new(&format!("clash-rs-desktop-test-{nonce}")))
    }
}
