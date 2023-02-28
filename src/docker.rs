use std::{borrow::Cow, fmt::Display};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct TagListResponse {
    name: String,
    tags: Vec<String>,
}

async fn auth(client: &reqwest::Client, image_name: &str) -> Result<String, ()> {
    let mut base_url = reqwest::Url::parse("https://auth.docker.io/token").unwrap();
    base_url
        .query_pairs_mut()
        .append_pair("service", "registry.docker.io")
        .append_pair("scope", &format!("repository:{image_name}:pull"))
        .finish();

    let resp = client.get(base_url).send().await.map_err(|e| ())?;
    if !resp.status().is_success() {
        return Err(());
    }

    let raw_content = resp.bytes().await.map_err(|e| ())?;

    let content: serde_json::Value = serde_json::from_slice(&raw_content).unwrap();

    let token = content
        .as_object()
        .unwrap()
        .get("token")
        .unwrap()
        .as_str()
        .unwrap();

    let jwt_res: jwt::Token<jwt::Header, serde_json::Value, jwt::Unverified> =
        jwt::Token::parse_unverified(token).map_err(|e| ())?;

    Ok(token.to_string())
}

pub async fn get_tags(client: &reqwest::Client, image_name: &str) -> Result<Vec<String>, ()> {
    let token = auth(client, image_name).await?;

    let registry_url = reqwest::Url::parse("https://registry.hub.docker.com").unwrap();

    let target_url = registry_url
        .join(&format!("v2/{image_name}/tags/list"))
        .unwrap();
    // dbg!(&target_url);

    let resp = client
        .get(target_url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| ())?;

    if !resp.status().is_success() {
        dbg!(resp.status(), image_name);
        return Err(());
    }

    let raw_content = resp.bytes().await.map_err(|e| ())?;

    let content: TagListResponse = serde_json::from_slice(&raw_content).unwrap();

    Ok(content.tags)
}

pub struct Image {
    name: String,
    tag: RawTag<'static>,
}

#[derive(Debug)]
pub struct RawTag<'a> {
    tag: Cow<'a, str>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Version {
    Latest,
    Semantic {
        major: usize,
        minor: Option<usize>,
        patch: Option<usize>,
    },
}

impl Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Latest => write!(f, "latest"),
            Self::Semantic {
                major,
                minor,
                patch,
            } => {
                write!(f, "{major}")?;

                match minor {
                    Some(minor) => write!(f, ".{minor}")?,
                    None => return Ok(()),
                };

                match patch {
                    Some(patch) => write!(f, ".{patch}"),
                    None => Ok(()),
                }
            }
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Self::Latest, Self::Latest) => Some(std::cmp::Ordering::Equal),
            (Self::Latest, _) => Some(std::cmp::Ordering::Less),
            (_, Self::Latest) => Some(std::cmp::Ordering::Greater),
            (
                Self::Semantic {
                    major: smajor,
                    minor: sminor,
                    patch: spatch,
                },
                Self::Semantic {
                    major: omajor,
                    minor: ominor,
                    patch: opatch,
                },
            ) => {
                match smajor.cmp(omajor) {
                    std::cmp::Ordering::Equal => {}
                    other => return Some(other),
                };

                match (sminor, ominor) {
                    (None, None) => return Some(std::cmp::Ordering::Equal),
                    (Some(_), None) => return Some(std::cmp::Ordering::Less),
                    (None, Some(_)) => return Some(std::cmp::Ordering::Greater),
                    (Some(sm), Some(om)) => match sm.cmp(om) {
                        std::cmp::Ordering::Equal => {}
                        other => return Some(other),
                    },
                };

                match (spatch, opatch) {
                    (None, None) => Some(std::cmp::Ordering::Equal),
                    (Some(_), None) => Some(std::cmp::Ordering::Less),
                    (None, Some(_)) => Some(std::cmp::Ordering::Greater),
                    (Some(sp), Some(op)) => Some(sp.cmp(op)),
                }
            }
        }
    }
}
impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).expect("Should always work")
    }
}

impl<'a> RawTag<'a> {
    pub fn new(t: &'a str) -> Self {
        Self {
            tag: Cow::Borrowed(t),
        }
    }
    pub fn parse_version(&self) -> Result<Version, ()> {
        if self.tag.eq("latest") {
            return Ok(Version::Latest);
        }

        let tag = self.tag.strip_prefix('v').unwrap_or(self.tag.as_ref());

        let mut parts = tag.split('.');

        let raw_major = parts.next().ok_or(())?;
        let major: usize = raw_major.parse().map_err(|e| ())?;

        let raw_minor = parts.next();
        let minor: Option<usize> = raw_minor.and_then(|m| m.parse().ok());

        let raw_patch = parts.next();
        let patch: Option<usize> = raw_patch.and_then(|m| m.parse().ok());

        Ok(Version::Semantic {
            major,
            minor,
            patch,
        })
    }
}

impl Version {
    pub fn fully_qualified(&self) -> bool {
        match self {
            Self::Latest => true,
            Self::Semantic { minor, patch, .. } => minor.is_some() && patch.is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_latest() {
        let tag = RawTag::new("latest");
        let version = tag.parse_version().expect("Valid Version");

        assert_eq!(Version::Latest, version);
    }

    #[test]
    fn tag_semantic() {
        let tag = RawTag::new("1.2.3");
        let version = tag.parse_version().expect("Valid Version");

        assert_eq!(
            Version::Semantic {
                major: 1,
                minor: Some(2),
                patch: Some(3)
            },
            version
        );
    }

    #[test]
    fn tag_semantic_with_leading_v() {
        let tag = RawTag::new("v1.2.3");
        let version = tag.parse_version().expect("Valid Version");

        assert_eq!(
            Version::Semantic {
                major: 1,
                minor: Some(2),
                patch: Some(3)
            },
            version
        );
    }
}
