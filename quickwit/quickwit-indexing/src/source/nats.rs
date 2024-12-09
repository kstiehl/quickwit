use anyhow::anyhow;
use async_trait::async_trait;
use futures::StreamExt;
use quickwit_actors::ActorExitStatus;
use quickwit_config::NatsSourceParams;
use quickwit_proto::metastore::SourceType;

pub struct NatsSource {
    consumer: async_nats::jetstream::consumer::PullConsumer,
}

impl NatsSource {
    async fn try_new(params: NatsSourceParams) -> anyhow::Result<Self> {
        let client = async_nats::connect("localhost").await?;
        let jetstream = async_nats::jetstream::new(client);

        let consumer = jetstream
            .get_consumer_from_stream(params.consumer, params.stream)
            .await?;

        Ok(Self { consumer })
    }
}

use crate::source::{BatchBuilder, Source, SourceContext, BATCH_NUM_BYTES_LIMIT};

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
                let message = match message {
                    Ok(m) => m,
                    Err(err) => return Err(ActorExitStatus::Failure(anyhow!(err).into())),
                };
                batch_builder.add_doc(message.payload.clone());
            }
            ctx.send_message(doc_processor_mailbox, batch_builder.build())
                .await?;
            ctx.record_progress();
        }
    }

    fn name(&self) -> String {
        todo!()
    }

    fn observable_state(&self) -> serde_json::Value {
        todo!()
    }
}
