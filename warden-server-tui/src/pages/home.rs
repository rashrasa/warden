use ratatui::{
    buffer::Buffer,
    layout::{Offset, Rect},
    style::Style,
    text::{Line, Text},
    widgets::{Block, Borders, StatefulWidget, Widget},
};

pub enum Status {
    Healthy,
    Unhealthy,
}

pub enum Ssl {
    Disabled,
}

pub struct Host {
    pub ip: [u8; 4],
    pub port: u16,
    pub ssl: Ssl,
}

pub struct HomePageState {
    pub host: Host,
    pub status: Status,
}

pub struct HomePage;

impl StatefulWidget for HomePage {
    type State = HomePageState;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        Block::default()
            .title("Home")
            .borders(Borders::ALL)
            .border_style(Style::default().cyan())
            .render(area, buf);

        let mut status = StyledLabelledText {
            label: "Status".into(),
            value: match state.status {
                Status::Healthy => "Healthy".into(),
                Status::Unhealthy => "Unhealthy".into(),
            },
            label_style: Style::default().green(),
            value_style: Style::new().gray(),
        };
        status.render(area.offset(Offset { x: 1, y: 1 }), buf);

        let mut host = StyledLabelledText {
            label: "Host".into(),
            label_style: Style::default().green(),

            value: format!(
                "{}.{}.{}.{}:{}",
                state.host.ip[0],
                state.host.ip[1],
                state.host.ip[2],
                state.host.ip[3],
                state.host.port
            ),
            value_style: Style::default().gray(),
        };
        host.render(area.offset(Offset { x: 1, y: 2 }), buf);

        let mut ssl = StyledLabelledText {
            label: "SSL Mode".into(),
            value: match state.host.ssl {
                Ssl::Disabled => "Disabled".into(),
            },

            label_style: Style::default().green(),
            value_style: Style::default().gray(),
        };

        ssl.render(area.offset(Offset { x: 1, y: 3 }), buf);
    }
}

struct StyledLabelledText {
    label: String,
    label_style: Style,

    value: String,
    value_style: Style,
}

impl StyledLabelledText {
    const LABEL_DELIM: &'static str = ": ";

    fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let mut host_label = Text::default();
        host_label
            .push_line(Line::from(self.label.clone() + Self::LABEL_DELIM).style(self.label_style));
        host_label.render(area.offset(Offset { x: 0, y: 0 }), buf);

        let mut text = Text::default();
        text.push_line(Line::from(self.value.clone()).style(self.value_style));
        text.render(
            area.offset(Offset {
                x: self.label.len() as i32 + Self::LABEL_DELIM.len() as i32,
                y: 0,
            }),
            buf,
        );
    }
}
