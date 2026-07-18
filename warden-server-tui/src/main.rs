use crossterm::event::KeyCode;
use ratatui::{DefaultTerminal, Frame, layout::Offset};

const QUIT_CHAR: char = 'q';

fn main() -> anyhow::Result<()> {
    ratatui::run(app)?;
    Ok(())
}

fn app(terminal: &mut DefaultTerminal) -> anyhow::Result<()> {
    loop {
        terminal.draw(render)?;
        if let Some(k) = crossterm::event::read()?.as_key_event() {
            if k.code == KeyCode::Char(QUIT_CHAR) {
                break Ok(());
            }
        }
    }
}

fn render(frame: &mut Frame) {
    frame.render_widget("Hello World", frame.area());
    let quit_str = "Press q to quit";
    frame.render_widget(
        quit_str,
        frame.area().offset(Offset::new(
            frame.area().width as i32 - quit_str.len() as i32,
            frame.area().height as i32 - 8,
        )),
    );
}
