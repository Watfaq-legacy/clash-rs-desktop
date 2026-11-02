//! Shared app state and the UI-facing snapshot (`View`).
//!
//! The engine sits behind an async mutex and can be held for a long time
//! (first-run core download, subscription fetch). The UI never waits on it:
//! it reads `View`, a cheap snapshot republished after every change.

use std::sync::Mutex;

use clash_rs_desktop_core::{ConnectMode, Engine, RoutingMode, SubscriptionInfo, UserProfile};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;

pub struct AppState {
    pub engine: tokio::sync::Mutex<Engine>,
    view: Mutex<View>,
    /// Stops the live traffic stream when flipped to `true`.
    pub traffic_stop: Mutex<Option<watch::Sender<bool>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    pub remote: bool,
    pub active: bool,
    pub subscription: Option<SubscriptionInfo>,
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub phase: Phase,
    /// Progress text while connecting ("正在下载内核…").
    pub step: Option<String>,
    /// Last user-facing error; cleared by the next action.
    pub error: Option<String>,
    /// Unix millis when the current connection came up.
    pub connected_since: Option<i64>,
    pub port: Option<u16>,
    pub profiles: Vec<ProfileView>,
    pub routing_mode: RoutingMode,
    pub connect_mode: ConnectMode,
    pub auto_connect: bool,
}

impl AppState {
    pub fn new(engine: Engine) -> Self {
        let view = View {
            phase: Phase::Disconnected,
            step: None,
            error: None,
            connected_since: None,
            port: None,
            profiles: Vec::new(),
            routing_mode: RoutingMode::default(),
            connect_mode: ConnectMode::default(),
            auto_connect: false,
        };
        let state = Self {
            engine: tokio::sync::Mutex::new(engine),
            view: Mutex::new(view),
            traffic_stop: Mutex::new(None),
        };
        if let Ok(engine) = state.engine.try_lock() {
            state.update(|view| sync_from_engine(view, &engine));
        }
        state
    }

    pub fn view(&self) -> View {
        self.view.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn update(&self, change: impl FnOnce(&mut View)) -> View {
        let mut view = self.view.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut view);
        view.clone()
    }
}

/// Copy persisted engine data (profiles, settings) into the view.
pub fn sync_from_engine(view: &mut View, engine: &Engine) {
    let settings = engine.settings().settings();
    view.routing_mode = settings.routing_mode;
    view.connect_mode = settings.connect_mode;
    view.auto_connect = settings.auto_connect;
    view.port = engine.mixed_port();
    view.profiles = engine
        .profiles()
        .profiles()
        .iter()
        .map(profile_view)
        .collect();
}

fn profile_view(profile: &UserProfile) -> ProfileView {
    ProfileView {
        id: profile.id.0.clone(),
        name: profile.name.clone(),
        remote: matches!(
            profile.source,
            clash_rs_desktop_core::ProfileSource::RemoteUrl { .. }
        ),
        active: profile.active,
        subscription: profile.subscription.clone(),
        updated_at: profile.updated_at,
    }
}

/// Apply a change, then push the new view to the window and tray.
pub fn publish(app: &AppHandle, change: impl FnOnce(&mut View)) -> View {
    let view = app.state::<AppState>().update(change);
    let _ = app.emit("view", &view);
    crate::tray::refresh(app, &view);
    view
}
