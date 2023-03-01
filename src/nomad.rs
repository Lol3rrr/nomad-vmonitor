use serde::Deserialize;

pub type JobListResponse = Vec<JobListEntry>;

#[derive(Debug, Deserialize)]
pub struct JobListEntry {
    pub ID: String,
    ParentID: String,
    Name: String,
    Type: String,
    Priority: usize,
}

#[derive(Debug, Deserialize)]
pub struct ReadJobResponse {
    pub Name: String,
    pub ParentID: String,
    pub TaskGroups: Vec<ReadJobTaskGroup>,
}

#[derive(Debug, Deserialize)]
pub struct ReadJobTaskGroup {
    pub Name: String,
    Count: usize,
    pub Tasks: Vec<ReadJobTask>,
}

#[derive(Debug, Deserialize)]
pub struct ReadJobTask {
    pub Name: String,
    #[serde(flatten)]
    pub config: ReadJobConfig,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "Driver", content = "Config")]
pub enum ReadJobConfig {
    #[serde(rename = "docker")]
    Docker { image: String },
    #[serde(rename = "raw_exec")]
    RawExec {},
}

pub async fn list_jobs(
    client: &reqwest::Client,
    base_url: &reqwest::Url,
) -> Result<JobListResponse, ()> {
    let target_url = base_url.join("v1/jobs").map_err(|e| ())?;

    let resp = client.get(target_url).send().await.map_err(|e| ())?;

    if !resp.status().is_success() {
        return Err(());
    }

    let raw_content = resp.bytes().await.map_err(|e| ())?;

    serde_json::from_slice(&raw_content).map_err(|e| ())
}

pub async fn read_job(
    client: &reqwest::Client,
    base_url: &reqwest::Url,
    job_id: &str,
) -> Result<ReadJobResponse, ()> {
    let target_url = base_url.join(&format!("v1/job/{job_id}")).map_err(|e| ())?;

    let resp = client.get(target_url).send().await.map_err(|e| ())?;

    if !resp.status().is_success() {
        return Err(());
    }

    let raw_content = resp.bytes().await.map_err(|e| ())?;

    serde_json::from_slice(&raw_content).map_err(|e| {
        println!("{}", std::str::from_utf8(&raw_content).unwrap());
        dbg!(e);
        ()
    })
}
