//! Layered YAML merge for Clash configs.

use serde_yaml::{Mapping, Value};

#[derive(Debug, Clone)]
pub struct RuntimeConfigBuilder {
    layers: Vec<Value>,
}

impl RuntimeConfigBuilder {
    pub fn new(base: Value) -> Self {
        Self { layers: vec![base] }
    }

    pub fn push_override(mut self, value: Value) -> Self {
        self.layers.push(value);
        self
    }

    pub fn build(self) -> Value {
        let mut merged = Value::Mapping(Mapping::new());
        for layer in self.layers {
            apply_overlay(&mut merged, layer);
        }
        merged
    }
}

/// Top-level list keys that overlays can extend instead of replace.
const SPLICE_KEYS: [&str; 3] = ["rules", "proxies", "proxy-groups"];

/// Apply one override layer on top of `base`.
///
/// Plain keys deep-merge (maps recurse, everything else — lists included —
/// is replaced). On top of that, `prepend-<key>` / `append-<key>` for
/// `rules`, `proxies` and `proxy-groups` splice into the base list, so an
/// override can add a rule without wiping the subscription's rules. Named
/// entries (proxies, groups) from the overlay replace same-named base entries.
pub fn apply_overlay(base: &mut Value, overlay: Value) {
    let Value::Mapping(mut overlay_map) = overlay else {
        *base = overlay;
        return;
    };

    let mut splices = Vec::new();
    for key in SPLICE_KEYS {
        for (prefix, prepend) in [("prepend-", true), ("append-", false)] {
            if let Some(Value::Sequence(items)) = overlay_map.remove(format!("{prefix}{key}")) {
                splices.push((key, prepend, items));
            }
        }
    }

    merge_yaml(base, Value::Mapping(overlay_map));
    for (key, prepend, items) in splices {
        splice_list(base, key, prepend, items);
    }
}

fn splice_list(base: &mut Value, key: &str, prepend: bool, items: Vec<Value>) {
    if !base.is_mapping() {
        *base = Value::Mapping(Mapping::new());
    }
    let Value::Mapping(map) = base else {
        unreachable!("base was just made a mapping")
    };

    let entry = map
        .entry(Value::String(key.to_owned()))
        .or_insert_with(|| Value::Sequence(Vec::new()));
    if !entry.is_sequence() {
        *entry = Value::Sequence(Vec::new());
    }
    let Value::Sequence(list) = entry else {
        unreachable!("entry was just made a sequence")
    };

    if key != "rules" {
        let names: Vec<&str> = items.iter().filter_map(entry_name).collect();
        list.retain(|existing| entry_name(existing).is_none_or(|name| !names.contains(&name)));
    }

    let at = if prepend {
        0
    } else if key == "rules" {
        // Appending after the catch-all MATCH would never match; keep it last.
        list.iter()
            .rposition(|rule| rule.as_str().is_some_and(|r| r.trim_start().starts_with("MATCH")))
            .unwrap_or(list.len())
    } else {
        list.len()
    };
    list.splice(at..at, items);
}

fn entry_name(value: &Value) -> Option<&str> {
    value.get("name").and_then(Value::as_str)
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

#[cfg(test)]
mod tests {
    use serde_yaml::Value;

    use super::RuntimeConfigBuilder;

    #[test]
    fn later_layers_override_earlier_layers() {
        let base: Value = serde_yaml::from_str("mixed-port: 7890\nmode: rule\ntun: {enable: false, mtu: 1400}\n").unwrap();
        let global: Value = serde_yaml::from_str("mode: global\nallow-lan: false\ntun: {enable: true}\n").unwrap();

        let merged = RuntimeConfigBuilder::new(base).push_override(global).build();

        assert_eq!(merged["mixed-port"], Value::from(7890));
        assert_eq!(merged["mode"], Value::from("global"));
        assert_eq!(merged["allow-lan"], Value::from(false));
        assert_eq!(merged["tun"]["enable"], Value::from(true));
        assert_eq!(merged["tun"]["mtu"], Value::from(1400));
    }

    #[test]
    fn prepend_and_append_rules_keep_subscription_rules() {
        let base: Value = serde_yaml::from_str(
            "rules:\n  - DOMAIN,a.com,PROXY\n  - MATCH,PROXY\n",
        )
        .unwrap();
        let overlay: Value = serde_yaml::from_str(
            "prepend-rules:\n  - DOMAIN,first.com,DIRECT\nappend-rules:\n  - DOMAIN,last.com,DIRECT\n",
        )
        .unwrap();

        let merged = RuntimeConfigBuilder::new(base).push_override(overlay).build();
        let rules: Vec<&str> = merged["rules"]
            .as_sequence()
            .unwrap()
            .iter()
            .map(|rule| rule.as_str().unwrap())
            .collect();

        assert_eq!(
            rules,
            [
                "DOMAIN,first.com,DIRECT",
                "DOMAIN,a.com,PROXY",
                "DOMAIN,last.com,DIRECT",
                "MATCH,PROXY",
            ]
        );
        assert!(merged.get("prepend-rules").is_none());
    }

    #[test]
    fn spliced_named_entries_replace_same_named_base_entries() {
        let base: Value = serde_yaml::from_str(
            "proxy-groups:\n  - {name: Auto, type: url-test}\n  - {name: PROXY, type: select}\n",
        )
        .unwrap();
        let overlay: Value =
            serde_yaml::from_str("prepend-proxy-groups:\n  - {name: Auto, type: smart}\n").unwrap();

        let merged = RuntimeConfigBuilder::new(base).push_override(overlay).build();
        let groups = merged["proxy-groups"].as_sequence().unwrap();

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0]["name"], Value::from("Auto"));
        assert_eq!(groups[0]["type"], Value::from("smart"));
        assert_eq!(groups[1]["name"], Value::from("PROXY"));
    }
}
