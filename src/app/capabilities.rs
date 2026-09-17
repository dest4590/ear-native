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
        if matches!(model.base.as_str(), "B172" | "B168" | "B185") {
            PacketCommand::SetListeningMode
        } else {
            PacketCommand::SetEq
        }
    }

    pub fn latency_payload(model: &ModelInfo, enabled: bool) -> Vec<u8> {
        vec![
            if enabled {
                0x01
            } else if model.base == "B185" {
                0x02
            } else {
                0x00
            },
            0x00,
        ]
    }

    pub fn eq_mode_from_raw(value: u8) -> EqMode {
        EqMode::from_u8(value).unwrap_or(EqMode::Balanced)
    }

    pub fn supports_ultra_bass(model: &ModelInfo) -> bool {
        matches!(
            model.base.as_str(),
            "B171" | "B172" | "B168" | "B162" | "B185"
        )
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

    pub fn supports_in_ear(model: &ModelInfo) -> bool {
        model.base != "B185"
    }

    pub fn supports_command(model: &ModelInfo, command: PacketCommand) -> bool {
        model.base != "B185"
            || !matches!(
                command,
                PacketCommand::ReadInEar
                    | PacketCommand::SetInEar
                    | PacketCommand::ReadAdvancedEq
                    | PacketCommand::SetAdvancedEqEnabled
                    | PacketCommand::ReadPersonalizedAnc
                    | PacketCommand::SetPersonalizedAnc
                    | PacketCommand::StartEarFitTest
            )
    }

    pub fn matched_model_key(&self, device_name: &str) -> Option<&str> {
        let device_name = device_name.trim().to_lowercase();
        self.models
            .iter()
            .filter_map(|(key, model)| {
                let name = model.name.to_lowercase();
                device_name
                    .contains(&name)
                    .then_some((key, name == device_name, name.len()))
            })
            .max_by(|(key_a, exact_a, len_a), (key_b, exact_b, len_b)| {
                exact_a
                    .cmp(exact_b)
                    .then_with(|| len_a.cmp(len_b))
                    .then_with(|| key_b.cmp(key_a))
            })
            .map(|(key, _, _)| key.as_str())
    }

    pub fn connection_model_key(&self, device_name: &str, id: &str) -> Option<String> {
        if let Some(key) = self.matched_model_key(device_name) {
            if self.models.get(key)?.base == "B185" {
                return Some(key.to_string());
            }
        }

        self.config
            .known_model_key(id)
            .filter(|key| self.models.contains_key(*key))
            .map(str::to_string)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::InitialDataLoad;

    #[test]
    fn name_matching_prefers_exact_and_longest_names_deterministically() {
        for _ in 0..32 {
            let app = EarNative::default();
            for (name, key) in [
                ("CMF Buds", "donphan_black"),
                ("CMF Buds 2a", "hoothoot_generic"),
                ("  cmf BUDS 2A  ", "hoothoot_generic"),
                ("My CMF Buds 2a - paired", "hoothoot_generic"),
                ("CMF Buds Pro", "corsola_black"),
                ("My CMF Buds Pro 2", "espeon_black"),
                ("Nothing Ear", "entei_black"),
                ("Nothing Ear (1)", "ear_1_black"),
                ("Nothing Ear (2)", "ear_2_black"),
                ("Nothing Ear (a)", "cleffa_black"),
                ("Nothing Ear (stick)", "ear_stick"),
                ("Nothing Ear (open)", "flaaffy_white"),
            ] {
                assert_eq!(app.matched_model_key(name), Some(key), "{name}");
            }
            assert_eq!(app.matched_model_key("unrelated headphones"), None);
        }
    }

    #[test]
    fn b185_has_only_the_confirmed_capabilities() {
        let app = EarNative::default();
        let model = &app.models["hoothoot_generic"];
        assert_eq!(model.base, "B185");
        assert!(model.is_anc);
        assert!(EarNative::supports_ultra_bass(model));
        assert!(EarNative::supports_custom_eq(model));
        assert!(EarNative::supports_split_ring(model));
        assert!(!EarNative::supports_in_ear(model));
        assert!(!EarNative::supports_advanced_eq(model));
        assert!(!EarNative::supports_personalized_anc(model));
        assert!(!EarNative::supports_ear_tip_test(model));
        assert_eq!(
            EarNative::eq_command_for_model(model),
            PacketCommand::SetListeningMode
        );
        for command in [
            PacketCommand::ReadInEar,
            PacketCommand::SetInEar,
            PacketCommand::ReadAdvancedEq,
            PacketCommand::SetAdvancedEqEnabled,
            PacketCommand::ReadPersonalizedAnc,
            PacketCommand::SetPersonalizedAnc,
            PacketCommand::StartEarFitTest,
        ] {
            assert!(!EarNative::supports_command(model, command));
            assert!(EarNative::supports_command(
                &app.models["donphan_black"],
                command
            ));
        }
        for command in [
            PacketCommand::ReadBattery,
            PacketCommand::ReadAnc,
            PacketCommand::SetAnc,
            PacketCommand::ReadListeningMode,
            PacketCommand::SetListeningMode,
            PacketCommand::ReadEnhancedBass,
            PacketCommand::SetEnhancedBass,
            PacketCommand::ReadCustomEq,
            PacketCommand::SetCustomEq,
            PacketCommand::ReadLatency,
            PacketCommand::SetLatency,
            PacketCommand::ReadFirmware,
            PacketCommand::SetRingBuds,
        ] {
            assert!(EarNative::supports_command(model, command));
        }
        assert_eq!(EarNative::latency_payload(model, false), vec![2, 0]);
        assert_eq!(EarNative::latency_payload(model, true), vec![1, 0]);
        assert_eq!(
            EarNative::latency_payload(&app.models["donphan_black"], false),
            vec![0, 0]
        );
    }

    #[test]
    fn b185_loading_does_not_wait_for_unsupported_responses() {
        let app = EarNative::default();
        let mut load = InitialDataLoad::for_model(&app.models["hoothoot_generic"]);
        assert!(load.in_ear);
        assert!(!load.require_personalized_anc);
        assert!(!load.is_complete());
        load.battery = true;
        load.anc = true;
        load.eq = true;
        load.latency = true;
        load.enhanced_bass = true;
        assert!(!load.is_complete());
        load.custom_eq = true;
        assert!(load.is_complete());
        assert!(!InitialDataLoad::for_model(&app.models["donphan_black"]).in_ear);
    }

    #[test]
    fn b185_does_not_invent_sku_or_color_metadata() {
        let app = EarNative::default();
        assert!(!app.sku_map.values().any(|key| key == "hoothoot_generic"));
        for value in ["70F8E3", "ED5412", "509CAE", "A10600109", "SH1009"] {
            assert!(!app.sku_map.contains_key(value));
            assert_eq!(app.matched_model_key(value), None);
        }
        let generic = &app.models["hoothoot_generic"];
        let reused = &app.models["donphan_black"];
        assert_eq!(generic.left_img, reused.left_img);
        assert_eq!(generic.case_img, reused.case_img);
        assert_eq!(generic.right_img, reused.right_img);
    }
}
