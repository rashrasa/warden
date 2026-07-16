use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use log::LevelFilter;
use warden_core::Warden;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::new()
        .target(env_logger::Target::Stdout)
        .filter_level(LevelFilter::Trace)
        .init();

    let mut warden = Warden::new(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::new(127, 0, 0, 1),
        3000,
    )));

    warden.serve().await
}
