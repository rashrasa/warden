use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    time::Duration,
};

use crossterm::event::{Event, KeyCode};
use futures::{FutureExt, StreamExt};
use tokio::{select, time::Instant};
use warden_core::Warden;
use warden_server_tui::pages::home::{HomePage, HomePageState, Host, Ssl, Status};

const QUIT_CHAR: char = 'q';
const HOST: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 3000));

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut stream = crossterm::event::EventStream::new();
    let mut gateway = Warden::bind(HOST).await?;

    let mut state = HomePageState {
        host: Host {
            host: HOST,
            ssl: Ssl::Disabled,
        },
        status: Status::Healthy,
        lifetime_connections: 0,
        uptime: Duration::ZERO,
    };
    let (tx, mut rx) = tokio::sync::watch::channel::<usize>(0);
    tokio::spawn(async move {
        let tx = tx;
        loop {
            gateway.serve_next().await.unwrap();
            let _ = tx.send(gateway.lifetime_connections());
        }
    });
    let start = Instant::now();
    let mut interval = tokio::time::interval(Duration::from_millis(1000));

    let mut terminal = ratatui::init();

    let res = loop {
        terminal.draw(|frame| frame.render_stateful_widget(HomePage, frame.area(), &mut state))?;
        select! {
            event_result = stream.next().fuse() => {
                if let Some(e) = event_result {
                    match e? {
                        Event::Key(k) => match k.code {
                            KeyCode::Char(QUIT_CHAR) => {
                                break Ok(());
                            }
                            _ => {}
                        },
                    _ => {}
                    }
                }
            }
            update = rx.wait_for(|v| *v != state.lifetime_connections) => {
                state.lifetime_connections = *update.unwrap();
            }
            _ = interval.tick() => {
                state.uptime = start.elapsed();
            }
        }
    };

    ratatui::restore();

    res
}
