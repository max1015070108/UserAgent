use std::sync::Arc;
use std::time::Duration;

use async_std::stream::StreamExt;
use fluvio::{
    metadata::topic::TopicSpec, Compression, Fluvio, Offset, RecordKey, TopicProducerConfigBuilder,
};

pub struct FluvioManager {
    fluvio: Arc<Fluvio>,
}

impl FluvioManager {
    // 1. Connect to Fluvio cluster
    pub async fn connect() -> anyhow::Result<Self> {
        let fluvio = Fluvio::connect().await?;
        Ok(Self {
            fluvio: Arc::new(fluvio),
        })
    }

    // 2. Produce data with TopicProducerConfig
    pub async fn produce(&self, topic: &str, key: Option<&str>, data: &str) -> anyhow::Result<()> {
        let producer_config = TopicProducerConfigBuilder::default()
            .batch_size(500)
            .linger(Duration::from_millis(500))
            .compression(Compression::Gzip)
            .build()
            .expect("Failed to create topic producer config");

        let producer = self
            .fluvio
            .topic_producer_with_config(topic, producer_config)
            .await?;

        let record_key = key.unwrap_or("");

        producer.send(record_key, data).await?;
        producer.flush().await?;
        Ok(())
    }

    // 3. Consume data
    pub async fn consume(
        &self,
        topic: &str,
        partition: u32,
        offset: Offset,
        max_records: usize,
    ) -> anyhow::Result<Vec<(Option<RecordKey>, String)>> {
        let consumer = self.fluvio.partition_consumer(topic, partition).await?;
        let mut stream = consumer.stream(offset).await?;
        let mut results = Vec::new();
        while let Some(Ok(record)) = StreamExt::next(&mut stream).await {
            let value = String::from_utf8_lossy(record.value()).into_owned();
            let key = record.key().map(|k| k.to_vec());
            results.push((key, value));
            if results.len() >= max_records {
                break;
            }
        }
        // Convert Option<Vec<u8>> to Option<RecordKey>
        let converted_results = results
            .into_iter()
            .map(|(key_opt, value)| {
                let key = key_opt.map(RecordKey::from);
                (key, value)
            })
            .collect();
        Ok(converted_results)
    }

    // 4. Query topics (list)
    pub async fn list_topics(&self) -> anyhow::Result<Vec<String>> {
        let admin = self.fluvio.admin().await;
        let topics = admin.list::<TopicSpec, _>(Vec::<String>::new()).await?;
        let names = topics.iter().map(|t| t.name.clone()).collect();
        Ok(names)
    }
}
