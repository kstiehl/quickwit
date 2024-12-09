use anyhow::anyhow;
use async_trait::async_trait;
use futures::StreamExt;
use quickwit_actors::ActorExitStatus;
use quickwit_config::NatsSourceParams;
use quickwit_proto::metastore::SourceType;

use serde_json::json;
use tracing::{info};
pub struct NatsSourceFactory;

#[async_trait]
impl TypedSourceFactory for NatsSourceFactory {
    type Source = NatsSource;

    type Params = NatsSourceParams;

    async fn typed_create_source(
        source_runtime: super::SourceRuntime,
        source_params: Self::Params,
    ) -> anyhow::Result<Self::Source> {
        NatsSource::try_new(source_runtime, source_params).await
    }
}

pub struct NatsSource {
    source_runtime: super::SourceRuntime,
    source_params: NatsSourceParams,
    consumer: async_nats::jetstream::consumer::PullConsumer,
}

impl NatsSource {
    async fn try_new(
        source_runtime: super::SourceRuntime,
        source_params: NatsSourceParams,
    ) -> anyhow::Result<Self> {
        info!("setting up nats source");
        let client = async_nats::connect(&source_params.endpoint).await?;
        let jetstream = async_nats::jetstream::new(client);

        let consumer = jetstream
            .get_consumer_from_stream(&source_params.consumer, &source_params.stream)
            .await?;

        Ok(Self {
            source_runtime,
            source_params,
            consumer,
        })
    }
}
impl std::fmt::Debug for NatsSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter
            .debug_struct("NatsSource")
            .field("index_uid", &self.source_runtime.index_uid())
            .field("source_id", &self.source_runtime.source_id())
            .field("stream", &self.source_params.stream)
            .field("consumer", &self.source_params.consumer)
            .finish()
    }
}

use crate::source::{BatchBuilder, Source, SourceContext, BATCH_NUM_BYTES_LIMIT};

use super::TypedSourceFactory;

#[async_trait]
impl Source for NatsSource {
    async fn emit_batches(
        &mut self,
        doc_processor_mailbox: &quickwit_actors::Mailbox<crate::actors::DocProcessor>,
        ctx: &SourceContext,
    ) -> Result<std::time::Duration, quickwit_actors::ActorExitStatus> {
        loop {
            let mut batch_builder = BatchBuilder::new(SourceType::Nats);
            let mut messages = self
                .consumer
                .fetch()
                .max_bytes(BATCH_NUM_BYTES_LIMIT.try_into().unwrap())
                .messages()
                .await
                .unwrap();
            while let Some(message) = messages.next().await {
                info!("processing");
                let message = match message {
                    Ok(m) => m,
                    Err(err) => return Err(ActorExitStatus::Failure(anyhow!(err).into())),
                };
                batch_builder.add_doc(message.payload.clone());
                message.ack().await.unwrap();
            }
            ctx.send_message(doc_processor_mailbox, batch_builder.build())
                .await?;
            ctx.record_progress();
        }
    }

    fn name(&self) -> String {
        format!("{:?}", self)
    }

    fn observable_state(&self) -> serde_json::Value {
        json!({})
    }
}
