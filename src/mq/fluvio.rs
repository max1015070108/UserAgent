// use async_std::stream::StreamExt;
use futures_util::StreamExt;

use dotenv::dotenv;
use fluvio::{
    metadata::topic::TopicSpec, Compression, ConsumerConfig, Fluvio, FluvioClusterConfig, Offset,
    RecordKey, TopicProducer, TopicProducerConfigBuilder,
};
use std::env;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use fluvio::spu::SpuPool;

pub struct FluvioManager<S>
where
    S: SpuPool + Send + Sync + 'static,
{
    fluvio: Arc<Fluvio>,
    producers: Mutex<HashMap<String, Arc<TopicProducer<S>>>>,
}

impl<S> FluvioManager<S>
where
    S: SpuPool + Send + Sync + 'static,
{
    // 1. Connect to Fluvio cluster
    pub async fn connect() -> anyhow::Result<Self> {
        // let fluvio = Fluvio::connect().await?;
        // Ok(Self {
        //     fluvio: Arc::new(fluvio),
        //     producers: Mutex::new(HashMap::new()),
        // })
        // pub async fn connect() -> anyhow::Result<Self> {
        dotenv().ok(); // 加载.env文件

        // 读取环境变量
        let endpoint =
            env::var("FLUVIO_CLUSTER_ENDPOINT").unwrap_or_else(|_| "localhost:9003".to_string());

        // 构造 FluvioClusterConfig
        let mut config = FluvioClusterConfig::new(endpoint);

        // 你可以根据需要设置更多 config 字段

        let fluvio = Fluvio::connect_with_config(&config).await?;
        Ok(Self {
            fluvio: Arc::new(fluvio),
            producers: Mutex::new(HashMap::new()),
        })
        // }
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
        // let config = fluvio::ConsumerConfig::builder().build()?;
        let config = fluvio::consumer::ConsumerConfigExtBuilder::default()
            .topic(topic.to_string())
            .partition(partition)
            .offset_start(offset)
            .build()?;
        let mut stream = self.fluvio.consumer_with_config(config).await?;

        let mut results = Vec::new();
        while let Some(Ok(record)) = stream.next().await {
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
