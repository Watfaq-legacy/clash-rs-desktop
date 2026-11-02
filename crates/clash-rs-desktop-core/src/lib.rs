pub mod binary_update;
pub mod config_layers;
pub mod engine;
pub mod ipc;
pub mod paths;
pub mod process;
pub mod profile;
pub mod runtime;
pub mod service;
pub mod settings;
pub mod system_proxy;
pub mod tun;

pub use binary_update::{
    ActiveCore, CoreUpdateManager, CoreUpdateSettings, GitHubRelease, GitHubReleaseAsset,
    ManagedCoreFiles, ReleaseCandidate, UpdateChannel, UpdateCheckResult, UpdateDecision,
    WindowsArch, select_release_candidate,
};
pub use config_layers::{
    ConfigLayers, GeneratedRuntimeConfig, RuntimeConfigBuilder, RuntimeConfigManager,
    shell_runtime_override,
};
pub use ipc::{
    ClashIpcClient, ClashIpcClientConfig, ConfigReloadRequest, ConnectionsResponse,
    ControllerConfigPatch, ControllerConfigs, ControllerEndpoint, MemoryResponse,
    ProviderCollection, ProxyCollection, ProxySelectionRequest, RulesResponse, VersionInfo,
};
pub use paths::AppPaths;
pub use process::{
    ClashProcessManager, CoreLaunchRequest, CoreOwner, CoreState, ElevationReason,
    LaunchRequirements,
};
pub use engine::Engine;
pub use profile::{
    FetchedSubscription, ProfileId, ProfileManifest, ProfileSource, ProfileStore,
    SubscriptionInfo, UserProfile, default_profile_yaml, fetch_subscription,
};
pub use runtime::{
    CoreRuntimeManager, RuntimeLaunchOptions, RuntimeStatus, local_dev_runtime_options,
    runtime_options_from_active_profile,
};
pub use service::{ServiceCommand, ServiceResponse, ServiceSupervisor, pipe_transport};
pub use settings::{
    AppSettings, ConnectMode, RoutingMode, SettingsStore, SystemProxySettings, TunLaunchPolicy,
};
pub use system_proxy::{
    SystemProxyBackend, SystemProxySnapshot, SystemProxyTarget, WindowsSystemProxy,
};
pub use tun::{TunSettings, launch_requirements_from_config};
