use anyhow::Result;
use rmcp::transport::io::stdio;
use rmcp::ServiceExt;
use tracing::info;

mod db;
mod tools;

use tools::EmailMcpServer;

#[tokio::main]
async fn main() -> Result<()> {
    let config = dataxlr8_mcp_core::Config::from_env("dataxlr8-email-mcp")
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    dataxlr8_mcp_core::logging::init(&config.log_level);

    info!(
        server = config.server_name,
        "Starting DataXLR8 Email MCP server"
    );

    let database = dataxlr8_mcp_core::Database::connect(&config.database_url)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    db::setup_schema(database.pool()).await?;

    let resend_key = std::env::var("RESEND_API_KEY").ok();
    if resend_key.is_none() {
        tracing::warn!("RESEND_API_KEY not set — emails will be logged but not sent");
    }

    let server = EmailMcpServer::new(database.clone(), resend_key);

    let transport = stdio();
    let service = server.serve(transport).await?;

    info!("Email MCP server connected via stdio");
    service.waiting().await?;

    database.close().await;
    info!("Email MCP server shut down");

    Ok(())
}
