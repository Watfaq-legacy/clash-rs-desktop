//! Turns the controller's `/proxies` map into the one node list the simple
//! UI shows: the first selector group the user would normally pick from.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

const GROUP_TYPES: [&str; 6] = [
    "Selector",
    "URLTest",
    "Fallback",
    "LoadBalance",
    "Relay",
    "Smart",
];

/// Groups that pick a node by themselves; shown as "自动选择".
const AUTO_TYPES: [&str; 4] = ["URLTest", "Fallback", "LoadBalance", "Smart"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeItem {
    pub name: String,
    pub kind: String,
    pub delay: Option<u64>,
    /// True for automatic groups (url-test, smart, ...).
    pub auto: bool,
    /// For automatic groups: the node they currently use.
    pub resolved: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodesView {
    pub group: Option<String>,
    pub selected: Option<String>,
    /// The concrete node traffic ends up on, following nested groups.
    pub resolved: Option<String>,
    pub resolved_delay: Option<u64>,
    pub items: Vec<NodeItem>,
}

pub fn build(proxies: &BTreeMap<String, Value>) -> NodesView {
    let Some(group) = main_group(proxies) else {
        return NodesView::default();
    };
    let entry = &proxies[&group];
    let selected = entry.get("now").and_then(Value::as_str).map(str::to_owned);
    let resolved = selected.as_deref().map(|name| resolve(proxies, name));

    let items = entry
        .get("all")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|name| {
            let kind = kind_of(proxies, name).unwrap_or_default();
            let auto = AUTO_TYPES.contains(&kind.as_str());
            let resolved = auto.then(|| resolve(proxies, name));
            NodeItem {
                delay: delay_of(proxies, resolved.as_deref().unwrap_or(name)),
                name: name.to_owned(),
                kind,
                auto,
                resolved,
            }
        })
        .collect();

    NodesView {
        resolved_delay: resolved.as_deref().and_then(|name| delay_of(proxies, name)),
        group: Some(group),
        selected,
        resolved,
        items,
    }
}

/// First `Selector` in config order. `GLOBAL.all` lists groups in the order
/// the config declares them; fall back to map order if it is missing.
fn main_group(proxies: &BTreeMap<String, Value>) -> Option<String> {
    let ordered: Vec<&str> = proxies
        .get("GLOBAL")
        .and_then(|global| global.get("all"))
        .and_then(Value::as_array)
        .map(|all| all.iter().filter_map(Value::as_str).collect())
        .unwrap_or_else(|| proxies.keys().map(String::as_str).collect());

    ordered
        .into_iter()
        .filter(|name| *name != "GLOBAL")
        .find(|name| kind_of(proxies, name).as_deref() == Some("Selector"))
        .map(str::to_owned)
}

fn kind_of(proxies: &BTreeMap<String, Value>, name: &str) -> Option<String> {
    proxies.get(name)?.get("type")?.as_str().map(str::to_owned)
}

/// Follow `now` through nested groups down to a real node.
fn resolve(proxies: &BTreeMap<String, Value>, name: &str) -> String {
    let mut current = name.to_owned();
    for _ in 0..8 {
        let is_group =
            kind_of(proxies, &current).is_some_and(|k| GROUP_TYPES.contains(&k.as_str()));
        let next = proxies
            .get(&current)
            .and_then(|entry| entry.get("now"))
            .and_then(Value::as_str);
        match (is_group, next) {
            (true, Some(next)) if next != current => current = next.to_owned(),
            _ => break,
        }
    }
    current
}

/// Latest recorded delay; 0 means the last test failed.
fn delay_of(proxies: &BTreeMap<String, Value>, name: &str) -> Option<u64> {
    proxies
        .get(name)?
        .get("history")?
        .as_array()?
        .last()?
        .get("delay")?
        .as_u64()
        .filter(|delay| *delay > 0)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::build;

    #[test]
    fn picks_first_selector_and_resolves_auto_group() {
        let proxies = serde_json::from_value(json!({
            "GLOBAL": {"type": "Selector", "now": "Proxy", "all": ["Proxy", "Auto", "HK"]},
            "Proxy": {"type": "Selector", "now": "Auto", "all": ["Auto", "HK", "JP"]},
            "Auto": {"type": "URLTest", "now": "JP", "all": ["HK", "JP"]},
            "HK": {"type": "Shadowsocks", "history": [{"delay": 0}]},
            "JP": {"type": "Trojan", "history": [{"delay": 80}, {"delay": 42}]},
        }))
        .unwrap();

        let view = build(&proxies);
        assert_eq!(view.group.as_deref(), Some("Proxy"));
        assert_eq!(view.selected.as_deref(), Some("Auto"));
        assert_eq!(view.resolved.as_deref(), Some("JP"));
        assert_eq!(view.resolved_delay, Some(42));

        assert!(view.items[0].auto);
        assert_eq!(view.items[0].resolved.as_deref(), Some("JP"));
        assert_eq!(view.items[1].delay, None, "failed test reads as no delay");
        assert_eq!(view.items[2].delay, Some(42));
    }
}
