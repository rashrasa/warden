use std::net::SocketAddr;

use ratatui::{
    layout::Offset,
    style::Style,
    widgets::{Block, Borders, StatefulWidget, Widget},
};

use crate::components::text::StyledLabelledText;

pub enum Status {
    Healthy,
    Unhealthy,
}

pub enum Ssl {
    Disabled,
}

pub struct Host {
    pub host: SocketAddr,
    pub ssl: Ssl,
}

pub struct HomePageState {
    pub host: Host,
    pub status: Status,
    pub lifetime_connections: usize,
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
            label_style: Style::default().gray(),
            value_style: Style::default().gray(),
        };
        status.render(area.offset(Offset { x: 1, y: 1 }), buf);

        let mut host = StyledLabelledText {
            label: "Host".into(),
            value: state.host.host.to_string(),

            label_style: Style::default().gray(),
            value_style: Style::default().gray(),
        };
        host.render(area.offset(Offset { x: 1, y: 2 }), buf);

        let mut ssl = StyledLabelledText {
            label: "SSL Mode".into(),
            value: match state.host.ssl {
                Ssl::Disabled => "Disabled".into(),
            },

            label_style: Style::default().gray(),
            value_style: Style::default().gray(),
        };

        ssl.render(area.offset(Offset { x: 1, y: 3 }), buf);

        let mut connections = StyledLabelledText {
            label: "Lifetime Connections".into(),
            value: format!("{}", state.lifetime_connections),

            label_style: Style::default().gray(),
            value_style: Style::default().green(),
        };
        connections.render(area.offset(Offset { x: 1, y: 4 }), buf);
    }
}
