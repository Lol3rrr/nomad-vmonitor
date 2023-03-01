use std::{sync::Arc, time::Duration};

use prometheus::{Encoder, Registry, TextEncoder};

mod docker;
mod metrics;
mod nomad;

#[derive(Debug)]
pub struct Client {
    client: reqwest::Client,
    nomad_url: reqwest::Url,
    registry: Registry,
    general: metrics::Metrics,
}

impl Client {
    pub fn new(nomad_url: impl reqwest::IntoUrl) -> Self {
        let reg = Registry::new();

        let general_metrics = metrics::Metrics::new(&reg);

        Self {
            client: reqwest::Client::builder().build().unwrap(),
            nomad_url: nomad_url.into_url().unwrap(),
            registry: reg,
            general: general_metrics,
        }
    }

    pub fn get_metrics(&self) -> String {
        let mut buffer = vec![];
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        encoder.encode(&metric_families, &mut buffer).unwrap();

        String::from_utf8(buffer).unwrap()
    }

    pub async fn run(self: Arc<Self>) {
        let sleep_time = Duration::from_secs(60 * 15);

        loop {
            self.check().await;

            tokio::time::sleep(sleep_time).await;
        }
    }

    #[tracing::instrument(skip(self))]
    async fn check(&self) {
        tracing::info!("Running Check");
        tracing::info!("Loading Tasks...");
        let raw_task_list = nomad::list_jobs(&self.client, &self.nomad_url)
            .await
            .unwrap();

        let tasks = {
            let mut tmp = Vec::new();
            for raw_task in raw_task_list {
                let task = nomad::read_job(&self.client, &self.nomad_url, &raw_task.id)
                    .await
                    .unwrap();

                if !task.parent_id.is_empty() {
                    tracing::warn!("Skipping Job that has ParentID - {:?}", task.name);
                    continue;
                }

                tmp.push(task);
            }
            tmp
        };

        tracing::info!("Processing Jobs...");

        let job_task_iter = tasks.into_iter().flat_map(|job| {
            job.task_groups.into_iter().flat_map(move |jgroup| {
                let j_name = job.name.clone();
                let g_name = jgroup.name.clone();
                jgroup
                    .tasks
                    .into_iter()
                    .map(move |task| (j_name.clone(), g_name.clone(), task))
            })
        });

        let updates = {
            let mut tmp = Vec::new();

            for (jname, gname, task) in job_task_iter {
                let get_version = move || async {
                    match task.config {
                        nomad::ReadJobConfig::Docker { image: raw_image } => {
                            let image = match docker::Image::parse(raw_image) {
                                Ok(i) => i,
                                Err(raw) => {
                                    tracing::warn!("Could not parse Image: {:?}", raw);
                                    return None;
                                }
                            };

                            if image.name.contains('.') {
                                tracing::error!("Image Contains '.': {:?}", image.name);
                                // return None;
                            }

                            let image_version = match image.tag.parse_version() {
                                Ok(v) => v,
                                Err(e) => {
                                    tracing::error!(
                                        "Parsing Image ({}) Version: {:?}",
                                        image.name,
                                        image.tag
                                    );

                                    return None;
                                }
                            };

                            if docker::Version::Latest == image_version {
                                tracing::warn!("Skipping Image check as its already latest");
                                return Some(metrics::UpdatedVersion::UpToDate {
                                    version: format!("{image_version}"),
                                });
                            }

                            let tags = match docker::get_tags(&self.client, &image).await {
                                Ok(t) => t,
                                Err(e) => {
                                    tracing::error!("Getting Tags for '{:?}': {:?}", image, e);
                                    return None;
                                }
                            };

                            let latest_version = match tags
                                .iter()
                                .filter_map(|tag| {
                                    let raw_tag = docker::RawTag::new(tag);
                                    raw_tag.parse_version().ok()
                                })
                                .filter(|v| v.fully_qualified())
                                .max()
                            {
                                Some(v) => v,
                                None => {
                                    return None;
                                }
                            };

                            if latest_version > image_version {
                                Some(metrics::UpdatedVersion::OutOfDate {
                                    current: format!("{image_version}"),
                                    newest: format!("{latest_version}"),
                                })
                            } else {
                                Some(metrics::UpdatedVersion::UpToDate {
                                    version: format!("{image_version}"),
                                })
                            }
                        }
                        nomad::ReadJobConfig::RawExec {} => {
                            tracing::warn!("Not implemented for Raw-Exec");

                            Some(metrics::UpdatedVersion::UpToDate {
                                version: "".to_string(),
                            })
                        }
                    }
                };

                let result = match get_version().await {
                    Some(r) => r,
                    None => continue,
                };

                tmp.push((jname, gname, task.name, result));
            }

            tmp
        };

        tracing::info!("Updating Metrics...");

        self.general.clear();
        for (job_name, group_name, task_name, version) in updates {
            self.general
                .update(&job_name, &group_name, &task_name, version);
        }

        tracing::info!("Check Done");
    }
}
