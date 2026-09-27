use openduck::providers::base::ProviderDescriptor;
use openduck::providers::grok_acp::{GrokAcpProvider, GROK_ACP_BINARY, GROK_ACP_PROVIDER_NAME};

#[test]
fn test_grok_acp_metadata() {
    let metadata = GrokAcpProvider::metadata();
    assert_eq!(metadata.name, GROK_ACP_PROVIDER_NAME);
    assert_eq!(metadata.display_name, "Grok CLI (ACP)");
    assert_eq!(metadata.default_model, "current");

    let setup = metadata.setup.expect("Grok ACP should have setup metadata");
    assert!(setup.acp);
    assert_eq!(setup.binary_name.as_deref(), Some(GROK_ACP_BINARY));
    assert_eq!(setup.aliases, &["grok-acp", "grok"]);
}
