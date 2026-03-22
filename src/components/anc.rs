use iced::{
    widget::{button, column, container, row, text},
    Alignment, Element, Length, Padding,
};

use crate::{
    app::state::{ConnectedDevice, Message},
    models::ModelInfo,
    ui::{app_font, btn_style_active, btn_style_default, GREY},
};

fn section_title(label: &'static str) -> Element<'static, Message> {
    container(text(label).font(app_font()).size(12).color(GREY))
        .padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: 4.0,
            left: 0.0,
        })
        .into()
}

fn action_button(
    label: &'static str,
    is_active: bool,
    is_pending: bool,
    message: Message,
) -> Element<'static, Message> {
    let label = if is_pending {
        format!("{}...", label)
    } else {
        label.to_string()
    };

    let button = button(
        text(label)
            .font(app_font())
            .size(14)
            .width(Length::Fill)
            .align_x(Alignment::Center),
    )
    .on_press(message)
    .width(Length::Fill)
    .padding(12);

    if is_active {
        button.style(btn_style_active).into()
    } else if is_pending {
        button.style(btn_style_default).into()
    } else {
        button.style(btn_style_default).into()
    }
}

fn small_action_button(
    label: &'static str,
    is_active: bool,
    is_pending: bool,
    message: Message,
) -> Element<'static, Message> {
    let label = if is_pending {
        format!("{}...", label)
    } else {
        label.to_string()
    };

    let button = button(
        text(label)
            .font(app_font())
            .size(12)
            .width(Length::Fill)
            .align_x(Alignment::Center),
    )
    .on_press(message)
    .width(Length::Fill)
    .padding(10);

    if is_active {
        button.style(btn_style_active).into()
    } else if is_pending {
        button.style(btn_style_default).into()
    } else {
        button.style(btn_style_default).into()
    }
}

fn strength_options(model: &ModelInfo) -> &'static [(u8, &'static str)] {
    match model.base.as_str() {
        "B181" => &[(4, "high"), (3, "low")],
        "B163" => &[(4, "high"), (5, "mid"), (3, "low")],
        "B155" | "B171" | "B162" | "B172" => {
            &[(4, "high"), (5, "mid"), (3, "low"), (6, "adaptive")]
        }
        _ => &[],
    }
}

fn supports_personalized_anc(model: &ModelInfo) -> bool {
    model.base == "B155"
}

pub fn view(model: &ModelInfo, device: &ConnectedDevice) -> Element<'static, Message> {
    let pending_notice: Element<'static, Message> = container(text(""))
        .width(Length::Shrink)
        .height(Length::Shrink)
        .into();

    let strength_row: Element<'static, Message> = if device.anc_status >= 3 {
        strength_options(model)
            .iter()
            .fold(
                row![].spacing(8).width(Length::Fill),
                |row, (preset, label)| {
                    row.push(small_action_button(
                        label,
                        device.anc_status == *preset,
                        false,
                        Message::SetANC(*preset),
                    ))
                },
            )
            .into()
    } else {
        row![].into()
    };

    let personalized_row: Element<'static, Message> = if supports_personalized_anc(model) {
        row![action_button(
            if device.personalized_anc_enabled {
                "personalized anc [ on ]"
            } else {
                "personalized anc [ off ]"
            },
            device.personalized_anc_enabled,
            false,
            Message::SetPersonalizedANC(!device.personalized_anc_enabled),
        )]
        .into()
    } else {
        row![].into()
    };

    column![
        section_title("noise control"),
        row![
            action_button(
                "noise cancellation",
                device.anc_status >= 3,
                false,
                Message::SetANC(4),
            ),
            action_button(
                "transparent",
                device.anc_status == 2,
                false,
                Message::SetANC(2),
            ),
            action_button("off", device.anc_status == 1, false, Message::SetANC(1)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        pending_notice,
        strength_row,
        personalized_row,
    ]
    .spacing(8)
    .into()
}
