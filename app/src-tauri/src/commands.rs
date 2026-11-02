//! Tauri commands plus the connect/disconnect flow shared with the tray.
//!
//! Errors cross the IPC boundary as plain user-facing strings.

use std::time::Duration;

use clash_rs_desktop_core::{
    ControllerConfigPatch, ProfileId, ProfileSource, RoutingMode, fetch_subscription,
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
            let message = user_error(error);
            publish(app, |view| {
                sync_from_engine(view, &engine);
                view.phase = Phase::Disconnected;
                view.step = None;
                view.error = Some(message.clone());
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
    let result = engine.disconnect();
    let view = publish(app, |view| {
        sync_from_engine(view, &engine);
        view.phase = Phase::Disconnected;
        view.connected_since = None;
        if let Err(error) = &result {
            view.error = Some(format!("断开时出错：{error}"));
        }
    });
    Ok(view)
}

pub async fn toggle_flow(app: &AppHandle) -> CmdResult<View> {
    match app.state::<AppState>().view().phase {
        Phase::Disconnected => connect_flow(app).await,
        Phase::Connected => disconnect_flow(app).await,
        // Mid-transition: ignore repeated clicks.
        Phase::Connecting | Phase::Disconnecting => Ok(app.state::<AppState>().view()),
    }
}

/// Bounce the core so it picks up new config (profile switch, refresh).
async fn reconnect_if_connected(app: &AppHandle) -> CmdResult<View> {
    if app.state::<AppState>().view().phase == Phase::Connected {
        disconnect_flow(app).await?;
        return connect_flow(app).await;
    }
    Ok(app.state::<AppState>().view())
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
            // Stream dropped (core busy/restarting): retry until stopped.
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
            if let Some(reason) = engine.check_health() {
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

// ---- commands ---------------------------------------------------------------

#[tauri::command]
pub fn get_view(state: State<'_, AppState>) -> View {
    state.view()
}

#[tauri::command]
pub async fn connect(app: AppHandle) -> CmdResult<View> {
    connect_flow(&app).await
}

#[tauri::command]
pub async fn disconnect(app: AppHandle) -> CmdResult<View> {
    disconnect_flow(&app).await
}

#[tauri::command]
pub async fn toggle_connection(app: AppHandle) -> CmdResult<View> {
    toggle_flow(&app).await
}

#[tauri::command]
pub fn dismiss_error(app: AppHandle) -> View {
    publish(&app, |view| view.error = None)
}

#[tauri::command]
pub async fn add_subscription(app: AppHandle, url: String) -> CmdResult<View> {
    // Download outside the engine lock so the UI stays responsive.
    let fetched = fetch_subscription(&url).await.map_err(user_error)?;

    let state = app.state::<AppState>();
    let mut engine = state.engine.lock().await;
    let paths = engine.paths().clone();
    let profiles = engine.profiles_mut();
    let id = profiles.import_remote(&paths, fetched).map_err(user_error)?;
    // A newly added subscription is what the user wants to use now.
    profiles.set_active(&id).map_err(user_error)?;
    profiles.save().map_err(user_error)?;
    drop(engine);

    reconnect_if_connected(&app).await?;
    let engine = state.engine.lock().await;
    Ok(publish(&app, |view| sync_from_engine(view, &engine)))
}

#[tauri::command]
pub async fn refresh_subscription(app: AppHandle, id: String) -> CmdResult<View> {
    let id = ProfileId(id);
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
        publish(&app, |view| sync_from_engine(view, &engine));
    }

    if active {
        reconnect_if_connected(&app).await
    } else {
        Ok(state.view())
    }
}

#[tauri::command]
pub async fn select_profile(app: AppHandle, id: String) -> CmdResult<View> {
    {
        let state = app.state::<AppState>();
        let mut engine = state.engine.lock().await;
        let profiles = engine.profiles_mut();
        profiles.set_active(&ProfileId(id)).map_err(user_error)?;
        profiles.save().map_err(user_error)?;
        publish(&app, |view| sync_from_engine(view, &engine));
    }
    reconnect_if_connected(&app).await
}

#[tauri::command]
pub async fn remove_profile(app: AppHandle, id: String) -> CmdResult<View> {
    let state = app.state::<AppState>();
    let id = ProfileId(id);
    let was_active = {
        let engine = state.engine.lock().await;
        engine.profiles().active_profile().is_some_and(|p| p.id == id)
    };
    if was_active && state.view().phase == Phase::Connected {
        disconnect_flow(&app).await?;
    }

    let mut engine = state.engine.lock().await;
    let profiles_dir = engine.paths().profiles_dir.clone();
    let profiles = engine.profiles_mut();
    if profiles.remove(&id).is_some() {
        let _ = std::fs::remove_dir_all(profiles_dir.join(&id.0));
    }
    // Keep something active so the big button keeps working.
    if profiles.active_profile().is_none()
        && let Some(next) = profiles.profiles().first().map(|p| p.id.clone())
    {
        profiles.set_active(&next).map_err(user_error)?;
    }
    profiles.save().map_err(user_error)?;
    Ok(publish(&app, |view| sync_from_engine(view, &engine)))
}

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
    Ok(state.engine.lock().await.core_log_tail(200))
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
            return Err("覆盖配置的顶层必须是键值对".to_owned());
        }
    }
    let path = app
        .state::<AppState>()
        .engine
        .lock()
        .await
        .paths()
        .global_override_path();
    std::fs::write(path, text).map_err(|e| e.to_string())?;
    reconnect_if_connected(&app).await
}
