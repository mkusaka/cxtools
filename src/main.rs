use cxtools::CxTools;
use rmcp::ServiceExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = CxTools::new().serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
