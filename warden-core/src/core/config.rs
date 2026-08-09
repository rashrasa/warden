use std::{
    collections::HashMap,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::Context;
use log::{error, warn};
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{File, create_dir_all},
    io::{AsyncReadExt, AsyncWriteExt},
};

use crate::{
    core::{Source, SourceInner},
    up::Upstream,
};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ConfigurationDesc {
    #[serde(skip)]
    pub path: PathBuf,
    pub handlers: HashMap<String, LocationDesc>,
    pub roles: HashMap<String, RoleDesc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolDesc {
    Html,
    Http,
    Https,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct RoleDesc {
    identity: Vec<IdentityDesc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum IdentityDesc {
    Jwt { secret: String },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum FilterDesc {
    Allow,
    Block,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct PermissionDesc {
    #[serde(rename = "type")]
    pub filter: FilterDesc,
    pub roles: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum CacheDesc {
    None,
    Static,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LocationDesc {
    pub protocol: ProtocolDesc,
    pub path: String,
    pub cache: CacheDesc,
    pub permission: PermissionDesc,

    #[serde(skip)]
    pub source: Source,
}

impl ConfigurationDesc {
    /// Propogates std::io errors for handling. Serialization errors are represented
    /// as std::io::ErrorKind::InvalidData.
    pub async fn from_path(p: impl AsRef<Path>) -> std::io::Result<Self> {
        let p = p.as_ref();
        let mut buf = Vec::new();

        File::open(p).await?.read_to_end(&mut buf).await?;

        let mut config = serde_json::from_slice::<Self>(&buf)
            .map_err(|err| std::io::Error::new(ErrorKind::InvalidData, err))?;

        for handler in config.handlers.values_mut() {
            let protocol = handler.protocol.clone();
            let cache = handler.cache.clone();
            let path = handler.path.clone();

            handler.source = match protocol {
                ProtocolDesc::Html => match cache {
                    CacheDesc::None => Source::new(SourceInner::DynamicHtml(path.into())),
                    CacheDesc::Static => {
                        let mut buf = vec![];
                        let mut file = File::open(&path).await?;

                        let meta = file.metadata().await?;

                        if meta.len() > crate::MAX_STATIC_HTML_FILE_SIZE {
                            return Err(std::io::Error::new(
                                ErrorKind::FileTooLarge,
                                anyhow::Error::msg(format!(
                                    "html file at {path:?} exceeds max size {}",
                                    crate::MAX_STATIC_HTML_FILE_SIZE
                                )),
                            ));
                        }
                        file.read_to_end(&mut buf).await?;
                        Source::new(SourceInner::StaticHtml(buf))
                    }
                },
                ProtocolDesc::Http => {
                    let url = path
                        .parse::<hyper::Uri>()
                        .map_err(|e| std::io::Error::new(ErrorKind::InvalidInput, e))?;

                    let up = Upstream::http1(&url).await.map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::ConnectionAborted, e)
                    })?;
                    Source::new(SourceInner::Http(url, up))
                }
                ProtocolDesc::Https => {
                    let url = path
                        .parse::<hyper::Uri>()
                        .map_err(|e| std::io::Error::new(ErrorKind::InvalidInput, e))?;

                    let up = Upstream::http2(&url).await.map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::ConnectionAborted, e)
                    })?;
                    Source::new(SourceInner::Https(url, up))
                }
            };
        }

        config.path = p.to_path_buf();

        let mut missing_env = vec![];

        // replace special strings
        for r in config.roles.values_mut() {
            for ident in r.identity.iter_mut() {
                if let IdentityDesc::Jwt { secret } = ident
                    && secret.starts_with("!env ")
                    && secret.len() > 5
                {
                    let key = &secret[5..];
                    match std::env::var(key) {
                        Ok(v) => *secret = v,
                        Err(_) => {
                            missing_env.push(key.to_owned());
                        }
                    }
                }
            }
        }

        if !missing_env.is_empty() {
            Err(std::io::Error::new(
                ErrorKind::NotFound,
                anyhow::anyhow!("missing env variables: {}", missing_env.join(", ")),
            ))
        } else {
            Ok(config)
        }
    }

    pub async fn from_path_or_default(p: impl AsRef<Path>) -> Self {
        let p = p.as_ref();
        let mut config = match Self::from_path(p)
            .await
            .with_context(|| "config serialization failed")
        {
            Ok(ser) => ser,
            Err(e) => {
                error!("{e:#}");
                warn!("falling back to default config");
                ConfigurationDesc::default()
            }
        };
        config.path = p.to_path_buf();
        config
    }

    pub async fn save_if_missing(&self) -> std::io::Result<()> {
        if !self.path.try_exists()? {
            if let Some(parent) = self.path.parent() {
                create_dir_all(parent).await?;
            }
            File::create(&self.path)
                .await?
                .write_all(
                    &serde_json::to_vec_pretty(self)
                        .map_err(|err| std::io::Error::new(ErrorKind::InvalidData, err))?,
                )
                .await?;
        }

        Ok(())
    }
}
