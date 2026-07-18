use crossterm::event::KeyCode;
use warden_server_tui::pages::home::{HomePage, HomePageState, Host, Ssl, Status};

const QUIT_CHAR: char = 'q';

fn main() -> anyhow::Result<()> {
    let mut state = HomePageState {
        host: Host {
            ip: [127, 0, 0, 1],
            port: 3000,
            ssl: Ssl::Disabled,
        },
        status: Status::Healthy,
    };

    ratatui::run::<_, anyhow::Result<()>>(|terminal| {
        loop {
            terminal
                .draw(|frame| frame.render_stateful_widget(HomePage, frame.area(), &mut state))?;
            if let Some(k) = crossterm::event::read()?.as_key_event() {
                if k.code == KeyCode::Char(QUIT_CHAR) {
                    return Ok(());
                }
            }
        }
    })?;
    Ok(())
}
