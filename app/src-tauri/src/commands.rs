//! Tauri commands plus the connect/disconnect flow shared with the tray.
//!
//! Errors cross the IPC boundary as plain user-facing strings.

use std::{path::PathBuf, time::Duration};

use clash_rs_desktop_core::{
    ConnectMode, ControllerConfigPatch, CustomRule, NeedsServiceInstall, ProfileId, ProfileSource,
    RoutingMode, RuleAction, RuleKind, engine::CoreUpdateInfo, fetch_subscription,
};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::watch;

use crate::{
    nodes::{self, NodesView},
    state::{AppState, Phase, View, publish, sync_from_engine},
};

type CmdResult<T> = Result<T, String>;

fn user_error(error: anyhow::Error) -> String {
    error.to_string()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

// ---- connection lifecycle ---------------------------------------------------

pub async fn connect_flow(app: &AppHandle) -> CmdResult<View> {
    publish(app, |view| {
        view.phase = Phase::Connecting;
        view.step = None;
        view.error = None;
        view.needs_install = None;
    });

    let state = app.state::<AppState>();
    let mut engine = state.engine.lock().await;
    let step_app = app.clone();
    let on_step = move |step: &str| {
        let step = step.to_owned();
        publish(&step_app, |view| view.step = Some(step));
    };

    match engine.connect(&on_step).await {
        Ok(()) => {
            start_traffic(app, engine.controller());
            Ok(publish(app, |view| {
                sync_from_engine(view, &engine);
                view.phase = Phase::Connected;
                view.step = None;
                view.connected_since = Some(now_ms());
            }))
        }
        Err(error) => {
            let needs_install = error.downcast_ref::<NeedsServiceInstall>().copied();
            let message = user_error(error);
            publish(app, |view| {
                sync_from_engine(view, &engine);
                view.phase = Phase::Disconnected;
                view.step = None;
                // The install prompt explains itself; no extra error toast.
                view.needs_install = needs_install;
                view.error = needs_install.is_none().then(|| message.clone());
            });
            Err(message)
        }
    }
}

pub async fn disconnect_flow(app: &AppHandle) -> CmdResult<View> {
    publish(app, |view| {
        view.phase = Phase::Disconnecting;
        view.error = None;
    });
    stop_traffic(app);

    let state = app.state::<AppState>();
    let mut engine = state.engine.lock().await;
    let result = engine.disconnect().await;
    Ok(publish(app, |view| {
        sync_from_engine(view, &engine);
        view.phase = Phase::Disconnected;
        view.connected_since = None;
        if let Err(error) = &result {
            view.error = Some(format!("断开时出错：{error}"));
        }
    }))
}

pub async fn toggle_flow(app: &AppHandle) -> CmdResult<View> {
    match app.state::<AppState>().view().phase {
        Phase::Disconnected => connect_flow(app).await,
        Phase::Connected => disconnect_flow(app).await,
        // Mid-transition: ignore repeated clicks.
        Phase::Connecting | Phase::Disconnecting => Ok(app.state::<AppState>().view()),
    }
}

/// Apply config changes. Hot-reloads the running core; falls back to a
/// reconnect if the core can't take the new config live.
async fn apply_config_change(app: &AppHandle) -> CmdResult<View> {
    let state = app.state::<AppState>();
    if state.view().phase != Phase::Connected {
        let engine = state.engine.lock().await;
        return Ok(publish(app, |view| sync_from_engine(view, &engine)));
    }
    let reloaded = state.engine.lock().await.reload().await;
    match reloaded {
        Ok(()) => {
            let engine = state.engine.lock().await;
            Ok(publish(app, |view| sync_from_engine(view, &engine)))
        }
        // A config the core rejects should not take the connection down.
        Err(error)
            if ["配置有误", "自定义配置"].iter().any(|p| error.to_string().starts_with(p)) =>
        {
            Err(user_error(error))
        }
        Err(_) => reconnect(app).await,
    }
}

async fn reconnect(app: &AppHandle) -> CmdResult<View> {
    disconnect_flow(app).await?;
    connect_flow(app).await
}

fn start_traffic(app: &AppHandle, client: Option<clash_rs_desktop_core::ClashIpcClient>) {
    stop_traffic(app);
    let Some(client) = client else { return };

    let (stop_tx, stop_rx) = watch::channel(false);
    *app.state::<AppState>()
        .traffic_stop
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(stop_tx);

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let emit_app = app.clone();
            let _ = client
                .stream_websocket_text(
                    "/traffic",
                    move |frame| {
                        if let Ok(value) = serde_json::from_str::<Value>(&frame) {
                            let _ = emit_app.emit("traffic", value);
                        }
                    },
                    stop_rx.clone(),
                )
                .await;
            // Stream dropped (core busy/reloading): retry until stopped.
            if *stop_rx.borrow() {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
            if *stop_rx.borrow() {
                break;
            }
        }
    });
}

fn stop_traffic(app: &AppHandle) {
    if let Some(stop) = app
        .state::<AppState>()
        .traffic_stop
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
    {
        let _ = stop.send(true);
    }
}

/// On startup: pick up a TUN core the service kept running while we were
/// closed, so the app shows "connected" instead of starting a second one.
pub async fn adopt_existing(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut engine = state.engine.lock().await;
    if engine.adopt_running_service().await {
        start_traffic(app, engine.controller());
        publish(app, |view| {
            sync_from_engine(view, &engine);
            view.phase = Phase::Connected;
            view.connected_since = Some(now_ms());
        });
    }
}

/// Background watchdog: notices a core that died on its own.
pub fn spawn_health_watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(2));
        loop {
            tick.tick().await;
            let state = app.state::<AppState>();
            if state.view().phase != Phase::Connected {
                continue;
            }
            // Never queue behind a long-running command; check next tick.
            let Ok(mut engine) = state.engine.try_lock() else {
                continue;
            };
            if let Some(reason) = engine.check_health().await {
                stop_traffic(&app);
                publish(&app, |view| {
                    sync_from_engine(view, &engine);
                    view.phase = Phase::Disconnected;
                    view.connected_since = None;
                    view.error = Some(reason);
                });
            }
        }
    });
}

/// Background: refresh subscriptions on the provider's interval.
pub fn spawn_subscription_updater(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Let startup (and auto-connect) settle first.
        tokio::time::sleep(Duration::from_secs(20)).await;
        let mut tick = tokio::time::interval(Duration::from_secs(30 * 60));
        loop {
            tick.tick().await;
            let due = {
                let state = app.state::<AppState>();
                let engine = state.engine.lock().await;
                if !engine.settings().settings().auto_update_subscriptions {
                    continue;
                }
                engine.profiles().due_for_refresh()
            };
            for (id, _) in due {
                // Failures are silent here; the user can refresh by hand.
                let _ = refresh_profile(&app, id).await;
            }
        }
    });
}

// ---- commands: connection ---------------------------------------------------

#[tauri::command]
pub fn get_view(state: State<'_, AppState>) -> View {
    state.view()
}

#[tauri::command]
pub async fn toggle_connection(app: AppHandle) -> CmdResult<View> {
    toggle_flow(&app).await
}

#[tauri::command]
pub async fn connect(app: AppHandle) -> CmdResult<View> {
    connect_flow(&app).await
}

#[tauri::command]
pub fn dismiss_error(app: AppHandle) -> View {
    publish(&app, |view| {
        view.error = None;
        view.needs_install = None;
    })
}

#[tauri::command]
pub async fn set_connect_mode(app: AppHandle, mode: ConnectMode) -> CmdResult<View> {
    {
        let state = app.state::<AppState>();
        let mut engine = state.engine.lock().await;
        let store = engine.settings_mut();
        store.settings_mut().connect_mode = mode;
        store.save().map_err(user_error)?;
        publish(&app, |view| sync_from_engine(view, &engine));
    }
    if app.state::<AppState>().view().phase == Phase::Connected {
        return reconnect(&app).await;
    }
    Ok(app.state::<AppState>().view())
}

fn service_exe(app: &AppHandle) -> PathBuf {
    let name = format!("clash-rs-desktop-service{}", std::env::consts::EXE_SUFFIX);
    // Installed: bundled as a resource. Dev: built next to the app exe.
    app.path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join(&name))
        .filter(|path| path.exists())
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|dir| dir.join(&name)))
        })
        .unwrap_or_else(|| PathBuf::from(&name))
}

/// One UAC prompt (pkexec on Linux), then connect.
#[tauri::command]
pub async fn install_tun_support(app: AppHandle) -> CmdResult<View> {
    publish(&app, |view| {
        view.step = Some("正在等待授权…".to_owned());
        view.phase = Phase::Connecting;
    });
    let result = {
        let state = app.state::<AppState>();
        let engine = state.engine.lock().await;
        engine.install_tun_support(&service_exe(&app)).await
    };
    if let Err(error) = result {
        let message = user_error(error);
        publish(&app, |view| {
            view.phase = Phase::Disconnected;
            view.step = None;
            view.error = Some(message.clone());
        });
        return Err(message);
    }
    publish(&app, |view| view.needs_install = None);
    connect_flow(&app).await
}

// ---- commands: subscriptions ------------------------------------------------

#[tauri::command]
pub async fn add_subscription(
    app: AppHandle,
    url: String,
    name: Option<String>,
) -> CmdResult<View> {
    // Download outside the engine lock so the UI stays responsive.
    let mut fetched = fetch_subscription(&url).await.map_err(user_error)?;
    // A name given by the provider's import link beats header guesses.
    if let Some(name) = name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()) {
        fetched.suggested_name = Some(name);
    }
    {
        let state = app.state::<AppState>();
        let mut engine = state.engine.lock().await;
        let paths = engine.paths().clone();
        let profiles = engine.profiles_mut();
        let id = profiles.import_remote(&paths, fetched).map_err(user_error)?;
        // A newly added subscription is what the user wants to use now.
        profiles.set_active(&id).map_err(user_error)?;
        profiles.save().map_err(user_error)?;
    }
    apply_config_change(&app).await
}

async fn refresh_profile(app: &AppHandle, id: ProfileId) -> CmdResult<View> {
    let state = app.state::<AppState>();
    let (url, active) = {
        let engine = state.engine.lock().await;
        let profile = engine
            .profiles()
            .profile(&id)
            .ok_or_else(|| "订阅不存在".to_owned())?;
        let ProfileSource::RemoteUrl { url, .. } = &profile.source else {
            return Err("这个配置不是订阅，无法更新".to_owned());
        };
        (url.clone(), profile.active)
    };

    let fetched = fetch_subscription(&url).await.map_err(user_error)?;
    {
        let mut engine = state.engine.lock().await;
        let profiles = engine.profiles_mut();
        profiles.apply_refresh(&id, fetched).map_err(user_error)?;
        profiles.save().map_err(user_error)?;
    }
    if active {
        apply_config_change(app).await
    } else {
        let engine = state.engine.lock().await;
        Ok(publish(app, |view| sync_from_engine(view, &engine)))
    }
}

#[tauri::command]
pub async fn refresh_subscription(app: AppHandle, id: String) -> CmdResult<View> {
    refresh_profile(&app, ProfileId(id)).await
}

#[tauri::command]
pub async fn select_profile(app: AppHandle, id: String) -> CmdResult<View> {
    {
        let state = app.state::<AppState>();
        let mut engine = state.engine.lock().await;
        let profiles = engine.profiles_mut();
        profiles.set_active(&ProfileId(id)).map_err(user_error)?;
        profiles.save().map_err(user_error)?;
    }
    apply_config_change(&app).await
}

#[tauri::command]
pub async fn remove_profile(app: AppHandle, id: String) -> CmdResult<View> {
    let state = app.state::<AppState>();
    let id = ProfileId(id);
    let was_active = {
        let engine = state.engine.lock().await;
        engine.profiles().active_profile().is_some_and(|p| p.id == id)
    };

    let has_other = {
        let mut engine = state.engine.lock().await;
        let profiles_dir = engine.paths().profiles_dir.clone();
        let profiles = engine.profiles_mut();
        if profiles.remove(&id).is_some() {
            let _ = std::fs::remove_dir_all(profiles_dir.join(&id.0));
        }
        // Keep something active so the big button keeps working.
        let next = profiles.profiles().first().map(|p| p.id.clone());
        if profiles.active_profile().is_none()
            && let Some(next) = &next
        {
            profiles.set_active(next).map_err(user_error)?;
        }
        profiles.save().map_err(user_error)?;
        next.is_some()
    };

    if was_active && state.view().phase == Phase::Connected {
        if has_other {
            return apply_config_change(&app).await;
        }
        return disconnect_flow(&app).await;
    }
    let engine = state.engine.lock().await;
    Ok(publish(&app, |view| sync_from_engine(view, &engine)))
}

#[tauri::command]
pub async fn set_auto_update_subscriptions(app: AppHandle, enabled: bool) -> CmdResult<View> {
    let state = app.state::<AppState>();
    let mut engine = state.engine.lock().await;
    let store = engine.settings_mut();
    store.settings_mut().auto_update_subscriptions = enabled;
    store.save().map_err(user_error)?;
    Ok(publish(&app, |view| sync_from_engine(view, &engine)))
}

// ---- commands: settings -----------------------------------------------------

#[tauri::command]
pub async fn set_routing_mode(app: AppHandle, mode: RoutingMode) -> CmdResult<View> {
    let state = app.state::<AppState>();
    let mut engine = state.engine.lock().await;
    let store = engine.settings_mut();
    store.settings_mut().routing_mode = mode;
    store.save().map_err(user_error)?;

    // Apply live; the setting is also baked into the next generated config.
    if let Some(client) = engine.controller() {
        let patch = ControllerConfigPatch {
            mode: Some(mode.as_clash().to_owned()),
            ..Default::default()
        };
        client.patch_configs(&patch).await.map_err(user_error)?;
    }
    Ok(publish(&app, |view| sync_from_engine(view, &engine)))
}

#[tauri::command]
pub async fn set_auto_connect(app: AppHandle, enabled: bool) -> CmdResult<View> {
    let state = app.state::<AppState>();
    let mut engine = state.engine.lock().await;
    let store = engine.settings_mut();
    store.settings_mut().auto_connect = enabled;
    store.save().map_err(user_error)?;
    Ok(publish(&app, |view| sync_from_engine(view, &engine)))
}

#[tauri::command]
pub async fn add_custom_rule(
    app: AppHandle,
    kind: RuleKind,
    value: String,
    action: RuleAction,
) -> CmdResult<View> {
    let rule = CustomRule::validated(kind, &value, action).map_err(user_error)?;
    {
        let state = app.state::<AppState>();
        let mut engine = state.engine.lock().await;
        let store = engine.settings_mut();
        let rules = &mut store.settings_mut().custom_rules;
        // Same target again: replace instead of stacking duplicates.
        rules.retain(|existing| !(existing.kind == rule.kind && existing.value == rule.value));
        rules.insert(0, rule);
        store.save().map_err(user_error)?;
    }
    apply_config_change(&app).await
}

#[tauri::command]
pub async fn remove_custom_rule(app: AppHandle, index: usize) -> CmdResult<View> {
    {
        let state = app.state::<AppState>();
        let mut engine = state.engine.lock().await;
        let store = engine.settings_mut();
        let rules = &mut store.settings_mut().custom_rules;
        if index < rules.len() {
            rules.remove(index);
        }
        store.save().map_err(user_error)?;
    }
    apply_config_change(&app).await
}

#[tauri::command]
pub async fn check_core_update(state: State<'_, AppState>) -> CmdResult<CoreUpdateInfo> {
    let engine = state.engine.lock().await;
    engine.check_core_update().await.map_err(user_error)
}

#[tauri::command]
pub async fn update_core(app: AppHandle) -> CmdResult<View> {
    {
        let state = app.state::<AppState>();
        let mut engine = state.engine.lock().await;
        engine.update_core().await.map_err(user_error)?;
    }
    let view = if app.state::<AppState>().view().phase == Phase::Connected {
        reconnect(&app).await?
    } else {
        let state = app.state::<AppState>();
        let engine = state.engine.lock().await;
        publish(&app, |view| sync_from_engine(view, &engine))
    };
    Ok(view)
}

// ---- commands: nodes, logs, advanced ------------------------------------------

async fn controller(state: &AppState) -> CmdResult<clash_rs_desktop_core::ClashIpcClient> {
    state
        .engine
        .lock()
        .await
        .controller()
        .ok_or_else(|| "尚未连接".to_owned())
}

#[tauri::command]
pub async fn get_nodes(state: State<'_, AppState>) -> CmdResult<NodesView> {
    let client = controller(&state).await?;
    let proxies = client.get_proxies().await.map_err(user_error)?;
    Ok(nodes::build(&proxies.proxies))
}

#[tauri::command]
pub async fn select_node(
    state: State<'_, AppState>,
    group: String,
    name: String,
) -> CmdResult<NodesView> {
    let client = controller(&state).await?;
    client.select_proxy(&group, &name).await.map_err(user_error)?;
    // Drop connections still pinned to the old node so the switch is felt now.
    let _ = client.close_all_connections().await;
    let proxies = client.get_proxies().await.map_err(user_error)?;
    Ok(nodes::build(&proxies.proxies))
}

#[tauri::command]
pub async fn test_delays(state: State<'_, AppState>, group: String) -> CmdResult<NodesView> {
    let client = controller(&state).await?;
    // Failures (timeouts) are recorded in history as delay 0; still rebuild.
    let _ = client
        .test_group_delay(&group, "https://www.gstatic.com/generate_204", 5000)
        .await;
    let proxies = client.get_proxies().await.map_err(user_error)?;
    Ok(nodes::build(&proxies.proxies))
}

#[tauri::command]
pub async fn core_logs(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    Ok(state.engine.lock().await.core_log_tail(200).await)
}

#[tauri::command]
pub async fn read_override(state: State<'_, AppState>) -> CmdResult<String> {
    let path = state.engine.lock().await.paths().global_override_path();
    Ok(std::fs::read_to_string(path).unwrap_or_default())
}

#[tauri::command]
pub async fn write_override(app: AppHandle, text: String) -> CmdResult<View> {
    if !text.trim().is_empty() {
        let value: serde_yaml::Value =
            serde_yaml::from_str(&text).map_err(|e| format!("YAML 格式有误：{e}"))?;
        if !value.is_mapping() {
            return Err("自定义配置的顶层必须是键值对".to_owned());
        }
    }
    let path = app
        .state::<AppState>()
        .engine
        .lock()
        .await
        .paths()
        .global_override_path();
    let previous = std::fs::read_to_string(&path).unwrap_or_default();
    std::fs::write(&path, &text).map_err(|e| e.to_string())?;

    let result = apply_config_change(&app).await;
    if result.is_err() {
        // The core rejected it: keep the last working version on disk.
        let _ = std::fs::write(&path, previous);
    }
    result
}

// ---- deep links ---------------------------------------------------------------

/// `choclite://install-config?url=...` / `clash://install-config?url=...`
/// → ask the UI to confirm the import (never import silently).
pub fn handle_deep_links(app: &AppHandle, urls: impl IntoIterator<Item = String>) {
    for raw in urls {
        let Ok(url) = reqwest_url(&raw) else { continue };
        if !matches!(url.scheme(), "choclite" | "clash") || url.host_str() != Some("install-config") {
            continue;
        }
        let Some(sub) = url
            .query_pairs()
            .find(|(key, _)| key == "url")
            .map(|(_, value)| value.into_owned())
        else {
            continue;
        };
        let name = url
            .query_pairs()
            .find(|(key, _)| key == "name")
            .map(|(_, value)| value.into_owned());
        crate::tray::show_main_window(app);
        let payload = serde_json::json!({ "url": sub, "name": name });
        // Kept until the UI takes it: on a cold start the page isn't
        // listening yet when the link arrives.
        *pending_import().lock().unwrap_or_else(|e| e.into_inner()) = Some(payload.clone());
        let _ = app.emit("deep-link-import", payload);
    }
}

fn pending_import() -> &'static std::sync::Mutex<Option<Value>> {
    static PENDING: std::sync::OnceLock<std::sync::Mutex<Option<Value>>> = std::sync::OnceLock::new();
    PENDING.get_or_init(Default::default)
}

#[tauri::command]
pub fn take_deep_link() -> Option<Value> {
    pending_import().lock().unwrap_or_else(|e| e.into_inner()).take()
}

fn reqwest_url(raw: &str) -> Result<tauri::Url, ()> {
    tauri::Url::parse(raw.trim()).map_err(|_| ())
}
