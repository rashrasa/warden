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
        uptime: Duration::ZERO,
        active_connections: vec![],
    };

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
            res = gateway.serve_next() => {
                res?;
            }
            _ = interval.tick() => {
                state.active_connections = gateway.connections().to_vec();
                state.uptime = start.elapsed();
                state.status = if gateway.is_healthy() {Status::Healthy} else {Status::Unhealthy};

            }
        }
    };

    ratatui::restore();

    res
}
