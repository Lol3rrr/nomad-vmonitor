use std::{borrow::Cow, collections::BTreeMap, fmt::Display};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct TagListResponse {
    name: String,
    tags: Vec<String>,
}

#[derive(Debug)]
pub enum AuthError {
    SendRequest(reqwest::Error),
    StatusCode(reqwest::StatusCode),
    LoadingBytes(reqwest::Error),
    JwtToken(jwt::Error),
}

#[derive(Debug)]
struct AuthConfig {
    realm: String,
    service: String,
    scope: String,
}

async fn auth(client: &reqwest::Client, conf: &AuthConfig) -> Result<String, AuthError> {
    let mut base_url = reqwest::Url::parse(&conf.realm).unwrap();
    base_url
        .query_pairs_mut()
        .append_pair("service", &conf.service)
        .append_pair("scope", &conf.scope)
        .append_pair("client_id", "Nomad-VMonitor")
        .finish();

    let resp = client
        .get(base_url)
        .send()
        .await
        .map_err(AuthError::SendRequest)?;
    if !resp.status().is_success() {
        return Err(AuthError::StatusCode(resp.status()));
    }

    let raw_content = resp.bytes().await.map_err(AuthError::LoadingBytes)?;

    let content: serde_json::Value = serde_json::from_slice(&raw_content).unwrap();

    let token = content
        .as_object()
        .unwrap()
        .get("token")
        .unwrap()
        .as_str()
        .unwrap();

    let _: jwt::Token<jwt::Header, serde_json::Value, jwt::Unverified> =
        jwt::Token::parse_unverified(token).map_err(AuthError::JwtToken)?;

    Ok(token.to_string())
}

#[derive(Debug)]
pub enum GetTagsError {
    AuthError(AuthError),
    FailedAuth,
    SendRequest(reqwest::Error),
    StatusCode(reqwest::StatusCode),
    LoadingBytes(reqwest::Error),
}

enum FetchResult {
    Ok(TagListResponse),
    NeedsAuth(AuthConfig),
    Err(GetTagsError),
}

async fn try_get_tags(
    client: &reqwest::Client,
    image: &Image,
    token: Option<String>,
) -> FetchResult {
    let registry_url = reqwest::Url::parse("https://registry.hub.docker.com").unwrap();

    let target_url = registry_url
        .join(&match &image.namespace {
            Some(n) => format!("v2/{}/{}/tags/list", n, image.name),
            None => format!("v2/library/{}/tags/list", image.name),
        })
        .unwrap();

    let mut req = client.get(target_url);
    if let Some(token) = token {
        req = req.bearer_auth(token);
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => return FetchResult::Err(GetTagsError::SendRequest(e)),
    };

    let statuscode = resp.status();
    let headers = resp.headers().clone();

    let raw_content = match resp.bytes().await.map_err(GetTagsError::LoadingBytes) {
        Ok(c) => c,
        Err(e) => return FetchResult::Err(e),
    };

    if !statuscode.is_success() {
        if statuscode.as_u16() == 401 {
            let auth_header = match headers.get("www-authenticate") {
                Some(h) => h,
                None => return FetchResult::Err(GetTagsError::FailedAuth),
            };

            let auth_header_content = auth_header.to_str().unwrap();

            let (_, raw_parts) = auth_header_content.split_once(' ').unwrap();

            let mut parts = raw_parts
                .split(',')
                .filter_map(|part| part.split_once('='))
                .map(|(key, val)| (key, val.replace('"', "")))
                .collect::<BTreeMap<_, _>>();

            return FetchResult::NeedsAuth(AuthConfig {
                realm: parts.remove("realm").unwrap(),
                service: parts.remove("service").unwrap(),
                scope: parts.remove("scope").unwrap(),
            });
        }

        return FetchResult::Err(GetTagsError::StatusCode(statuscode));
    }

    FetchResult::Ok(serde_json::from_slice(&raw_content).unwrap())
}

pub async fn get_tags(
    client: &reqwest::Client,
    image: &Image,
) -> Result<Vec<String>, GetTagsError> {
    let auth_conf = match try_get_tags(client, image, None).await {
        FetchResult::Ok(r) => return Ok(r.tags),
        FetchResult::NeedsAuth(conf) => conf,
        FetchResult::Err(e) => return Err(e),
    };

    let token = auth(client, &auth_conf)
        .await
        .map_err(GetTagsError::AuthError)?;

    match try_get_tags(client, image, Some(token)).await {
        FetchResult::Ok(r) => Ok(r.tags),
        FetchResult::NeedsAuth(_) => Err(GetTagsError::FailedAuth),
        FetchResult::Err(e) => Err(e),
    }
}

#[derive(Debug, PartialEq)]
pub struct Image {
    pub registry: Cow<'static, str>,
    pub namespace: Option<String>,
    pub name: String,
    pub tag: RawTag<'static>,
}

impl Image {
    pub fn parse(raw: String) -> Result<Self, String> {
        if raw.contains('$') {
            return Err(raw);
        }

        let (raw_name, tag) = match raw.split_once(':') {
            Some((first, second)) => (
                first,
                RawTag {
                    tag: Cow::Owned(second.to_owned()),
                },
            ),
            None => (
                raw.as_str(),
                RawTag {
                    tag: Cow::Borrowed("latest"),
                },
            ),
        };

        let mut parts: Vec<_> = raw_name.split('/').collect();

        if parts.is_empty() {
            return Err(raw);
        }
        let registry = if parts.first().unwrap().contains('.') {
            Cow::Owned(parts.remove(0).to_string())
        } else {
            Cow::Borrowed("registry.hub.docker.com")
        };

        let (namespace, name) = if parts.len() == 1 {
            (None, parts.remove(0))
        } else if parts.len() == 2 {
            (Some(parts.remove(0).to_string()), parts.remove(0))
        } else {
            return Err(raw);
        };

        Ok(Self {
            registry,
            namespace,
            name: name.to_string(),
            tag,
        })
    }
}

#[derive(Debug, PartialEq)]
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
    fn parse_image() {
        assert_eq!(
            Ok(Image {
                registry: Cow::Borrowed("registry.hub.docker.com"),
                namespace: Some("user".to_string()),
                name: "test".to_string(),
                tag: RawTag {
                    tag: Cow::Borrowed("version")
                }
            }),
            Image::parse("user/test:version".to_string()),
        );

        assert_eq!(
            Ok(Image {
                registry: Cow::Borrowed("registry.hub.docker.com"),
                namespace: None,
                name: "test".to_string(),
                tag: RawTag {
                    tag: Cow::Borrowed("version")
                }
            }),
            Image::parse("test:version".to_string()),
        );

        assert_eq!(
            Ok(Image {
                registry: Cow::Borrowed("test.com"),
                namespace: Some("user".to_string()),
                name: "test".to_string(),
                tag: RawTag {
                    tag: Cow::Borrowed("version")
                }
            }),
            Image::parse("test.com/user/test:version".to_string()),
        );
    }

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
