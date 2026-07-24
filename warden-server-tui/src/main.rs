use std::{
    net::SocketAddr,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

use crossterm::event::{Event, KeyCode};
use futures::{FutureExt, StreamExt};
use tokio::{select, time::Instant};
use warden_core::Warden;
use warden_server_tui::pages::home::{HomePage, HomePageState, Host, LogBuf, Ssl, Status};

const QUIT_CHAR: char = 'q';
const URL: &str = "127.0.0.1:443";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let buf = LogBuf {
        inner: Arc::new(Mutex::new(Vec::new())),
    };
    env_logger::builder()
        .target(env_logger::Target::Pipe(Box::new(buf.clone())))
        .filter_level(log::LevelFilter::Trace)
        .init();

    let host = SocketAddr::from_str(URL).unwrap();
    let mut stream = crossterm::event::EventStream::new();
    let gateway = Warden::bind(host, "./temp/config.json").await?;

    let mut state = Arc::new(Mutex::new(HomePageState {
        host: Host {
            host,
            ssl: Ssl::Disabled,
        },
        status: Status::Unknown,
        uptime: Duration::ZERO,
    }));

    let start = Instant::now();
    let mut interval = tokio::time::interval(Duration::from_millis(1000));
    let mut terminal = ratatui::init();

    // let (tx_health, mut rx_health) = tokio::sync::watch::channel(Status::Unknown);
    // let mut last_status = Status::Unknown;
    // tokio::spawn(async move {
    // let mut health_interval = tokio::time::interval(Duration::from_millis(5000));
    // loop {
    // health_interval.tick().await;
    // let code = reqwest::get(format!("https://{URL}/status"))
    // .await
    // .unwrap()
    // .status();
    //
    // if code == StatusCode::OK {
    // tx_health.send(Status::Healthy).unwrap();
    // } else {
    // tx_health.send(Status::Unhealthy).unwrap();
    // }
    // }
    // });
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
                let mut state = state.lock().unwrap();
                state.uptime = start.elapsed();
            }
            // status = rx_health.wait_for(|v| *v != last_status) => {
            //     let mut state = state.lock().unwrap();
            //     if let Ok(new_status) = status {
            //         state.status = *new_status;
            //         last_status = *new_status;
            //     }
            // }

        }
    };
    ratatui::restore();

    gateway.close().await?;
    res
}
