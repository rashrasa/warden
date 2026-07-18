use ratatui::{
    buffer::Buffer,
    layout::{Offset, Rect},
    style::Style,
    text::{Line, Text},
    widgets::Widget,
};

pub struct StyledLabelledText {
    pub label: String,
    pub label_style: Style,

    pub value: String,
    pub value_style: Style,
}

impl StyledLabelledText {
    const LABEL_DELIM: &'static str = ": ";

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
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
