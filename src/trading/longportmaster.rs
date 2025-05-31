use std::sync::Arc;

use longport::{
    decimal,
    trade::{OrderSide, OrderType, SubmitOrderOptions, TimeInForceType},
    Config, QuoteContext, TradeContext,
};

pub struct LongPort {
    pub config: Arc<Config>,
    pub quote_ctx: QuoteContext,
    pub _receiver: tokio::sync::mpsc::UnboundedReceiver<longport::quote::PushEvent>,
}

impl LongPort {
    pub async fn new() -> longport::Result<Self> {
        // Load configuration from environment variables
        let config = Arc::new(Config::from_env()?);

        // Create a context for quote APIs
        let (quote_ctx, mut _receiver) = QuoteContext::try_new(config.clone()).await?;

        Ok(Self {
            config,
            quote_ctx,
            _receiver,
        })
    }

    pub async fn quote_pairs_once(&self, symbol: &str) -> longport::Result<()> {
        // Get basic information of securities
        let resp = self.quote_ctx.quote([symbol]).await?;
        println!("{:?}", resp);

        Ok(())
    }

    pub async fn quote_pairs_continue(&mut self, symbol: &str) -> longport::Result<()> {
        use longport::quote::SubFlags;
        // let config = Arc::new(Config::from_env()?);

        // Subscribe to real-time quote updates for the given symbol
        // let mut receiver = self
        //     .quote_ctx
        //     .subscribe([symbol], SubFlags::QUOTE, true)
        //     .await?;

        // 订阅行情（以腾讯控股 700.HK 为例）
        self.quote_ctx
            .subscribe([symbol], SubFlags::QUOTE, true)
            .await?;

        while let Some(event) = self._receiver.recv().await {
            println!("{:?}", event);
        }
        // Receive and print push events (once, for demonstration; remove break to receive continuously)
        // if let Some(event) = self._receiver.recv().await {
        //     println!("{:?}", event);
        // }

        Ok(())
    }
}
