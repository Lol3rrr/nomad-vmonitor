use std::{future::Future, sync::Arc, time::Duration};

use bytes::{Buf, BytesMut};
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

                    pending.extend(chunk.as_ref());

                    let tmp = pending
                        .iter()
                        .enumerate()
                        .find(|(_, v)| **v == b'\n')
                        .map(|(i, _)| i);
                    let end_index = match tmp {
                        Some(i) => i,
                        None => continue,
                    };

                    let content = pending.split_to(end_index);
                    let _ = pending.split_to(1);

                    let event: EventResponse = match serde_json::from_slice(&content) {
                        Ok(e) => e,
                        Err(err) => {
                            tracing::error!("Parsing Event: {:?}", err);
                            continue;
                        }
                    };

                    tracing::debug!("Event: {:#?}", event);

                    if let Some(index) = event.index {
                        self.index = core::cmp::max(self.index, index);
                    }

                    notify.notify_waiters();
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
    events: Option<Vec<Event>>,
    #[serde(rename = "Index")]
    index: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct Event {
    #[serde(rename = "FilterKeys", default)]
    filter_keys: Option<Vec<String>>,
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
