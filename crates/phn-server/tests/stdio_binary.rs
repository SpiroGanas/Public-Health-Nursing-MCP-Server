//! End-to-end test of the real binary over stdio, the way Claude Desktop
//! launches it.

mod common;

use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};

#[tokio::test]
async fn binary_serves_mcp_over_stdio() {
    let dir = tempfile::tempdir().unwrap();
    common::copy_dir(&common::corpus().join("data"), &dir.path().join("data"));
    let cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_phn-mcp-server")).configure(|c| {
        c.env("PHN_DATA_DIR", dir.path().join("data"))
            .env("PHN_CACHE_DIR", dir.path().join("cache"))
            .env("PHN_OFFLINE", "1")
            .env("PHN_PROFILE", "cd_nurse")
            .env("PHN_LOG", "warn");
    });
    let client = ().serve(TokioChildProcess::new(cmd).unwrap()).await.unwrap();
    let info = client.peer_info().expect("server info");
    assert_eq!(
        info.server_info.as_ref().unwrap().name,
        phn_core::SERVER_NAME
    );
    let tools = client.list_all_tools().await.unwrap();
    // cd_nurse enables core, cd, vax, epi, comms, equity.
    assert!(
        tools
            .iter()
            .any(|t| t.name == "lookup_reportable_condition")
    );
    assert!(
        tools
            .iter()
            .any(|t| t.name == "check_document_accessibility")
    );
    let r = client
        .call_tool(
            CallToolRequestParams::new("lookup_reportable_condition").with_arguments(
                serde_json::json!({"query": "Neisseria meningitidis"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(r.is_error, Some(false));
    assert!(common::summary(&r).contains("Meningococcal"));
    client.cancel().await.unwrap();
}
