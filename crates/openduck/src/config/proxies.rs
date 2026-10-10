use super::base::{Config, ConfigError};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_yaml::Mapping;
use tracing::warn;

pub const PROXIES_CONFIG_KEY: &str = "proxies";

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct ProxyEntry {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ProxyEntry {
    pub fn new(url: impl Into<String>, description: Option<String>) -> Self {
        Self {
            url: url.into().trim().to_string(),
            description: description.and_then(|d| {
                let trimmed = d.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            }),
        }
    }

    pub fn normalize(&mut self) {
        self.url = self.url.trim().to_string();
        if let Some(desc) = &self.description {
            let trimmed = desc.trim();
            if trimmed.is_empty() {
                self.description = None;
            } else if trimmed != desc {
                self.description = Some(trimmed.to_string());
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(untagged)]
enum ProxyEntryValue {
    Detailed(ProxyEntry),
    Simple(String),
}

fn parse_proxies_map(raw: Mapping) -> IndexMap<String, ProxyEntry> {
    let mut map = IndexMap::with_capacity(raw.len());
    for (k, v) in raw {
        let Some(key) = k.as_str() else {
            warn!(key = ?k, "Skipping non-string proxy key");
            continue;
        };
        let key = key.trim().to_string();
        if key.is_empty() {
            continue;
        }

        match serde_yaml::from_value::<ProxyEntryValue>(v.clone()) {
            Ok(ProxyEntryValue::Detailed(mut entry)) => {
                entry.normalize();
                map.insert(key, entry);
            }
            Ok(ProxyEntryValue::Simple(url)) => {
                let trimmed = url.trim().to_string();
                if !trimmed.is_empty() {
                    map.insert(key, ProxyEntry::new(trimmed, None));
                }
            }
            Err(err) => {
                warn!(
                    key = %key,
                    error = %err,
                    "Skipping malformed proxy entry"
                );
            }
        }
    }
    map
}

pub fn get_proxies_map(config: &Config) -> IndexMap<String, ProxyEntry> {
    let raw: Mapping = config
        .get_param(PROXIES_CONFIG_KEY)
        .unwrap_or_else(|_| Default::default());
    parse_proxies_map(raw)
}

pub fn get_proxy_entry(config: &Config, id: &str) -> Option<ProxyEntry> {
    get_proxies_map(config).get(id.trim()).cloned()
}

pub fn set_proxy_entry(config: &Config, id: &str, entry: &ProxyEntry) -> Result<(), ConfigError> {
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err(ConfigError::DeserializeError(
            "Proxy ID cannot be empty".to_string(),
        ));
    }
    let mut entry = entry.clone();
    entry.normalize();
    config.update_param::<Mapping, _, _>(PROXIES_CONFIG_KEY, |raw| {
        let mut map = parse_proxies_map(raw);
        map.insert(id, entry);
        map
    })
}

pub fn remove_proxy_entry(config: &Config, id: &str) -> Result<(), ConfigError> {
    let id = id.trim().to_string();
    config.update_param::<Mapping, _, _>(PROXIES_CONFIG_KEY, |raw| {
        let mut map = parse_proxies_map(raw);
        map.shift_remove(&id);
        map
    })
}

pub fn set_proxies_map(
    config: &Config,
    proxies: &IndexMap<String, ProxyEntry>,
) -> Result<(), ConfigError> {
    let mut clean_map = IndexMap::with_capacity(proxies.len());
    for (k, v) in proxies {
        let key = k.trim().to_string();
        if key.is_empty() {
            continue;
        }
        let mut entry = v.clone();
        entry.normalize();
        clean_map.insert(key, entry);
    }
    config.set_param(PROXIES_CONFIG_KEY, clean_map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn test_config() -> Config {
        let config_file = NamedTempFile::new().unwrap();
        let secrets_file = NamedTempFile::new().unwrap();
        Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap()
    }

    #[test]
    fn test_empty_proxies_map() {
        let config = test_config();
        assert!(get_proxies_map(&config).is_empty());
        assert_eq!(get_proxy_entry(&config, "clash"), None);
    }

    #[test]
    fn test_set_and_get_proxies() {
        let config = test_config();

        let entry = ProxyEntry::new("http://127.0.0.1:7890", Some("Clash Proxy".to_string()));
        set_proxy_entry(&config, "clash", &entry).unwrap();

        let retrieved = get_proxy_entry(&config, "clash").unwrap();
        assert_eq!(retrieved.url, "http://127.0.0.1:7890");
        assert_eq!(retrieved.description.as_deref(), Some("Clash Proxy"));

        let mut all = get_proxies_map(&config);
        assert_eq!(all.len(), 1);
        assert!(all.contains_key("clash"));

        // Add second proxy
        let socks_entry = ProxyEntry::new("socks5://127.0.0.1:1088", None);
        set_proxy_entry(&config, "socks", &socks_entry).unwrap();

        all = get_proxies_map(&config);
        assert_eq!(all.len(), 2);

        // Remove first proxy
        remove_proxy_entry(&config, "clash").unwrap();
        assert_eq!(get_proxy_entry(&config, "clash"), None);
        assert_eq!(get_proxies_map(&config).len(), 1);
    }

    #[test]
    fn test_set_proxies_map_replaces_all() {
        let config = test_config();
        let mut map = IndexMap::new();
        map.insert(
            "p1".to_string(),
            ProxyEntry::new("http://127.0.0.1:8080", None),
        );
        map.insert(
            "p2".to_string(),
            ProxyEntry::new("http://127.0.0.1:9090", Some("Secondary".to_string())),
        );

        set_proxies_map(&config, &map).unwrap();
        let loaded = get_proxies_map(&config);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.get("p1").unwrap().url, "http://127.0.0.1:8080");
        assert_eq!(loaded.get("p2").unwrap().url, "http://127.0.0.1:9090");
    }
}
