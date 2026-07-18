use ratatui::{
    buffer::Buffer,
    layout::{Offset, Rect},
    style::Style,
    text::Line,
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
        let host_label = Line::from(self.label.clone() + Self::LABEL_DELIM).style(self.label_style);
        host_label.render(area.offset(Offset { x: 0, y: 0 }), buf);

        let text = Line::from(self.value.clone()).style(self.value_style);
        text.render(
            area.offset(Offset {
                x: self.label.len() as i32 + Self::LABEL_DELIM.len() as i32,
                y: 0,
            }),
            buf,
        );
    }
}
