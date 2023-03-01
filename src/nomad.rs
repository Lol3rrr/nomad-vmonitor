use serde::Deserialize;

pub type JobListResponse = Vec<JobListEntry>;

#[derive(Debug, Deserialize)]
pub struct JobListEntry {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "ParentID")]
    parentID: String,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Type")]
    type_: String,
    #[serde(rename = "Priority")]
    priority: usize,
}

#[derive(Debug, Deserialize)]
pub struct ReadJobResponse {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "ParentID")]
    pub parent_id: String,
    #[serde(rename = "TaskGroups")]
    pub task_groups: Vec<ReadJobTaskGroup>,
}

#[derive(Debug, Deserialize)]
pub struct ReadJobTaskGroup {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Count")]
    count: usize,
    #[serde(rename = "Tasks")]
    pub tasks: Vec<ReadJobTask>,
}

#[derive(Debug, Deserialize)]
pub struct ReadJobTask {
    #[serde(rename = "Name")]
    pub name: String,
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
