use anyhow::{Context, Error, Result};
use chrono::{DateTime, Utc};
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

// 定义消息处理器 trait
pub trait MessageProcessor: Send + Sync {
    fn process(&self, message: KafkaMessage) -> Result<()>;
}

// 定义消息结构
#[derive(Debug, Serialize, Deserialize)]
pub struct KafkaMessage {
    pub event_type: String,
    pub event_args: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

// Kafka配置常量
pub const KAFKA_TOPIC: &str = "topai-marketing";

// Kafka 处理器
pub struct KafkaHandler {
    pub producer: FutureProducer,
    pub consumer: StreamConsumer,
}

impl KafkaHandler {
    pub fn new(brokers: &str, group_id: &str) -> Result<Self> {
        // 创建生产者
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .create()
            .context("Failed to create Kafka producer")?;

        // 创建消费者
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", group_id)
            .set("enable.auto.commit", "true")
            .set("auto.offset.reset", "earliest")
            .create()
            .context("Failed to create Kafka consumer")?;

        Ok(Self { producer, consumer })
    }

    // 发送消息
    pub async fn send_message(
        &self,
        event_type: &str,
        event_args: HashMap<String, serde_json::Value>,
    ) -> Result<(), Error> {
        let message = KafkaMessage {
            event_type: event_type.to_string(),
            event_args,
            created_at: Utc::now(),
        };

        let payload = serde_json::to_string(&message).context("Failed to serialize message")?;

        match self
            .producer
            .send(
                FutureRecord::to(KAFKA_TOPIC)
                    .payload(&payload)
                    .key(event_type),
                Duration::from_secs(5),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err((err, _)) => {
                println!("Error sending message: {}", err);
                Err(anyhow::anyhow!("Failed to send message: {}", err))
            }
        }

        // Ok(())
    }

    // 订阅主题
    pub async fn subscribe(&self) -> Result<()> {
        self.consumer
            .subscribe(&[KAFKA_TOPIC])
            .context("Failed to subscribe to topic")?;
        Ok(())
    }

    // 开始消费消息
    pub async fn start_consuming<P: MessageProcessor + 'static>(&self, processor: P) -> Result<()> {
        loop {
            match self.consumer.recv().await {
                Ok(msg) => {
                    if let Some(payload) = msg.payload() {
                        match serde_json::from_slice::<KafkaMessage>(payload) {
                            Ok(kafka_message) => {
                                if let Err(e) = processor.process(kafka_message) {
                                    eprintln!("Error processing message: {}", e);
                                }
                            }
                            Err(e) => {
                                eprintln!("Error deserializing message: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error receiving message: {}", e);
                }
            }
        }
    }
}

// 实现一个基本的消息处理器
pub struct DefaultMessageProcessor;

impl MessageProcessor for DefaultMessageProcessor {
    fn process(&self, message: KafkaMessage) -> Result<()> {
        println!("Processing message: {:?}", message);
        Ok(())
    }
}

// 使用示例
#[tokio::main]
async fn main() -> Result<()> {
    // 创建 Kafka 处理器
    let kafka_handler = KafkaHandler::new("localhost:9092", "my-consumer-group")?;

    // 示例：发送消息
    let mut event_args = HashMap::new();
    event_args.insert(
        "user_id".to_string(),
        serde_json::Value::String("123".to_string()),
    );
    event_args.insert(
        "credits".to_string(),
        serde_json::Value::Number(serde_json::Number::from(100)),
    );

    kafka_handler.send_message("credits", event_args).await?;

    // 示例：消费消息
    kafka_handler.subscribe().await?;

    // 创建消息处理器
    let processor = DefaultMessageProcessor;

    // 开始消费消息
    kafka_handler.start_consuming(processor).await?;

    Ok(())
}

// 为闭包实现 MessageProcessor trait
pub struct ClosureMessageProcessor<F>(pub F);

impl<F> MessageProcessor for ClosureMessageProcessor<F>
where
    F: Fn(KafkaMessage) -> Result<()> + Send + Sync,
{
    fn process(&self, message: KafkaMessage) -> Result<()> {
        (self.0)(message)
    }
}

// 提供一个辅助函数来创建闭包处理器
pub fn create_processor<F>(f: F) -> ClosureMessageProcessor<F>
where
    F: Fn(KafkaMessage) -> Result<()> + Send + Sync,
{
    ClosureMessageProcessor(f)
}

// 测试模块
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_kafka_message_serialization() {
        let mut event_args = HashMap::new();
        event_args.insert(
            "user_id".to_string(),
            serde_json::Value::String("123".to_string()),
        );

        let message = KafkaMessage {
            event_type: "users".to_string(),
            event_args,
            created_at: Utc::now(),
        };

        let serialized = serde_json::to_string(&message).unwrap();
        let deserialized: KafkaMessage = serde_json::from_str(&serialized).unwrap();

        assert_eq!(message.event_type, deserialized.event_type);
    }
}
