use iced::{
    widget::{column, container, row, text},
    Alignment, Border, Element, Length,
};

use crate::{
    app::state::{ConnectedDevice, Message},
    ui::{app_font, BORDER_GREY, GREY},
};

fn pill(label: &'static str, value: Option<u8>) -> Element<'static, Message> {
    container(
        column![
            text(label).font(app_font()).size(12).color(GREY),
            text(format!("{}%", value.unwrap_or(0)))
                .font(app_font())
                .size(22),
        ]
        .align_x(Alignment::Center)
        .spacing(4),
    )
    .width(Length::Fill)
    .padding(12)
    .style(|_theme| container::Style {
        border: Border {
            color: BORDER_GREY,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    })
    .into()
}

pub fn view(device: &ConnectedDevice) -> Element<'static, Message> {
    row![
        pill("left", device.battery_left),
        pill("case", device.battery_case),
        pill("right", device.battery_right),
    ]
    .spacing(8)
    .width(Length::Fill)
    .into()
}
