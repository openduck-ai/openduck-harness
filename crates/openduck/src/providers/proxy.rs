pub fn resolve_outbound_proxy(
    config: &crate::config::Config,
    provider_name: &str,
) -> anyhow::Result<Option<String>> {
    if let Some(proxy) = resolve_global_proxy(config)? {
        return Ok(Some(proxy));
    }
    Ok(crate::config::providers::get_provider_proxy(
        config,
        provider_name,
    ))
}

pub fn resolve_global_proxy(config: &crate::config::Config) -> anyhow::Result<Option<String>> {
    Ok(config
        .get_proxy()?
        .map(|proxy| proxy.url_with_credentials().to_owned()))
}

pub fn apply_proxy(
    builder: reqwest::ClientBuilder,
    proxy: Option<&str>,
) -> anyhow::Result<reqwest::ClientBuilder> {
    let Some(proxy) = proxy.map(str::trim).filter(|proxy| !proxy.is_empty()) else {
        return Ok(builder);
    };
    let proxy = match reqwest::Proxy::all(proxy) {
        Ok(proxy) => proxy,
        Err(_) => return Err(anyhow::anyhow!("Invalid proxy configuration")),
    };
    Ok(builder.proxy(proxy))
}

#[cfg(test)]
mod tests {
    use super::{apply_proxy, resolve_global_proxy};
    use crate::config::Config;
    use std::fs;
    use tempfile::NamedTempFile;

    fn config_with(yaml: &str) -> (Config, NamedTempFile, NamedTempFile) {
        let config_file = NamedTempFile::new().unwrap();
        let secrets_file = NamedTempFile::new().unwrap();
        fs::write(config_file.path(), yaml).unwrap();
        let config =
            Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap();
        (config, config_file, secrets_file)
    }

    #[test]
    fn apply_proxy_absent_leaves_builder_usable() {
        apply_proxy(reqwest::Client::builder(), None)
            .unwrap()
            .build()
            .unwrap();
        apply_proxy(reqwest::Client::builder(), Some(" \t\n "))
            .unwrap()
            .build()
            .unwrap();
    }

    #[test]
    fn apply_proxy_rejects_invalid_url_without_leaking_credentials() {
        let err = apply_proxy(
            reqwest::Client::builder(),
            Some("http://user:secret@bad host"),
        )
        .unwrap_err();
        let rendered = format!("{err:#}");
        let debug = format!("{err:?}");
        assert!(rendered.contains("Invalid proxy configuration"));
        assert!(!rendered.contains("secret"), "{rendered}");
        assert!(!debug.contains("secret"), "{debug}");
        assert!(!rendered.contains("bad host"), "{rendered}");
        assert!(!debug.contains("bad host"), "{debug}");
    }

    #[test]
    fn resolve_global_proxy_reads_config_key() {
        let (config, _config_file, _secrets_file) = config_with("proxy: http://127.0.0.1:3128\n");
        assert_eq!(
            resolve_global_proxy(&config).unwrap().as_deref(),
            Some("http://127.0.0.1:3128/")
        );
    }

    #[test]
    fn resolve_global_proxy_absent_is_none() {
        let (config, _config_file, _secrets_file) = config_with("");
        assert!(resolve_global_proxy(&config).unwrap().is_none());

        let (config, _config_file, _secrets_file) = config_with("other_key: value\n");
        assert!(resolve_global_proxy(&config).unwrap().is_none());
    }
}
