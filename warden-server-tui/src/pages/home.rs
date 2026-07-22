use std::{
    io::Write,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use ratatui::{
    layout::Offset,
    style::Style,
    text::Line,
    widgets::{Block, Borders, StatefulWidget, Widget},
};

use crate::components::text::StyledLabelledText;

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum Status {
    Healthy,
    Unhealthy,
    Unknown,
}

pub enum Ssl {
    Disabled,
}

pub struct Host {
    pub host: SocketAddr,
    pub ssl: Ssl,
}

#[derive(Clone)]
pub struct LogBuf {
    pub inner: Arc<Mutex<Vec<u8>>>,
}

impl Write for LogBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.lock().unwrap().flush()
    }
}

pub struct HomePageState {
    pub host: Host,
    pub status: Status,
    pub uptime: Duration,
}

pub struct HomePage;

impl StatefulWidget for HomePage {
    type State = Arc<Mutex<HomePageState>>;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        let state = state.lock().unwrap();
        Block::default()
            .title("Home")
            .borders(Borders::ALL)
            .border_style(Style::default().cyan())
            .render(area, buf);
        let quit_hint = "Press q to quit";
        let quit_text = Line::from(quit_hint);
        quit_text.render(
            area.offset(Offset {
                x: area.width as i32 - quit_hint.len() as i32 - 4,
                y: 0,
            }),
            buf,
        );
        let mut y = 1;

        let mut status = StyledLabelledText {
            label: "Status".into(),
            value: match state.status {
                Status::Healthy => "Healthy".into(),
                Status::Unhealthy => "Unhealthy".into(),
                Status::Unknown => "Unknown".into(),
            },
            label_style: Style::default().yellow(),
            value_style: Style::default().gray(),
        };
        status.render(area.offset(Offset { x: 1, y }), buf);
        y += 1;

        let mut host = StyledLabelledText {
            label: "Host".into(),
            value: state.host.host.to_string(),

            label_style: Style::default().yellow(),
            value_style: Style::default().gray(),
        };
        host.render(area.offset(Offset { x: 1, y }), buf);
        y += 1;

        let mut ssl = StyledLabelledText {
            label: "SSL Mode".into(),
            value: match state.host.ssl {
                Ssl::Disabled => "Disabled".into(),
            },

            label_style: Style::default().yellow(),
            value_style: Style::default().gray(),
        };
        ssl.render(area.offset(Offset { x: 1, y }), buf);
        y += 1;

        let mut uptime = StyledLabelledText {
            label: "Uptime".into(),
            value: format!("{}s", state.uptime.as_secs()),

            label_style: Style::default().yellow(),
            value_style: Style::default().gray(),
        };
        uptime.render(area.offset(Offset { x: 1, y }), buf);
        y += 8;
    }
}
