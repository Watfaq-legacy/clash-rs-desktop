use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{
    ActiveCore, AppPaths, ClashIpcClient, ClashIpcClientConfig, ClashProcessManager, ConfigLayers,
    CoreLaunchRequest, CoreState, GeneratedRuntimeConfig, LaunchRequirements, ProfileStore,
    RuntimeConfigManager, VersionInfo,
    process::CoreLog,
    launch_requirements_from_config, shell_runtime_override,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeLaunchOptions {
    pub paths: AppPaths,
    pub active_core: ActiveCore,
    pub base_profile_path: PathBuf,
    pub global_override_path: Option<PathBuf>,
    pub profile_override_path: Option<PathBuf>,
    pub core_pipe_path: String,
    pub core_pid_path: PathBuf,
    /// Shell-owned settings layered on top of every user layer (ports, mode,
    /// TUN switch). Must be a mapping or null.
    #[serde(default)]
    pub extra_runtime_override: serde_yaml::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub state: CoreState,
    pub active_core: ActiveCore,
    pub runtime_config_path: PathBuf,
    pub version: Option<VersionInfo>,
}

#[derive(Debug)]
pub struct CoreRuntimeManager {
    options: RuntimeLaunchOptions,
    process: ClashProcessManager,
    generated_config: Option<GeneratedRuntimeConfig>,
}

impl CoreRuntimeManager {
    pub fn new(options: RuntimeLaunchOptions) -> Self {
        Self {
            options,
            process: ClashProcessManager::new(),
            generated_config: None,
        }
    }

    pub fn options(&self) -> &RuntimeLaunchOptions {
        &self.options
    }

    pub fn launch_request(&self) -> CoreLaunchRequest {
        CoreLaunchRequest {
            binary_path: self.options.active_core.binary_path.clone(),
            runtime_dir: self.options.paths.runtime_dir.clone(),
            config_path: self.runtime_config_path(),
            core_pipe_path: self.options.core_pipe_path.clone(),
        }
    }

    pub fn generate_config(&mut self) -> anyhow::Result<&GeneratedRuntimeConfig> {
        let manager = RuntimeConfigManager::new(
            ConfigLayers {
                base_profile_path: self.options.base_profile_path.clone(),
                global_override_path: self.options.global_override_path.clone(),
                profile_override_path: self.options.profile_override_path.clone(),
                shell_runtime_override: self.shell_override(),
            },
            self.runtime_config_path(),
        );

        self.generated_config = Some(manager.generate()?);
        Ok(self
            .generated_config
            .as_ref()
            .expect("generated config was just set"))
    }

    pub fn validate_config(&mut self) -> anyhow::Result<&GeneratedRuntimeConfig> {
        self.generate_config()?;
        self.launch_request().validate_config()?;
        Ok(self
            .generated_config
            .as_ref()
            .expect("generated config was just set"))
    }

    pub fn start_validated(&mut self) -> anyhow::Result<CoreState> {
        let generated = self.validate_config()?;
        let requirements = launch_requirements_from_config(&generated.value);

        if requirements.requires_service {
            return Ok(CoreState::NeedsService {
                reasons: requirements.reasons,
            });
        }

        self.process.start_direct(&self.launch_request())
    }

    pub fn start(&mut self, requirements: LaunchRequirements) -> anyhow::Result<CoreState> {
        if requirements.requires_service {
            return Ok(CoreState::NeedsService {
                reasons: requirements.reasons,
            });
        }

        self.generate_config()?;
        self.process.start_direct(&self.launch_request())
    }

    // The core runs inside a kill-on-close job object, so there is no orphan
    // left to hunt down via a PID file (which could also hit a reused PID).
    pub fn stop(&mut self) -> anyhow::Result<CoreState> {
        self.process.stop()
    }

    pub fn log(&self) -> &CoreLog {
        self.process.log()
    }

    pub fn state(&mut self) -> CoreState {
        self.process.state()
    }

    pub async fn probe_version(&self) -> anyhow::Result<VersionInfo> {
        self.ipc_client().get_version().await
    }

    pub async fn wait_until_ready(&self, timeout: Duration) -> anyhow::Result<VersionInfo> {
        let started = std::time::Instant::now();
        let mut last_error = None;

        while started.elapsed() < timeout {
            match self.probe_version().await {
                Ok(version) => return Ok(version),
                Err(error) => {
                    last_error = Some(error);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("timed out waiting for Clash core")))
    }

    pub async fn status(&mut self) -> RuntimeStatus {
        RuntimeStatus {
            state: self.state(),
            active_core: self.options.active_core.clone(),
            runtime_config_path: self.runtime_config_path(),
            version: self.probe_version().await.ok(),
        }
    }

    fn ipc_client(&self) -> ClashIpcClient {
        ClashIpcClient::new(ClashIpcClientConfig {
            pipe_path: self.options.core_pipe_path.clone(),
            auth_secret: None,
        })
    }

    fn shell_override(&self) -> serde_yaml::Value {
        let mut shell = shell_runtime_override(&self.options.core_pipe_path);
        if self.options.extra_runtime_override.is_mapping() {
            crate::config_layers::apply_overlay(
                &mut shell,
                self.options.extra_runtime_override.clone(),
            );
        }
        shell
    }

    fn runtime_config_path(&self) -> PathBuf {
        self.options.paths.runtime_dir.join("generated.yaml")
    }
}

pub fn local_dev_runtime_options(
    workspace_root: impl Into<PathBuf>,
    active_core: ActiveCore,
) -> RuntimeLaunchOptions {
    let workspace_root = workspace_root.into();
    let paths = AppPaths::from_program_data(workspace_root.join(".dev"));

    RuntimeLaunchOptions {
        base_profile_path: paths.runtime_dir.join("base.yaml"),
        global_override_path: None,
        profile_override_path: None,
        core_pipe_path: format!(r"\\.\pipe\clash-rs-desktop-dev-{}", std::process::id()),
        core_pid_path: paths.core_pid_path(),
        extra_runtime_override: serde_yaml::Value::Null,
        paths,
        active_core,
    }
}

pub fn runtime_options_from_active_profile(
    paths: AppPaths,
    active_core: ActiveCore,
    profile_store: &ProfileStore,
    global_override_path: Option<PathBuf>,
    core_pipe_path: String,
) -> anyhow::Result<RuntimeLaunchOptions> {
    let profile = profile_store
        .active_profile()
        .ok_or_else(|| anyhow::anyhow!("no active profile is configured"))?;

    Ok(RuntimeLaunchOptions {
        base_profile_path: profile.source_path.clone(),
        global_override_path: global_override_path.filter(|path| path.exists()),
        profile_override_path: profile
            .profile_override_path
            .as_ref()
            .filter(|path| path.exists())
            .cloned(),
        core_pipe_path,
        core_pid_path: paths.core_pid_path(),
        extra_runtime_override: serde_yaml::Value::Null,
        paths,
        active_core,
    })
}
