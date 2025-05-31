use rig::{
    completion::{Prompt, ToolDefinition},
    providers,
    tool::Tool,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing_subscriber;
pub async fn deepseekai(msg: &String) -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();
    tracing::info!("msg value: {}", msg);

    let client = providers::deepseek::Client::from_env();
    let agent = client
        .agent("deepseek-chat")
        .preamble("You are a helpful assistant.")
        .build();

    let answer = agent.prompt(msg).await?;
    println!("Answer: {}", answer);

    Ok(())
}
