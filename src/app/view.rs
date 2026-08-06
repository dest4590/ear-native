use iced::{
    widget::{button, column, container, image, row, scrollable, text},
    Alignment, Border, Color, Element, Length, Padding,
};

use crate::{
    app::state::{AppState, Message, PendingConfirmation, RingTarget},
    components::{anc, battery, equalizer},
    models::embedded_image_handle,
    ui::{
        app_font, btn_style_active, btn_style_default, btn_style_red, BORDER_GREY, GREY,
        PURE_BLACK, PURE_WHITE,
    },
    EarNative,
};

impl EarNative {
    pub fn view(&self) -> Element<'_, Message> {
        let content: Element<'_, Message> = match &self.state {
            AppState::Disconnected => {
                let header = column![
                    text("ear (native)").font(app_font()).size(36),
                    text("status: disconnected")
                        .font(app_font())
                        .size(14)
                        .color(GREY),
                ]
                .spacing(12)
                .align_x(Alignment::Center);

                let mut list = column![].spacing(12).align_x(Alignment::Center);

                if self.discovered_devices.is_empty() {
                    list = list.push(
                        text("searching for devices...")
                            .font(app_font())
                            .size(14)
                            .color(GREY),
                    );
                } else {
                    let mut devices: Vec<_> = self.discovered_devices.iter().collect();
                    devices.sort_by(|(addr_a, name_a), (addr_b, name_b)| {
                        let matched_a = self.matched_model_name(name_a);
                        let matched_b = self.matched_model_name(name_b);
                        let model_a = matched_a.unwrap_or(name_a.as_str()).to_lowercase();
                        let model_b = matched_b.unwrap_or(name_b.as_str()).to_lowercase();
                        let raw_a = name_a.to_lowercase();
                        let raw_b = name_b.to_lowercase();

                        matched_a
                            .is_none()
                            .cmp(&matched_b.is_none())
                            .then_with(|| model_a.cmp(&model_b))
                            .then_with(|| raw_a.cmp(&raw_b))
                            .then_with(|| addr_a.cmp(addr_b))
                    });

                    for (addr, name) in devices {
                        list = list.push(
                            button(
                                text(name.to_lowercase())
                                    .font(app_font())
                                    .size(14)
                                    .width(Length::Fill)
                                    .align_x(Alignment::Center),
                            )
                            .on_press(Message::Connect(addr.clone()))
                            .width(300)
                            .padding(16)
                            .style(btn_style_default),
                        );
                    }
                }

                column![header, list]
                    .spacing(48)
                    .align_x(Alignment::Center)
                    .padding(Padding {
                        top: 80.0,
                        right: 0.0,
                        bottom: 0.0,
                        left: 0.0,
                    })
                    .into()
            }
            AppState::Connecting(name) | AppState::Identifying(name) => self.loading_view(
                match &self.state {
                    AppState::Connecting(_) => "connecting to headphones",
                    AppState::Identifying(_) => "loading headphone data",
                    _ => "loading headphone data",
                },
                name.to_lowercase(),
            ),
            AppState::Error(msg) => container(
                column![
                    text("!")
                        .font(app_font())
                        .size(56)
                        .color(Color::from_rgb(1.0, 0.3, 0.3)),
                    text("connection error").font(app_font()).size(22),
                    text(msg.to_lowercase())
                        .font(app_font())
                        .size(14)
                        .color(GREY),
                    button(
                        text("back to menu")
                            .font(app_font())
                            .size(14)
                            .width(Length::Fill)
                            .align_x(Alignment::Center),
                    )
                    .on_press(Message::Disconnect)
                    .width(220)
                    .padding(12)
                    .style(btn_style_default),
                ]
                .spacing(16)
                .align_x(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(PURE_BLACK.into()),
                text_color: Some(PURE_WHITE),
                ..Default::default()
            })
            .into(),
            AppState::Connected(model) => {
                if self.initial_data_load.is_some() || !self.active_model_assets_ready {
                    self.loading_view("loading headphone data", model.name.to_lowercase())
                } else if let Some(device) = &self.connected_device {
                    let device_images: Element<'_, Message> = if !model.duo_img.is_empty() {
                        row![container(
                            image::<image::Handle>(embedded_image_handle(&model.duo_img),)
                                .width(260)
                                .filter_method(image::FilterMethod::Linear)
                        )]
                        .align_y(Alignment::Center)
                        .into()
                    } else {
                        row![
                            container(
                                image::<image::Handle>(embedded_image_handle(&model.left_img),)
                                    .width(90)
                                    .filter_method(image::FilterMethod::Linear)
                            ),
                            container(
                                image::<image::Handle>(embedded_image_handle(&model.case_img),)
                                    .width(90)
                                    .filter_method(image::FilterMethod::Linear)
                            ),
                            container(
                                image::<image::Handle>(embedded_image_handle(&model.right_img),)
                                    .width(90)
                                    .filter_method(image::FilterMethod::Linear)
                            ),
                        ]
                        .spacing(16)
                        .align_y(Alignment::Center)
                        .into()
                    };

                    let battery_info = battery::view(device);

                    let section_title = |t: &str| {
                        container(text(t.to_lowercase()).font(app_font()).size(12).color(GREY))
                            .padding(Padding {
                                top: 0.0,
                                right: 0.0,
                                bottom: 4.0,
                                left: 0.0,
                            })
                    };

                    let make_btn = |label: &str, is_active: bool, msg: Message| {
                        let t = text(label.to_lowercase())
                            .font(app_font())
                            .size(14)
                            .width(Length::Fill)
                            .align_x(Alignment::Center);

                        let b = button(t).on_press(msg).width(Length::Fill).padding(12);

                        if is_active {
                            b.style(btn_style_active)
                        } else {
                            b.style(btn_style_default)
                        }
                    };

                    let anc_ui = anc::view(model, device);
                    let eq_ui = equalizer::view(model, device);

                    let ultra_bass_ui = if EarNative::supports_ultra_bass(&device.model) {
                        column![
                            section_title("ultra bass"),
                            row![make_btn(
                                if device.bass_enhance_enabled {
                                    "enabled"
                                } else {
                                    "disabled"
                                },
                                device.bass_enhance_enabled,
                                Message::ToggleBassEnhance(!device.bass_enhance_enabled),
                            ),]
                            .spacing(8)
                            .width(Length::Fill),
                            if device.bass_enhance_enabled {
                                row![
                                    make_btn("1", device.bass_level == 1, Message::SetBassLevel(1)),
                                    make_btn("2", device.bass_level == 2, Message::SetBassLevel(2)),
                                    make_btn("3", device.bass_level == 3, Message::SetBassLevel(3)),
                                    make_btn("4", device.bass_level == 4, Message::SetBassLevel(4)),
                                    make_btn("5", device.bass_level == 5, Message::SetBassLevel(5)),
                                ]
                                .spacing(8)
                                .width(Length::Fill)
                            } else {
                                row![]
                            },
                        ]
                        .spacing(8)
                    } else {
                        column![]
                    };

                    let ring_ui = if EarNative::supports_split_ring(model) {
                        column![
                            section_title("find my earbuds"),
                            row![
                                if device.ringing_left {
                                    make_btn("stop left", true, Message::StopRing(RingTarget::Left))
                                } else {
                                    make_btn(
                                        "ring left",
                                        false,
                                        Message::RequestRing(RingTarget::Left),
                                    )
                                },
                                if device.ringing_right {
                                    make_btn(
                                        "stop right",
                                        true,
                                        Message::StopRing(RingTarget::Right),
                                    )
                                } else {
                                    make_btn(
                                        "ring right",
                                        false,
                                        Message::RequestRing(RingTarget::Right),
                                    )
                                },
                            ]
                            .spacing(8),
                        ]
                        .spacing(8)
                    } else {
                        column![
                            section_title("find my earbuds"),
                            row![if device.ringing_left || device.ringing_right {
                                make_btn("stop ringing", true, Message::StopRing(RingTarget::Both))
                            } else {
                                make_btn(
                                    "ring earbuds",
                                    false,
                                    Message::RequestRing(RingTarget::Both),
                                )
                            }],
                        ]
                        .spacing(8)
                    };

                    let confirmation_ui = if let Some(PendingConfirmation::StartRing(target)) =
                        self.pending_confirmation
                    {
                        container(
                            column![
                                text(EarNative::confirm_message(target))
                                    .font(app_font())
                                    .size(12)
                                    .color(GREY),
                                row![
                                    make_btn("confirm", false, Message::ConfirmPendingAction),
                                    make_btn("cancel", false, Message::CancelPendingAction),
                                ]
                                .spacing(8),
                            ]
                            .spacing(10),
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
                        })
                    } else {
                        container(column![])
                    };

                    let ear_tip_ui = if EarNative::supports_ear_tip_test(model) {
                        let tip_card = |label: &'static str, value: Option<u8>| {
                            container(
                                column![
                                    text(label).font(app_font()).size(12).color(GREY),
                                    text(EarNative::ear_tip_status_label(value))
                                        .font(app_font())
                                        .size(14)
                                        .color(EarNative::ear_tip_status_color(value)),
                                ]
                                .spacing(4),
                            )
                            .padding(12)
                            .width(Length::Fill)
                            .style(move |_theme| container::Style {
                                background: Some(Color::from_rgb(0.03, 0.03, 0.03).into()),
                                border: Border {
                                    color: BORDER_GREY,
                                    width: 1.0,
                                    radius: 0.0.into(),
                                },
                                ..Default::default()
                            })
                        };

                        column![
                            section_title("ear tip test"),
                            container(
                                column![
                                    text(EarNative::ear_tip_summary(
                                        device.ear_tip_left,
                                        device.ear_tip_right,
                                        device.ear_tip_test_running,
                                    ))
                                    .font(app_font())
                                    .size(12)
                                    .color(GREY),
                                    row![
                                        tip_card("left", device.ear_tip_left),
                                        tip_card("right", device.ear_tip_right),
                                    ]
                                    .spacing(8),
                                    row![
                                        make_btn(
                                            if device.ear_tip_test_running {
                                                "testing..."
                                            } else if device.ear_tip_left.is_some()
                                                || device.ear_tip_right.is_some()
                                            {
                                                "run again"
                                            } else {
                                                "start test"
                                            },
                                            device.ear_tip_test_running,
                                            Message::StartEarTipTest,
                                        ),
                                        make_btn("clear", false, Message::ResetEarTipTest),
                                    ]
                                    .spacing(8),
                                ]
                                .spacing(12),
                            )
                            .padding(16)
                            .width(Length::Fill)
                            .style(|_theme| container::Style {
                                background: Some(Color::from_rgb(0.03, 0.03, 0.03).into()),
                                border: Border {
                                    color: BORDER_GREY,
                                    width: 1.0,
                                    radius: 0.0.into(),
                                },
                                ..Default::default()
                            }),
                        ]
                        .spacing(8)
                    } else {
                        column![]
                    };

                    let settings_ui = column![
                        section_title("settings"),
                        make_btn(
                            if device.in_ear_enabled {
                                "in-ear detect [ on ]"
                            } else {
                                "in-ear detect [ off ]"
                            },
                            device.in_ear_enabled,
                            Message::ToggleInEar(!device.in_ear_enabled)
                        ),
                        make_btn(
                            if device.latency_low {
                                "low latency [ on ]"
                            } else {
                                "low latency [ off ]"
                            },
                            device.latency_low,
                            Message::ToggleLatency(!device.latency_low)
                        ),
                        container(
                            row![
                                text("firmware:").font(app_font()).size(12).color(GREY),
                                text(&device.firmware_version).font(app_font()).size(12)
                            ]
                            .spacing(8)
                        )
                        .padding(Padding {
                            top: 8.0,
                            right: 0.0,
                            bottom: 0.0,
                            left: 0.0
                        }),
                    ]
                    .spacing(8);

                    let disconnect_btn = button(
                        text("disconnect")
                            .font(app_font())
                            .size(14)
                            .width(Length::Fill)
                            .align_x(Alignment::Center),
                    )
                    .on_press(Message::Disconnect)
                    .width(Length::Fill)
                    .padding(12)
                    .style(btn_style_red);

                    scrollable(
                        column![
                            text(model.name.to_lowercase())
                                .font(app_font())
                                .size(28)
                                .width(Length::Fill)
                                .align_x(Alignment::Center),
                            device_images,
                            battery_info,
                            if model.is_anc {
                                anc_ui
                            } else {
                                column![].into()
                            },
                            eq_ui,
                            ultra_bass_ui,
                            ring_ui,
                            confirmation_ui,
                            ear_tip_ui,
                            settings_ui,
                            disconnect_btn,
                        ]
                        .spacing(32)
                        .align_x(Alignment::Center)
                        .width(Length::Fill)
                        .padding(24),
                    )
                    .into()
                } else {
                    column![text("error: device sync lost")
                        .font(app_font())
                        .size(14)
                        .color(Color::from_rgb(0.9, 0.2, 0.2))]
                    .align_x(Alignment::Center)
                    .into()
                }
            }
        };

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(PURE_BLACK.into()),
                text_color: Some(PURE_WHITE),
                ..Default::default()
            })
            .into()
    }

    pub fn loading_view(
        &self,
        title: impl Into<String>,
        subtitle: impl Into<String>,
    ) -> Element<'static, Message> {
        let frames = ["|", "/", "-", "\\"];
        let spinner = frames[self.loading_frame % frames.len()];
        let title = title.into();
        let subtitle = subtitle.into();

        container(
            column![
                text(spinner).font(app_font()).size(56),
                text(title).font(app_font()).size(22),
                text(subtitle).font(app_font()).size(14).color(GREY),
            ]
            .spacing(16)
            .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(PURE_BLACK.into()),
            text_color: Some(PURE_WHITE),
            ..Default::default()
        })
        .into()
    }
}
