use std::{
    collections::HashMap,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::Context;
use http_body_util::Full;
use hyper::body::Bytes;
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{File, create_dir_all},
    io::{AsyncReadExt, AsyncWriteExt},
};

use crate::core::Source;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Configuration {
    #[serde(skip)]
    pub path: PathBuf,
    pub handlers: HashMap<String, Location>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Html,
    Http,
    Https,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Cache {
    None,
    Static,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Location {
    pub protocol: Protocol,
    pub path: String,
    pub cache: Cache,
    #[serde(skip)]
    pub source: Source,
}

impl Location {
    pub async fn call(&self, _request: crate::Request) -> anyhow::Result<crate::Response> {
        match &self.source {
            Source::StaticHtml(d) => Ok(crate::Response::new(Full::new(Bytes::from(d.clone())))),
            Source::DynamicHtml(p) => {
                let mut buf = Vec::new();
                File::open(p)
                    .await
                    .with_context(|| "could not open dynamic page")?
                    .read_to_end(&mut buf)
                    .await
                    .with_context(|| "could not read dynamic page")?;

                Ok(crate::Response::new(Full::new(Bytes::from(buf))))
            }
            _ => unimplemented!(),
        }
    }
}

impl Configuration {
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
                Protocol::Html => match cache {
                    Cache::None => Source::DynamicHtml(path.into()),
                    Cache::Static => {
                        let mut buf = vec![];
                        File::open(&path).await?.read_to_end(&mut buf).await?;
                        Source::StaticHtml(buf)
                    }
                },
                _ => unimplemented!(),
            };
        }

        config.path = p.to_path_buf();

        Ok(config)
    }

    pub async fn from_path_or_default(p: impl AsRef<Path>) -> Self {
        let p = p.as_ref();
        let mut config = Self::from_path(p).await.unwrap_or_default();
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
