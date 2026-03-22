use iced::{
    mouse,
    widget::{button, column, container, mouse_area, row, text, Space},
    Alignment, Border, Color, Element, Length, Padding,
};

use crate::{
    app::state::{ConnectedDevice, Message},
    models::ModelInfo,
    ui::{app_font, btn_style_active, btn_style_default, BORDER_GREY, GREY, PURE_WHITE},
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
    message: Message,
) -> Element<'static, Message> {
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
    } else {
        button.style(btn_style_default).into()
    }
}

pub fn supports_custom_eq(model: &ModelInfo) -> bool {
    model.base != "B181"
}

fn supports_advanced_eq(model: &ModelInfo) -> bool {
    matches!(model.base.as_str(), "B157" | "B155" | "B171" | "B174")
}

fn eq_presets(model: &ModelInfo) -> &'static [(u8, &'static str)] {
    match model.base.as_str() {
        "B172" | "B168" => &[
            (0, "dirac opteo"),
            (3, "pop"),
            (1, "rock"),
            (5, "classical"),
            (2, "electronic"),
            (4, "enhance vocals"),
            (6, "custom"),
        ],
        "B155" | "B157" | "B171" | "B174" => &[
            (0, "balanced"),
            (3, "more bass"),
            (2, "more treble"),
            (1, "voice"),
            (5, "custom"),
            (6, "advanced"),
        ],
        _ => &[
            (0, "balanced"),
            (3, "more bass"),
            (2, "more treble"),
            (1, "voice"),
            (5, "custom"),
        ],
    }
}

fn eq_button_active(
    model: &ModelInfo,
    preset: u8,
    eq_preset: u8,
    advanced_eq_enabled: bool,
) -> bool {
    if supports_advanced_eq(model) && preset == 6 {
        advanced_eq_enabled
    } else {
        !advanced_eq_enabled && preset == eq_preset
    }
}

fn custom_eq_active(model: &ModelInfo, eq_preset: u8, advanced_eq_enabled: bool) -> bool {
    if matches!(model.base.as_str(), "B172" | "B168") {
        eq_preset == 6
    } else {
        eq_preset == 5 && !advanced_eq_enabled
    }
}

fn band_editor(label: &'static str, value: f32, band_index: usize) -> Element<'static, Message> {
    let meter = (0..=12).rev().fold(
        column![].spacing(4).align_x(Alignment::Center),
        |column, level| {
            let step_value = level as i8 - 6;
            let is_center = step_value == 0;
            let current_level = value as i8;
            let is_active = if current_level > 0 {
                step_value > 0 && step_value <= current_level
            } else if current_level < 0 {
                step_value < 0 && step_value >= current_level
            } else {
                false
            };

            column.push(
                mouse_area(
                    container(text(""))
                        .width(24)
                        .height(6)
                        .style(move |_theme| container::Style {
                            background: Some(
                                if is_active {
                                    PURE_WHITE
                                } else if is_center {
                                    Color::from_rgb(0.18, 0.18, 0.18)
                                } else {
                                    Color::from_rgb(0.08, 0.08, 0.08)
                                }
                                .into(),
                            ),
                            border: Border {
                                color: if is_center { GREY } else { BORDER_GREY },
                                width: 1.0,
                                radius: 0.0.into(),
                            },
                            ..Default::default()
                        }),
                )
                .interaction(mouse::Interaction::Pointer)
                .on_press(Message::SetCustomEQLevel(band_index, step_value)),
            )
        },
    );

    mouse_area(
        container(
            column![
                text(label).font(app_font()).size(12).color(GREY),
                text(format!("{:+.0} dB", value)).font(app_font()).size(16),
                meter,
                row![
                    button(
                        text("-")
                            .font(app_font())
                            .size(16)
                            .align_x(Alignment::Center)
                    )
                    .on_press(Message::DecCustomEQ(band_index))
                    .width(Length::Fill)
                    .padding(8)
                    .style(btn_style_default),
                    button(
                        text("+")
                            .font(app_font())
                            .size(16)
                            .align_x(Alignment::Center)
                    )
                    .on_press(Message::IncCustomEQ(band_index))
                    .width(Length::Fill)
                    .padding(8)
                    .style(btn_style_default),
                ]
                .spacing(8)
                .width(Length::Fill),
            ]
            .spacing(12)
            .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(Padding {
            top: 14.0,
            right: 12.0,
            bottom: 12.0,
            left: 12.0,
        }),
    )
    .interaction(mouse::Interaction::Pointer)
    .on_scroll(move |delta| {
        let step = match delta {
            mouse::ScrollDelta::Lines { y, .. } => {
                if y > 0.0 {
                    1
                } else if y < 0.0 {
                    -1
                } else {
                    0
                }
            }
            mouse::ScrollDelta::Pixels { y, .. } => {
                if y > 0.0 {
                    1
                } else if y < 0.0 {
                    -1
                } else {
                    0
                }
            }
        };

        Message::ScrollCustomEQ(band_index, step)
    })
    .into()
}

fn custom_eq_view(model: &ModelInfo, device: &ConnectedDevice) -> Element<'static, Message> {
    let is_active = custom_eq_active(model, device.eq_preset, device.advanced_eq_enabled);

    column![
        section_title("custom eq"),
        container(
            column![row![
                band_editor("low", device.custom_eq[0], 0),
                band_editor("mid", device.custom_eq[1], 1),
                band_editor("high", device.custom_eq[2], 2),
            ]
            .spacing(8)
            .width(Length::Fill)]
            .spacing(16),
        )
        .padding(16)
        .width(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(Color::from_rgb(0.03, 0.03, 0.03).into()),
            border: Border {
                color: if is_active { PURE_WHITE } else { BORDER_GREY },
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        }),
    ]
    .spacing(8)
    .into()
}

fn advanced_eq_view() -> Element<'static, Message> {
    column![
        section_title("advanced eq"),
        container(
            column![
                text("advanced eq is enabled").font(app_font()).size(14),
                text("select another preset to leave advanced mode")
                    .font(app_font())
                    .size(12)
                    .color(GREY),
            ]
            .spacing(6),
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(Color::from_rgb(0.03, 0.03, 0.03).into()),
            border: Border {
                color: PURE_WHITE,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        }),
    ]
    .spacing(8)
    .into()
}

pub fn view(model: &ModelInfo, device: &ConnectedDevice) -> Element<'static, Message> {
    let presets = eq_presets(model);

    let preset_rows = presets
        .chunks(2)
        .fold(column![].spacing(8), |column, chunk| {
            let mut current_row = row![].spacing(8).width(Length::Fill);

            for (preset, label) in chunk {
                let message = if supports_advanced_eq(model) && *preset == 6 {
                    Message::ToggleAdvancedEQ(true)
                } else {
                    Message::SetEQ(*preset)
                };

                current_row = current_row.push(action_button(
                    label,
                    eq_button_active(model, *preset, device.eq_preset, device.advanced_eq_enabled),
                    message,
                ));
            }

            if chunk.len() == 1 {
                current_row = current_row.push(Space::new().width(Length::Fill));
            }

            column.push(current_row)
        });

    let mut content = column![section_title("equalizer"), preset_rows].spacing(8);

    if supports_custom_eq(model)
        && custom_eq_active(model, device.eq_preset, device.advanced_eq_enabled)
    {
        content = content.push(custom_eq_view(model, device));
    }

    if device.advanced_eq_enabled {
        content = content.push(advanced_eq_view());
    }

    content.into()
}
