use std::{
    collections::HashMap,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::Context;
use http::StatusCode;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper_util::rt::TokioIo;
use log::error;
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{File, create_dir_all},
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::{core::Source, up::http1::make_http1_connection, utils::http_error};

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
pub struct Role {}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Filter {
    Allow,
    Block,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct Permission {
    #[serde(rename = "type")]
    pub filter: Filter,
    pub roles: Vec<String>,
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
    pub permission: Permission,

    #[serde(skip)]
    pub source: Source,
}

impl Location {
    pub async fn call(&self, request: crate::Request) -> anyhow::Result<crate::Response> {
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
            Source::Http(uri, sender) => {
                let host = match uri.host() {
                    None => return Ok(http_error(StatusCode::INTERNAL_SERVER_ERROR)),
                    Some(host) => host,
                };
                let request = match hyper::Request::builder()
                    .header(http::header::HOST, host)
                    .body(request.into_body())
                {
                    Ok(req) => req,
                    Err(err) => {
                        error!("error building downstream response: {err}");
                        return Ok(http_error(StatusCode::INTERNAL_SERVER_ERROR));
                    }
                };

                // TODO: Find better way to share HTTP client
                match sender.lock().await.send_request(request).await {
                    Ok(res) => {
                        let (parts, body) = res.into_parts();
                        let body = match body.collect().await {
                            Ok(bytes) => bytes.to_bytes(),
                            Err(err) => {
                                error!("error collecting upstream response: {err}");
                                return Ok(http_error(StatusCode::INTERNAL_SERVER_ERROR));
                            }
                        };
                        Ok(crate::Response::from_parts(parts, body.into()))
                    }
                    Err(err) => {
                        error!("failed to get response from upstream: {err}");
                        Ok(http_error(StatusCode::BAD_GATEWAY))
                    }
                }
            }
            _ => Ok(http_error(StatusCode::INTERNAL_SERVER_ERROR)),
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
                        Source::StaticHtml(buf)
                    }
                },
                Protocol::Http => {
                    let url = path
                        .parse::<hyper::Uri>()
                        .map_err(|e| std::io::Error::new(ErrorKind::InvalidInput, e))?;
                    let host = url.host().ok_or_else(|| {
                        std::io::Error::new(
                            ErrorKind::InvalidInput,
                            anyhow::anyhow!("invalid uri {path}"),
                        )
                    })?;
                    let address = format!("{host}:80");
                    let stream = TcpStream::connect(address).await?;
                    let io = TokioIo::new(stream);
                    let (sender, conn) = make_http1_connection(io)
                        .await
                        .map_err(|e| std::io::Error::new(ErrorKind::ConnectionRefused, e))?;
                    tokio::spawn(async move {
                        if let Err(err) = conn.await {
                            error!("connection failed: {err:?}");
                        }
                    });
                    Source::Http(url, tokio::sync::Mutex::new(sender))
                }
                Protocol::Https => Source::Https,
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
