use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use log::{LevelFilter, info};
use tokio::select;
use warden_core::Warden;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::new()
        .target(env_logger::Target::Stdout)
        .filter_level(LevelFilter::Info)
        .init();

    let mut warden = Warden::bind(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::new(127, 0, 0, 1),
        443,
    )))
    .await?;

    loop {
        select! {
            res = warden.serve_async() => {
                res?;
            }
            _ = tokio::signal::ctrl_c() => {
                info!("closing server");
                break;
            }
        }
    }

    Ok(())
}
