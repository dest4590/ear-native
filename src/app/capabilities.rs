use crate::{
    app::state::{ConnectedDevice, RingTarget},
    models::ModelInfo,
    protocol::{DeviceId, EqMode, PacketCommand},
    ui::GREY,
    EarNative,
};

use iced::Color;

impl EarNative {
    pub fn eq_command_for_model(model: &ModelInfo) -> PacketCommand {
        if model.base == "B172" || model.base == "B168" {
            PacketCommand::SetListeningMode
        } else {
            PacketCommand::SetEq
        }
    }

    pub fn eq_mode_from_raw(value: u8) -> EqMode {
        EqMode::from_u8(value).unwrap_or(EqMode::Balanced)
    }

    pub fn supports_ultra_bass(model: &ModelInfo) -> bool {
        matches!(model.base.as_str(), "B171" | "B172" | "B168" | "B162")
    }

    pub fn supports_advanced_eq(model: &ModelInfo) -> bool {
        matches!(model.base.as_str(), "B157" | "B155" | "B171" | "B174")
    }

    pub fn supports_personalized_anc(model: &ModelInfo) -> bool {
        model.base == "B155"
    }

    pub fn supports_ear_tip_test(model: &ModelInfo) -> bool {
        matches!(model.base.as_str(), "B155" | "B171" | "B172" | "B162")
    }

    pub fn supports_split_ring(model: &ModelInfo) -> bool {
        model.base != "B181" && !model.left_img.is_empty() && !model.right_img.is_empty()
    }

    pub fn supports_custom_eq(model: &ModelInfo) -> bool {
        crate::components::equalizer::supports_custom_eq(model)
    }

    pub fn matched_model_key(&self, device_name: &str) -> Option<&str> {
        let device_name = device_name.to_lowercase();
        self.models
            .iter()
            .find(|(_, model)| device_name.contains(&model.name.to_lowercase()))
            .map(|(key, _)| key.as_str())
    }

    pub fn matched_model_name<'a>(&'a self, device_name: &str) -> Option<&'a str> {
        self.matched_model_key(device_name)
            .and_then(|key| self.models.get(key))
            .map(|model| model.name.as_str())
    }

    pub fn inferred_model_key(&self, device_name: &str) -> String {
        self.matched_model_key(device_name)
            .unwrap_or("ear_1_black")
            .to_string()
    }

    pub fn ring_buds_payload(model: &ModelInfo, target: RingTarget, enabled: bool) -> Vec<u8> {
        if model.base == "B181" {
            vec![if enabled { 0x01 } else { 0x00 }]
        } else {
            vec![
                match target {
                    RingTarget::Left => DeviceId::Left as u8,
                    RingTarget::Right => DeviceId::Right as u8,
                    RingTarget::Both => DeviceId::Right as u8,
                },
                if enabled { 0x01 } else { 0x00 },
            ]
        }
    }

    pub fn set_ringing_state(device: &mut ConnectedDevice, target: RingTarget, enabled: bool) {
        match target {
            RingTarget::Left => device.ringing_left = enabled,
            RingTarget::Right => device.ringing_right = enabled,
            RingTarget::Both => {
                device.ringing_left = enabled;
                device.ringing_right = enabled;
            }
        }
    }

    pub fn confirm_message(target: RingTarget) -> &'static str {
        match target {
            RingTarget::Left => "start ringing the left earbud?",
            RingTarget::Right => "start ringing the right earbud?",
            RingTarget::Both => "start ringing both earbuds?",
        }
    }

    pub fn ear_tip_status_label(status: Option<u8>) -> &'static str {
        match status {
            Some(0) => "good fit",
            Some(1) => "adjust fit",
            Some(2) => "not detected",
            _ => "not tested",
        }
    }

    pub fn ear_tip_status_color(status: Option<u8>) -> Color {
        match status {
            Some(0) => Color::from_rgb(0.11, 0.69, 0.35),
            Some(1) => Color::from_rgb(0.95, 0.78, 0.12),
            Some(2) => Color::from_rgb(0.79, 0.13, 0.18),
            _ => GREY,
        }
    }

    pub fn ear_tip_summary(left: Option<u8>, right: Option<u8>, running: bool) -> &'static str {
        if running {
            "testing fit. keep both earbuds in your ears."
        } else {
            match (left, right) {
                (Some(0), Some(0)) => "perfect fit on both sides.",
                (Some(1), Some(1)) => "adjust both earbuds or try another tip size.",
                (Some(1), Some(0)) => "adjust the left earbud or try another tip size.",
                (Some(0), Some(1)) => "adjust the right earbud or try another tip size.",
                (Some(2), _) | (_, Some(2)) => {
                    "make sure both earbuds are connected and in your ears."
                }
                _ => "put both earbuds in your ears, then start the test.",
            }
        }
    }
}
