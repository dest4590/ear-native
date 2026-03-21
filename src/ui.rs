use iced::{widget::button, Border, Color, Shadow, Theme};

pub const PURE_BLACK: Color = Color::from_rgb(0.0, 0.0, 0.0);
pub const PURE_WHITE: Color = Color::from_rgb(1.0, 1.0, 1.0);
pub const GREY: Color = Color::from_rgb(0.4, 0.4, 0.4);
pub const BORDER_GREY: Color = Color::from_rgb(0.15, 0.15, 0.15);

pub fn btn_style_default(_theme: &Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: Some(Color::TRANSPARENT.into()),
        text_color: PURE_WHITE,
        border: Border {
            color: BORDER_GREY,
            width: 1.0,
            radius: 0.0.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    };

    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(Color::from_rgb(0.1, 0.1, 0.1).into()),
            border: Border {
                color: PURE_WHITE,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..base
        },
        _ => base,
    }
}

pub fn btn_style_active(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(PURE_WHITE.into()),
        text_color: PURE_BLACK,
        border: Border {
            color: PURE_WHITE,
            width: 1.0,
            radius: 0.0.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

pub fn btn_style_red(_theme: &Theme, status: button::Status) -> button::Style {
    let red = Color::from_rgb(0.9, 0.2, 0.2);
    let dark_red = Color::from_rgb(0.2, 0.05, 0.05);
    let base = button::Style {
        background: Some(Color::TRANSPARENT.into()),
        text_color: red,
        border: Border {
            color: red,
            width: 1.0,
            radius: 0.0.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    };

    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(dark_red.into()),
            ..base
        },
        _ => base,
    }
}
