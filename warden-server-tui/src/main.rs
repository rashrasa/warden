use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    time::Duration,
};

use crossterm::event::{Event, KeyCode, poll};
use warden_core::Warden;
use warden_server_tui::pages::home::{HomePage, HomePageState, Host, Ssl, Status};

const QUIT_CHAR: char = 'q';
const HOST: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 3000));

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut gateway = Warden::bind(HOST).await?;

    let mut state = HomePageState {
        host: Host {
            host: HOST,
            ssl: Ssl::Disabled,
        },
        status: Status::Healthy,
        lifetime_connections: 0,
    };
    let (tx, mut rx) = tokio::sync::watch::channel::<usize>(0);
    tokio::spawn(async move {
        let tx = tx;
        loop {
            gateway.serve_next().await.unwrap();
            let _ = tx.send(gateway.lifetime_connections());
        }
    });

    let mut terminal = ratatui::init();
    let res = loop {
        terminal.draw(|frame| frame.render_stateful_widget(HomePage, frame.area(), &mut state))?;
        if let Ok(true) = poll(Duration::from_millis(100)) {
            match crossterm::event::read()? {
                Event::Key(k) => match k.code {
                    KeyCode::Char(QUIT_CHAR) => {
                        break Ok(());
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        state.lifetime_connections = *rx.borrow_and_update();
    };

    ratatui::restore();

    res
}
