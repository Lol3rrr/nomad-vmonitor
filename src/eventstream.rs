use std::{future::Future, sync::Arc, time::Duration};

use bytes::BytesMut;
use reqwest::Url;
use serde::Deserialize;

pub struct EventStream {
    client: reqwest::Client,
    base_url: Url,
    index: usize,
}

impl EventStream {
    pub fn new(client: reqwest::Client, base_url: Url) -> Self {
        Self {
            client,
            base_url,
            index: 0,
        }
    }

    #[tracing::instrument(skip(self, notify))]
    async fn listen(mut self, notify: Arc<tokio::sync::Notify>) {
        let req_url = self.base_url.join("v1/event/stream").expect("");

        let mut pending = BytesMut::new();

        loop {
            let mut specific_url = req_url.clone();
            specific_url.set_query(Some(&format!("index={}", self.index)));

            let resp = self.client.get(specific_url).send().await;

            tracing::debug!("Starting Event-Stream: {}", resp.is_ok());

            if let Ok(mut resp) = resp {
                loop {
                    let chunk = match resp.chunk().await {
                        Ok(Some(c)) => c,
                        _ => break,
                    };

                    if chunk.len() <= 2 {
                        continue;
                    }

                    pending.extend(chunk.as_ref());

                    let event: EventResponse = match serde_json::from_slice(&pending) {
                        Ok(e) => e,
                        Err(err) => {
                            tracing::error!("Parsing Event: {:?}", err);
                            // tracing::error!("Chunk: {:?}", chunk);
                            // tracing::error!("Pending: {:?}", pending);
                            continue;
                        }
                    };

                    pending.clear();

                    tracing::debug!("Event: {:#?}", event);

                    notify.notify_waiters();

                    self.index = event.index;
                }
            } else {
                tracing::error!("{:?}", resp);
            }

            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }

    pub fn run(self) -> (impl Future<Output = ()>, Arc<tokio::sync::Notify>) {
        let notifier = Arc::new(tokio::sync::Notify::new());

        (self.listen(notifier.clone()), notifier)
    }
}

#[derive(Debug, Deserialize)]
struct EventResponse {
    #[serde(rename = "Events")]
    events: Vec<Event>,
    #[serde(rename = "Index")]
    index: usize,
}

#[derive(Debug, Deserialize)]
struct Event {
    #[serde(rename = "FilterKeys")]
    filter_keys: Vec<String>,
    #[serde(rename = "Index")]
    index: usize,
    #[serde(rename = "Key")]
    key: String,
    #[serde(rename = "Namespace")]
    namespace: String,
    #[serde(rename = "Payload")]
    payload: serde_json::Value,
    #[serde(rename = "Topic")]
    topic: EventTopic,
    #[serde(rename = "Type")]
    type_: EventType,
}

#[derive(Debug, Deserialize)]
enum EventTopic {
    ACLToken,
    ACLPolicy,
    ACLRoles,
    Allocation,
    Job,
    Evaluation,
    Deployment,
    Node,
    NodeDrain,
    Service,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
enum EventType {
    ACLTokenUpserted,
    ACLTokenDeleted,
    ACLPolicyUpserted,
    ACLPolicyDeleted,
    ACLRoleUpserted,
    ACLRoleDeleted,
    AllocationCreated,
    AllocationUpdated,
    AllocationUpdateDesiredStatus,
    DeploymentStatusUpdate,
    DeploymentPromotion,
    DeploymentAllocHealth,
    EvaluationUpdated,
    JobRegistered,
    JobDeregistered,
    JobBatchDeregistered,
    NodeRegistration,
    NodeDeregistration,
    NodeEligibility,
    NodeDrain,
    NodeEvent,
    PlanResult,
    ServiceRegistration,
    ServiceDeregistration,
    #[serde(other)]
    Unknown,
}
