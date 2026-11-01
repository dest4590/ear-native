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

    pub fn get_model_or_default(&self, key: &str) -> ModelInfo {
        self.models
            .get(key)
            .or_else(|| self.models.get("ear_1_black"))
            .or_else(|| self.models.values().next())
            .cloned()
            .unwrap_or_else(|| ModelInfo {
                name: "Unknown Ear".to_string(),
                base: "B181".to_string(),
                left_img: String::new(),
                case_img: String::new(),
                right_img: String::new(),
                duo_img: String::new(),
                is_anc: false,
            })
    }

    pub fn sort_discovered_devices(&mut self) {
        let models = &self.models;
        self.discovered_devices
            .sort_by(|(addr_a, name_a), (addr_b, name_b)| {
                let name_a_lower = name_a.to_lowercase();
                let name_b_lower = name_b.to_lowercase();
                let matched_a = models
                    .iter()
                    .find(|(_, model)| name_a_lower.contains(&model.name.to_lowercase()))
                    .map(|(_, model)| model.name.as_str());
                let matched_b = models
                    .iter()
                    .find(|(_, model)| name_b_lower.contains(&model.name.to_lowercase()))
                    .map(|(_, model)| model.name.as_str());
                let model_a = matched_a.unwrap_or(name_a.as_str()).to_lowercase();
                let model_b = matched_b.unwrap_or(name_b.as_str()).to_lowercase();

                matched_a
                    .is_none()
                    .cmp(&matched_b.is_none())
                    .then_with(|| model_a.cmp(&model_b))
                    .then_with(|| name_a_lower.cmp(&name_b_lower))
                    .then_with(|| addr_a.cmp(addr_b))
            });
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

    pub fn ring_buds_payloads(model: &ModelInfo, target: RingTarget, enabled: bool) -> Vec<Vec<u8>> {
        let enable_byte = if enabled { 0x01 } else { 0x00 };
        if model.base == "B181" {
            vec![vec![enable_byte]]
        } else {
            match target {
                RingTarget::Left => vec![vec![DeviceId::Left as u8, enable_byte]],
                RingTarget::Right => vec![vec![DeviceId::Right as u8, enable_byte]],
                RingTarget::Both => vec![
                    vec![DeviceId::Left as u8, enable_byte],
                    vec![DeviceId::Right as u8, enable_byte],
                ],
            }
        }
    }

    pub fn ring_buds_payload(model: &ModelInfo, target: RingTarget, enabled: bool) -> Vec<u8> {
        Self::ring_buds_payloads(model, target, enabled)
            .into_iter()
            .next()
            .unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_model(base: &str) -> ModelInfo {
        ModelInfo {
            name: "Test Ear".to_string(),
            base: base.to_string(),
            left_img: "left.png".to_string(),
            case_img: "case.png".to_string(),
            right_img: "right.png".to_string(),
            duo_img: "duo.png".to_string(),
            is_anc: true,
        }
    }

    #[test]
    fn ring_buds_payloads_b181() {
        let model = dummy_model("B181");
        let start = EarNative::ring_buds_payloads(&model, RingTarget::Both, true);
        assert_eq!(start, vec![vec![0x01]]);

        let stop = EarNative::ring_buds_payloads(&model, RingTarget::Both, false);
        assert_eq!(stop, vec![vec![0x00]]);
    }

    #[test]
    fn ring_buds_payloads_non_b181() {
        let model = dummy_model("B155");

        let left = EarNative::ring_buds_payloads(&model, RingTarget::Left, true);
        assert_eq!(left, vec![vec![DeviceId::Left as u8, 0x01]]);

        let right = EarNative::ring_buds_payloads(&model, RingTarget::Right, false);
        assert_eq!(right, vec![vec![DeviceId::Right as u8, 0x00]]);

        let both = EarNative::ring_buds_payloads(&model, RingTarget::Both, true);
        assert_eq!(
            both,
            vec![
                vec![DeviceId::Left as u8, 0x01],
                vec![DeviceId::Right as u8, 0x01],
            ]
        );
    }

    #[test]
    fn set_ringing_state_updates_flags() {
        let mut device = ConnectedDevice {
            id: "test".to_string(),
            model: dummy_model("B155"),
            battery_left: None,
            battery_right: None,
            battery_case: None,
            sku_attempts: 0,
            anc_status: 1,
            eq_mode: EqMode::Balanced,
            eq_preset: 0,
            advanced_eq_enabled: false,
            bass_level: 0,
            bass_enhance_enabled: false,
            custom_eq: [0.0; 3],
            in_ear_enabled: false,
            latency_low: false,
            personalized_anc_enabled: false,
            firmware_version: "1.0".to_string(),
            ear_tip_left: None,
            ear_tip_right: None,
            ear_tip_test_running: false,
            ringing_left: false,
            ringing_right: false,
        };

        EarNative::set_ringing_state(&mut device, RingTarget::Both, true);
        assert!(device.ringing_left);
        assert!(device.ringing_right);

        EarNative::set_ringing_state(&mut device, RingTarget::Left, false);
        assert!(!device.ringing_left);
        assert!(device.ringing_right);
    }
}
