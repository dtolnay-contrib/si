use std::time::Duration;

use futures::StreamExt;
use si_data_nats::{NatsClient, Subject, Subscriber};
use telemetry::{prelude::*, tracing::field};
use telemetry_nats::propagation;

use crate::{Graph, Id, Request, Response, SubjectGenerator};

pub mod management;

pub type ClientResult<T, E = ClientError> = Result<T, E>;

#[remain::sorted]
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error(transparent)]
    Nats(#[from] si_data_nats::Error),
    #[error("no listener available for message that was just sent")]
    NoListenerAvailable,
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}

#[remain::sorted]
#[derive(Debug)]
pub enum State {
    Continue,
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct PubClient {
    change_set_id: Id,
    pub_channel: Subject,
    reply_channel: Subject,
    nats: NatsClient,
}

impl PubClient {
    pub async fn register_dependency_graph(&self, dependency_graph: Graph) -> ClientResult<()> {
        let message = serde_json::to_vec(&Request::ValueDependencyGraph {
            change_set_id: self.change_set_id,
            dependency_graph,
        })?;
        self.nats
            .publish_with_reply_and_headers(
                self.pub_channel.clone(),
                self.reply_channel.clone(),
                propagation::empty_injected_headers(),
                message.into(),
            )
            .await?;
        Ok(())
    }

    pub async fn processed_value(&self, node_id: Id) -> ClientResult<()> {
        let message = serde_json::to_vec(&Request::ProcessedValue {
            change_set_id: self.change_set_id,
            node_id,
        })?;
        self.nats
            .publish_with_reply_and_headers(
                self.pub_channel.clone(),
                self.reply_channel.clone(),
                propagation::empty_injected_headers(),
                message.into(),
            )
            .await?;
        Ok(())
    }

    pub async fn failed_processing_value(&self, node_id: Id) -> ClientResult<()> {
        let message = serde_json::to_vec(&Request::ValueProcessingFailed {
            change_set_id: self.change_set_id,
            node_id,
        })?;
        self.nats
            .publish_with_reply_and_headers(
                self.pub_channel.clone(),
                self.reply_channel.clone(),
                propagation::empty_injected_headers(),
                message.into(),
            )
            .await?;
        Ok(())
    }

    pub async fn bye(self) -> ClientResult<()> {
        let message = serde_json::to_vec(&Request::Bye {
            change_set_id: self.change_set_id,
        })?;
        self.nats
            .publish_with_reply_and_headers(
                self.pub_channel.clone(),
                self.reply_channel.clone(),
                propagation::empty_injected_headers(),
                message.into(),
            )
            .await?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Client {
    change_set_id: Id,
    pub_channel: Subject,
    reply_channel: Subject,
    subscriber: Subscriber,
    nats: NatsClient,
}

impl Client {
    pub async fn new(
        nats: NatsClient,
        subject_prefix: Option<String>,
        id: Id,
        change_set_id: Id,
    ) -> ClientResult<Self> {
        let (pub_channel, reply_channel) = SubjectGenerator::for_client(subject_prefix, id);
        Ok(Self {
            pub_channel: pub_channel.into(),
            change_set_id,
            subscriber: nats.subscribe(reply_channel.clone()).await?,
            reply_channel: reply_channel.into(),
            nats,
        })
    }

    pub fn clone_into_pub(&self) -> PubClient {
        PubClient {
            pub_channel: self.pub_channel.clone(),
            reply_channel: self.reply_channel.clone(),
            change_set_id: self.change_set_id,
            nats: self.nats.clone(),
        }
    }

    // None means subscriber has been unsubscribed or that the connection has been closed
    #[instrument(
        name = "council_client.fetch_response",
        level = "info",
        skip_all,
        fields(
            response = Empty,
        )
    )]
    pub async fn fetch_response(&mut self) -> ClientResult<Option<Response>> {
        // TODO: timeout so we don't get stuck here forever if council goes away
        // TODO: handle message.data() empty with Status header as 503:
        // https://github.com/nats-io/nats.go/pull/576
        let msg = loop {
            let res = tokio::time::timeout(Duration::from_secs(60), self.subscriber.next()).await;

            match res {
                Ok(msg) => break msg,
                Err(_) => {
                    warn!(
                        change_set_id = ?self.change_set_id,
                        pub_channel = ?self.pub_channel,
                        reply_channel = ?self.reply_channel,
                        "Council client waiting for response for 60 seconds",
                    );
                }
            }
        };

        match msg {
            Some(msg) => {
                let span = Span::current();
                propagation::associate_current_span_from_headers(msg.headers());
                if msg.payload().is_empty() {
                    return Err(ClientError::NoListenerAvailable);
                }
                let response = serde_json::from_slice::<Response>(msg.payload())?;
                span.record("response", field::debug(&response));
                Ok(Some(response))
            }
            None => {
                // TODO(fnichol): I'm guessing at a trace messaage--is this expected behavior, or
                // the sign that something unexpected happened?
                trace!("no response recieved, subscriber stream closed");
                Ok(None)
            }
        }
    }

    #[instrument(
        name = "council_client.register_dependency_graph",
        level = "info",
        skip_all,
        fields()
    )]
    pub async fn register_dependency_graph(&self, dependency_graph: Graph) -> ClientResult<()> {
        self.clone_into_pub()
            .register_dependency_graph(dependency_graph)
            .await
    }

    pub async fn processed_value(&self, node_id: Id) -> ClientResult<()> {
        self.clone_into_pub().processed_value(node_id).await
    }

    pub async fn bye(&self) -> ClientResult<()> {
        self.clone_into_pub().bye().await
    }
}
