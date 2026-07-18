use std::{net::SocketAddr, time::Duration};

use ratatui::{
    layout::Offset,
    style::Style,
    text::Line,
    widgets::{Block, Borders, StatefulWidget, Widget},
};
use warden_core::ConnectionInfo;

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
    pub uptime: Duration,
    pub active_connections: Vec<ConnectionInfo>,
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
        y += 2;

        for conn in &state.active_connections {
            let mut connections = StyledLabelledText {
                label: format!("{}", conn.host),
                value: if let Some(ua) = &conn.user_agent {
                    format!("{}", ua)
                } else {
                    "No additional info".into()
                },

                label_style: Style::default().yellow(),
                value_style: Style::default().gray(),
            };
            connections.render(area.offset(Offset { x: 1, y }), buf);
            y += 1;
        }
    }
}
