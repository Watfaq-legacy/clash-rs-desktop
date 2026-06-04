use crate::controller_endpoint::controller_client_config;

const DEFAULT_DELAY_TEST_URL: &str = "https://www.gstatic.com/generate_204";


pub(crate) fn reactor_close_all_connections_text() -> String {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => return format!("close all connections failed: {error}"),
    };

    runtime.block_on(async {
        match controller_client().close_all_connections().await {
            Ok(value) => format!("close all connections: ok\n{}", pretty_json(&value)),
            Err(error) => format!("close all connections failed: {error}"),
        }
    })
}

pub(crate) fn reactor_close_connection_text(id: &str) -> String {
    let id = id.trim();
    if id.is_empty() {
        return "close connection: connection id is required".to_owned();
    }

    run_controller_action("close connection", async move {
        controller_client().close_connection(id).await
    })
}

pub(crate) fn reactor_patch_configs_text(
    mode: &str,
    log_level: &str,
    mixed_port: &str,
    bind_address: &str,
    allow_lan: bool,
    ipv6: bool,
) -> String {
    let mixed_port = match optional_port(mixed_port) {
        Ok(port) => port,
        Err(error) => return format!("patch configs failed: {error}"),
    };

    let patch = clash_rs_desktop_core::ControllerConfigPatch {
        mode: optional_string(mode),
        log_level: optional_string(log_level),
        mixed_port,
        bind_address: optional_string(bind_address),
        allow_lan: Some(allow_lan),
        ipv6: Some(ipv6),
        ..Default::default()
    };

    run_controller_action("patch configs", async move {
        controller_client().patch_configs(&patch).await
    })
}

pub(crate) fn reactor_reload_configs_text(path: &str) -> String {
    let request = clash_rs_desktop_core::ConfigReloadRequest {
        path: optional_string(path),
        payload: String::new(),
    };

    run_controller_action("reload configs", async move {
        controller_client().reload_configs(&request).await
    })
}

pub(crate) fn reactor_select_proxy_text(group: &str, proxy: &str) -> String {
    let group = group.trim().to_owned();
    let proxy = proxy.trim().to_owned();
    if group.is_empty() || proxy.is_empty() {
        return "select proxy: group and proxy are required".to_owned();
    }

    run_controller_action("select proxy", async move {
        controller_client().select_proxy(&group, &proxy).await
    })
}

pub(crate) fn reactor_group_delay_text(group: &str) -> String {
    let group = group.trim().to_owned();
    if group.is_empty() {
        return "group delay: group is required".to_owned();
    }

    run_controller_action("group delay", async move {
        controller_client()
            .test_group_delay(&group, DEFAULT_DELAY_TEST_URL, 5000)
            .await
    })
}

pub(crate) fn reactor_update_proxy_provider_text(provider: &str) -> String {
    let provider = provider.trim().to_owned();
    if provider.is_empty() {
        return "update proxy provider: provider name is required".to_owned();
    }

    run_controller_action("update proxy provider", async move {
        controller_client().update_proxy_provider(&provider).await
    })
}

pub(crate) fn reactor_healthcheck_proxy_provider_text(provider: &str) -> String {
    let provider = provider.trim().to_owned();
    if provider.is_empty() {
        return "proxy provider healthcheck: provider name is required".to_owned();
    }

    run_controller_action("proxy provider healthcheck", async move {
        controller_client()
            .healthcheck_proxy_provider(&provider)
            .await
    })
}

pub(crate) fn reactor_healthcheck_provider_proxy_text(provider: &str, proxy: &str) -> String {
    let provider = provider.trim().to_owned();
    let proxy = proxy.trim().to_owned();
    if provider.is_empty() || proxy.is_empty() {
        return "provider proxy healthcheck: provider and proxy are required".to_owned();
    }

    run_controller_action("provider proxy healthcheck", async move {
        controller_client()
            .healthcheck_provider_proxy(&provider, &proxy)
            .await
    })
}

pub(crate) fn reactor_update_rule_provider_text(provider: &str) -> String {
    let provider = provider.trim().to_owned();
    if provider.is_empty() {
        return "update rule provider: provider name is required".to_owned();
    }

    run_controller_action("update rule provider", async move {
        controller_client().update_rule_provider(&provider).await
    })
}

pub(crate) fn reactor_stream_messages(
    path: &str,
    max_messages: usize,
    timeout_secs: u64,
) -> Result<Vec<String>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;

    runtime
        .block_on(async {
            controller_client()
                .sample_websocket_text(
                    path,
                    max_messages,
                    std::time::Duration::from_secs(timeout_secs),
                )
                .await
        })
        .map_err(|error| error.to_string())
}


fn run_controller_action<F>(label: &str, future: F) -> String
where
    F: std::future::Future<Output = anyhow::Result<serde_json::Value>>,
{
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => return format!("{label} failed: {error}"),
    };

    match runtime.block_on(future) {
        Ok(value) => format!("{label}: ok\n{}", pretty_json(&value)),
        Err(error) => format!("{label} failed: {error}"),
    }
}

fn optional_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn optional_port(value: &str) -> anyhow::Result<Option<u16>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    Ok(Some(value.parse()?))
}

fn controller_client() -> clash_rs_desktop_core::ClashIpcClient {
    clash_rs_desktop_core::ClashIpcClient::new(controller_client_config())
}

fn pretty_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

