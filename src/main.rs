use futures::SinkExt;
use iced::{
    mouse, time,
    widget::{button, column, container, image, mouse_area, row, scrollable, text, Space},
    Alignment, Border, Color, Element, Font, Length, Padding, Subscription, Task, Theme,
};
use std::collections::HashMap;
use tokio::sync::mpsc;

mod bluetooth;
mod models;
mod protocol;
mod ui;

use bluetooth::{BluetoothEvent, BluetoothManager, ManagerCommand};
use models::{get_models, get_sku_map, DeviceId, EqMode, ModelInfo};
use protocol::{commands, Packet};
use ui::{
    btn_style_active, btn_style_default, btn_style_red, BORDER_GREY, GREY, PURE_BLACK, PURE_WHITE,
};

pub fn main() -> iced::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut settings = iced::window::Settings::default();
    settings.size = iced::Size::new(450.0, 700.0);

    iced::application(EarNative::boot, EarNative::update, EarNative::view)
        .theme(EarNative::theme)
        .subscription(EarNative::subscription)
        .window(settings)
        .run()
}

#[derive(Debug, Clone)]
pub enum Message {
    Bluetooth(BluetoothEvent),
    Connect(String),
    Disconnect,
    SendCustomEQ,
    IncCustomEQ(usize),
    DecCustomEQ(usize),
    ScrollCustomEQ(usize, i8),
    SetCustomEQLevel(usize, i8),
    LoadingTick,
    InitialDataLoadTimedOut,
    SetANC(u8),
    SetEQ(u8),
    ToggleAdvancedEQ(bool),
    SetBassLevel(u8),
    ToggleBassEnhance(bool),
    RequestRing(RingTarget),
    StopRing(RingTarget),
    ConfirmPendingAction,
    CancelPendingAction,
    SetPersonalizedANC(bool),
    StartEarTipTest,
    ResetEarTipTest,
    ToggleInEar(bool),
    ToggleLatency(bool),
    CommandSent(Packet),
    Ready(mpsc::Sender<ManagerCommand>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingTarget {
    Left,
    Right,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingConfirmation {
    StartRing(RingTarget),
}

struct EarNative {
    models: HashMap<String, ModelInfo>,
    sku_map: HashMap<String, String>,
    state: AppState,
    discovered_devices: Vec<(String, String)>,
    connected_device: Option<ConnectedDevice>,
    initial_data_load: Option<InitialDataLoad>,
    loading_frame: usize,
    cmd_tx: Option<mpsc::Sender<ManagerCommand>>,
    operation_id: u8,
    pending_confirmation: Option<PendingConfirmation>,
}

#[derive(Debug, Clone)]
enum AppState {
    Disconnected,
    Connecting(String),
    Identifying(String),
    Connected(ModelInfo),
    Error(String),
}

struct ConnectedDevice {
    model: ModelInfo,
    battery_left: Option<u8>,
    battery_right: Option<u8>,
    battery_case: Option<u8>,
    anc_status: u8,
    eq_mode: EqMode,
    eq_preset: u8,
    advanced_eq_enabled: bool,
    bass_level: u8,
    bass_enhance_enabled: bool,
    ringing_left: bool,
    ringing_right: bool,
    in_ear_enabled: bool,
    latency_low: bool,
    personalized_anc_enabled: bool,
    firmware_version: String,
    custom_eq: [f32; 3],
    ear_tip_left: Option<u8>,
    ear_tip_right: Option<u8>,
    ear_tip_test_running: bool,
}

struct InitialDataLoad {
    battery: bool,
    anc: bool,
    eq: bool,
    personalized_anc: bool,
    in_ear: bool,
    latency: bool,
    enhanced_bass: bool,
    custom_eq: bool,
    require_anc: bool,
    require_personalized_anc: bool,
}

impl InitialDataLoad {
    fn for_model(model: &ModelInfo) -> Self {
        Self {
            battery: false,
            anc: false,
            eq: false,
            personalized_anc: false,
            in_ear: false,
            latency: false,
            enhanced_bass: false,
            custom_eq: model.base == "B181",
            require_anc: model.is_anc,
            require_personalized_anc: EarNative::supports_personalized_anc(model),
        }
    }

    fn is_complete(&self) -> bool {
        self.battery
            && self.eq
            && (!self.require_personalized_anc || self.personalized_anc)
            && self.in_ear
            && self.latency
            && self.enhanced_bass
            && self.custom_eq
            && (!self.require_anc || self.anc)
    }
}

impl Default for EarNative {
    fn default() -> Self {
        Self {
            models: get_models(),
            sku_map: get_sku_map(),
            state: AppState::Disconnected,
            discovered_devices: Vec::new(),
            connected_device: None,
            initial_data_load: None,
            loading_frame: 0,
            cmd_tx: None,
            operation_id: 0,
            pending_confirmation: None,
        }
    }
}

impl EarNative {
    fn boot() -> (Self, Task<Message>) {
        (Self::default(), Task::none())
    }

    fn eq_command_for_model(model: &ModelInfo) -> u16 {
        if model.base == "B172" || model.base == "B168" {
            commands::SET_LISTENING_MODE
        } else {
            commands::SET_EQ
        }
    }

    fn eq_mode_from_raw(value: u8) -> EqMode {
        EqMode::from_u8(value).unwrap_or(EqMode::Balanced)
    }

    fn supports_ultra_bass(model: &ModelInfo) -> bool {
        matches!(model.base.as_str(), "B171" | "B172" | "B168" | "B162")
    }

    fn supports_advanced_eq(model: &ModelInfo) -> bool {
        matches!(model.base.as_str(), "B157" | "B155" | "B171" | "B174")
    }

    fn supports_personalized_anc(model: &ModelInfo) -> bool {
        model.base == "B155"
    }

    fn supports_ear_tip_test(model: &ModelInfo) -> bool {
        matches!(model.base.as_str(), "B155" | "B171" | "B172" | "B162")
    }

    fn supports_split_ring(model: &ModelInfo) -> bool {
        model.base != "B181" && !model.left_img.is_empty() && !model.right_img.is_empty()
    }

    fn custom_eq_active(model: &ModelInfo, eq_preset: u8, advanced_eq_enabled: bool) -> bool {
        if matches!(model.base.as_str(), "B172" | "B168") {
            eq_preset == 6
        } else {
            eq_preset == 5 && !advanced_eq_enabled
        }
    }

    fn eq_button_active(
        model: &ModelInfo,
        preset: u8,
        eq_preset: u8,
        advanced_eq_enabled: bool,
    ) -> bool {
        if EarNative::supports_advanced_eq(model) && preset == 6 {
            advanced_eq_enabled
        } else {
            !advanced_eq_enabled && preset == eq_preset
        }
    }

    fn eq_presets(model: &ModelInfo) -> Vec<(u8, &'static str)> {
        match model.base.as_str() {
            "B172" | "B168" => vec![
                (0, "dirac opteo"),
                (3, "pop"),
                (1, "rock"),
                (5, "classical"),
                (2, "electronic"),
                (4, "enhance vocals"),
                (6, "custom"),
            ],
            "B155" | "B157" | "B171" | "B174" => vec![
                (0, "balanced"),
                (3, "more bass"),
                (2, "more treble"),
                (1, "voice"),
                (5, "custom"),
                (6, "advanced"),
            ],
            _ => vec![
                (0, "balanced"),
                (3, "more bass"),
                (2, "more treble"),
                (1, "voice"),
                (5, "custom"),
            ],
        }
    }

    fn anc_strength_options(model: &ModelInfo) -> Vec<(u8, &'static str)> {
        match model.base.as_str() {
            "B181" => vec![(4, "high"), (3, "low")],
            "B163" => vec![(4, "high"), (5, "mid"), (3, "low")],
            "B155" | "B171" | "B162" | "B172" => {
                vec![(4, "high"), (5, "mid"), (3, "low"), (6, "adaptive")]
            }
            _ => vec![],
        }
    }

    fn matched_model_key(&self, device_name: &str) -> Option<&str> {
        let device_name = device_name.to_lowercase();
        self.models
            .iter()
            .find(|(_, model)| device_name.contains(&model.name.to_lowercase()))
            .map(|(key, _)| key.as_str())
    }

    fn matched_model_name<'a>(&'a self, device_name: &str) -> Option<&'a str> {
        self.matched_model_key(device_name)
            .and_then(|key| self.models.get(key))
            .map(|model| model.name.as_str())
    }

    fn inferred_model_key(&self, device_name: &str) -> String {
        self.matched_model_key(device_name)
            .unwrap_or("ear_1_black")
            .to_string()
    }

    fn ring_buds_payload(model: &ModelInfo, target: RingTarget, enabled: bool) -> Vec<u8> {
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

    fn set_ringing_state(device: &mut ConnectedDevice, target: RingTarget, enabled: bool) {
        match target {
            RingTarget::Left => device.ringing_left = enabled,
            RingTarget::Right => device.ringing_right = enabled,
            RingTarget::Both => {
                device.ringing_left = enabled;
                device.ringing_right = enabled;
            }
        }
    }

    fn confirm_message(target: RingTarget) -> &'static str {
        match target {
            RingTarget::Left => "start ringing the left earbud?",
            RingTarget::Right => "start ringing the right earbud?",
            RingTarget::Both => "start ringing both earbuds?",
        }
    }

    fn ear_tip_status_label(status: Option<u8>) -> &'static str {
        match status {
            Some(0) => "good fit",
            Some(1) => "adjust fit",
            Some(2) => "not detected",
            _ => "not tested",
        }
    }

    fn ear_tip_status_color(status: Option<u8>) -> Color {
        match status {
            Some(0) => Color::from_rgb(0.11, 0.69, 0.35),
            Some(1) => Color::from_rgb(0.95, 0.78, 0.12),
            Some(2) => Color::from_rgb(0.79, 0.13, 0.18),
            _ => GREY,
        }
    }

    fn ear_tip_summary(left: Option<u8>, right: Option<u8>, running: bool) -> &'static str {
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

    fn theme(&self) -> Theme {
        Theme::custom(
            "Dark".to_string(),
            iced::theme::Palette {
                background: PURE_BLACK,
                ..Theme::Dark.palette()
            },
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Ready(tx) => self.cmd_tx = Some(tx),
            Message::LoadingTick => {
                self.loading_frame = (self.loading_frame + 1) % 4;
            }
            Message::InitialDataLoadTimedOut => {
                self.initial_data_load = None;
            }
            Message::Bluetooth(event) => match event {
                BluetoothEvent::DeviceDiscovered(addr, name) => {
                    if !self.discovered_devices.iter().any(|(a, _)| a == &addr) {
                        self.discovered_devices.push((addr, name));
                    }
                }
                BluetoothEvent::Error(err) => {
                    log::error!("Bluetooth error event: {}", err);
                    self.state = AppState::Error(err.clone());
                    self.connected_device = None;
                    self.initial_data_load = None;
                    self.loading_frame = 0;
                    self.pending_confirmation = None;
                }
                BluetoothEvent::Connected(addr) => {
                    let name = self
                        .discovered_devices
                        .iter()
                        .find(|(a, _)| a == &addr)
                        .map(|(_, n)| n.clone())
                        .unwrap_or_else(|| "unknown".to_string());

                    log::info!("Connected to: {} ({})", name, addr);
                    log::info!("Identifying device...");

                    self.state = AppState::Identifying(name.clone());
                    self.initial_data_load = None;
                    self.loading_frame = 0;

                    let initial_model_key = self.inferred_model_key(&name);
                    let initial_model = self.models.get(&initial_model_key).unwrap().clone();

                    self.connected_device = Some(ConnectedDevice {
                        model: initial_model,
                        battery_left: None,
                        battery_right: None,
                        battery_case: None,
                        anc_status: 1,
                        eq_mode: EqMode::Balanced,
                        eq_preset: 0,
                        advanced_eq_enabled: false,
                        bass_level: 2,
                        bass_enhance_enabled: false,
                        ringing_left: false,
                        ringing_right: false,
                        in_ear_enabled: true,
                        latency_low: false,
                        personalized_anc_enabled: false,
                        firmware_version: "loading...".to_string(),
                        custom_eq: [0.0, 0.0, 0.0],
                        ear_tip_left: None,
                        ear_tip_right: None,
                        ear_tip_test_running: false,
                    });

                    return Task::batch(vec![
                        self.send_delayed_command(commands::READ_SKU, vec![], 100),
                        self.send_delayed_command(16392, vec![], 300),
                        self.send_delayed_command(57352, vec![], 500),
                        self.send_delayed_command(commands::READ_FIRMWARE, vec![], 700),
                    ]);
                }
                BluetoothEvent::Disconnected(_) => {
                    self.state = AppState::Disconnected;
                    self.connected_device = None;
                    self.initial_data_load = None;
                    self.loading_frame = 0;
                    self.pending_confirmation = None;
                }
                BluetoothEvent::PacketReceived(packet) => return self.handle_packet(packet),
            },
            Message::Connect(addr) => {
                self.state = AppState::Connecting("initializing".to_string());
                self.loading_frame = 0;
                return self.send_manager_command(ManagerCommand::Connect(addr));
            }
            Message::Disconnect => {
                self.state = AppState::Disconnected;
                self.connected_device = None;
                self.initial_data_load = None;
                self.loading_frame = 0;
                self.pending_confirmation = None;
                return self.send_manager_command(ManagerCommand::Disconnect);
            }
            Message::SetANC(l) => {
                if let Some(d) = &mut self.connected_device {
                    d.anc_status = l;
                    let proto = match l {
                        1 => 0x05,
                        2 => 0x07,
                        3 => 0x03,
                        4 => 0x01,
                        5 => 0x02,
                        6 => 0x04,
                        _ => 0x05,
                    };
                    return self.send_command(commands::SET_ANC, vec![0x01, proto, 0x00]);
                }
            }
            Message::SetEQ(m) => {
                let mut request = None;
                if let Some(d) = &mut self.connected_device {
                    d.eq_mode = EarNative::eq_mode_from_raw(m);
                    d.eq_preset = m;
                    d.advanced_eq_enabled = false;
                    request = Some((EarNative::eq_command_for_model(&d.model), vec![m, 0x00]));
                }
                if let Some((command, payload)) = request {
                    if let Some(d) = &self.connected_device {
                        if EarNative::supports_advanced_eq(&d.model) {
                            return Task::batch(vec![
                                self.send_command(
                                    commands::SET_ADVANCED_EQ_ENABLED,
                                    vec![0x00, 0x00],
                                ),
                                self.send_command(command, payload),
                            ]);
                        }
                    }
                    return self.send_command(command, payload);
                }
            }
            Message::ToggleAdvancedEQ(enabled) => {
                if let Some(d) = &mut self.connected_device {
                    if EarNative::supports_advanced_eq(&d.model) {
                        d.advanced_eq_enabled = enabled;
                        return self.send_command(
                            commands::SET_ADVANCED_EQ_ENABLED,
                            vec![if enabled { 0x01 } else { 0x00 }, 0x00],
                        );
                    }
                }
            }
            Message::SetBassLevel(l) => {
                let mut payload = None;
                if let Some(d) = &mut self.connected_device {
                    d.bass_level = l.clamp(1, 5);
                    payload = Some(vec![
                        if d.bass_enhance_enabled { 0x01 } else { 0x00 },
                        d.bass_level * 2,
                    ]);
                }
                if let Some(p) = payload {
                    return self.send_command(commands::SET_ENHANCED_BASS, p);
                }
            }
            Message::ToggleBassEnhance(e) => {
                let mut payload = None;
                if let Some(d) = &mut self.connected_device {
                    d.bass_enhance_enabled = e;
                    payload = Some(vec![if e { 0x01 } else { 0x00 }, d.bass_level * 2]);
                }
                if let Some(p) = payload {
                    return self.send_command(commands::SET_ENHANCED_BASS, p);
                }
            }
            Message::RequestRing(target) => {
                self.pending_confirmation = Some(PendingConfirmation::StartRing(target));
            }
            Message::StopRing(target) => {
                let mut request = None;
                if let Some(d) = &mut self.connected_device {
                    EarNative::set_ringing_state(d, target, false);
                    request = Some((
                        commands::SET_RING_BUDS,
                        EarNative::ring_buds_payload(&d.model, target, false),
                    ));
                }
                if let Some((command, payload)) = request {
                    return self.send_command(command, payload);
                }
            }
            Message::ConfirmPendingAction => {
                if let Some(PendingConfirmation::StartRing(target)) =
                    self.pending_confirmation.take()
                {
                    let mut request = None;
                    if let Some(d) = &mut self.connected_device {
                        EarNative::set_ringing_state(d, target, true);
                        request = Some((
                            commands::SET_RING_BUDS,
                            EarNative::ring_buds_payload(&d.model, target, true),
                        ));
                    }
                    if let Some((command, payload)) = request {
                        return self.send_command(command, payload);
                    }
                }
            }
            Message::CancelPendingAction => {
                self.pending_confirmation = None;
            }
            Message::SetPersonalizedANC(enabled) => {
                if let Some(d) = &mut self.connected_device {
                    if EarNative::supports_personalized_anc(&d.model) {
                        d.personalized_anc_enabled = enabled;
                        return self.send_command(
                            commands::SET_PERSONALIZED_ANC,
                            vec![if enabled { 0x01 } else { 0x00 }],
                        );
                    }
                }
            }
            Message::StartEarTipTest => {
                if let Some(d) = &mut self.connected_device {
                    if EarNative::supports_ear_tip_test(&d.model) {
                        d.ear_tip_left = None;
                        d.ear_tip_right = None;
                        d.ear_tip_test_running = true;
                        return self.send_command(commands::START_EAR_FIT_TEST, vec![0x01]);
                    }
                }
            }
            Message::ResetEarTipTest => {
                if let Some(d) = &mut self.connected_device {
                    d.ear_tip_left = None;
                    d.ear_tip_right = None;
                    d.ear_tip_test_running = false;
                }
            }
            Message::ToggleInEar(e) => {
                if let Some(d) = &mut self.connected_device {
                    d.in_ear_enabled = e;
                    return self.send_command(
                        commands::SET_IN_EAR,
                        vec![0x01, 0x01, if e { 0x01 } else { 0x00 }],
                    );
                }
            }
            Message::ToggleLatency(e) => {
                if let Some(d) = &mut self.connected_device {
                    d.latency_low = e;
                    return self.send_command(
                        commands::SET_LATENCY,
                        vec![if e { 0x01 } else { 0x02 }, 0x00],
                    );
                }
            }
            Message::IncCustomEQ(i) => {
                let mut payload = None;
                if let Some(d) = &mut self.connected_device {
                    if i < 3 && EarNative::supports_custom_eq(&d.model) {
                        d.custom_eq[i] = (d.custom_eq[i] + 1.0).min(6.0);
                        d.eq_mode = EqMode::Custom;
                        d.eq_preset = if matches!(d.model.base.as_str(), "B172" | "B168") {
                            6
                        } else {
                            5
                        };
                        d.advanced_eq_enabled = false;
                        payload = Some(d.custom_eq);
                    }
                }
                if let Some(custom_eq) = payload {
                    return self.send_custom_eq_commands(custom_eq);
                }
            }
            Message::DecCustomEQ(i) => {
                let mut payload = None;
                if let Some(d) = &mut self.connected_device {
                    if i < 3 && EarNative::supports_custom_eq(&d.model) {
                        d.custom_eq[i] = (d.custom_eq[i] - 1.0).max(-6.0);
                        d.eq_mode = EqMode::Custom;
                        d.eq_preset = if matches!(d.model.base.as_str(), "B172" | "B168") {
                            6
                        } else {
                            5
                        };
                        d.advanced_eq_enabled = false;
                        payload = Some(d.custom_eq);
                    }
                }
                if let Some(custom_eq) = payload {
                    return self.send_custom_eq_commands(custom_eq);
                }
            }
            Message::ScrollCustomEQ(i, delta) => {
                let mut payload = None;
                if let Some(d) = &mut self.connected_device {
                    if i < 3 && delta != 0 && EarNative::supports_custom_eq(&d.model) {
                        d.custom_eq[i] = (d.custom_eq[i] + delta as f32).clamp(-6.0, 6.0);
                        d.eq_mode = EqMode::Custom;
                        d.eq_preset = if matches!(d.model.base.as_str(), "B172" | "B168") {
                            6
                        } else {
                            5
                        };
                        d.advanced_eq_enabled = false;
                        payload = Some(d.custom_eq);
                    }
                }
                if let Some(custom_eq) = payload {
                    return self.send_custom_eq_commands(custom_eq);
                }
            }
            Message::SetCustomEQLevel(i, level) => {
                let mut payload = None;
                if let Some(d) = &mut self.connected_device {
                    if i < 3 && EarNative::supports_custom_eq(&d.model) {
                        d.custom_eq[i] = (level as f32).clamp(-6.0, 6.0);
                        d.eq_mode = EqMode::Custom;
                        d.eq_preset = if matches!(d.model.base.as_str(), "B172" | "B168") {
                            6
                        } else {
                            5
                        };
                        d.advanced_eq_enabled = false;
                        payload = Some(d.custom_eq);
                    }
                }
                if let Some(custom_eq) = payload {
                    return self.send_custom_eq_commands(custom_eq);
                }
            }
            Message::SendCustomEQ => {
                let mut payload = None;
                if let Some(d) = &mut self.connected_device {
                    if EarNative::supports_custom_eq(&d.model) {
                        d.eq_mode = EqMode::Custom;
                        d.eq_preset = if matches!(d.model.base.as_str(), "B172" | "B168") {
                            6
                        } else {
                            5
                        };
                        d.advanced_eq_enabled = false;
                        payload = Some(d.custom_eq);
                    }
                }
                if let Some(custom_eq) = payload {
                    return self.send_custom_eq_commands(custom_eq);
                }
            }
            _ => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let content: Element<'_, Message> = match &self.state {
            AppState::Disconnected => {
                let header = column![
                    text("ear (native)").font(Font::MONOSPACE).size(36),
                    text("status: disconnected")
                        .font(Font::MONOSPACE)
                        .size(14)
                        .color(GREY),
                ]
                .spacing(12)
                .align_x(Alignment::Center);

                let mut list = column![].spacing(12).align_x(Alignment::Center);

                if self.discovered_devices.is_empty() {
                    list = list.push(
                        text("searching for devices...")
                            .font(Font::MONOSPACE)
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
                                    .font(Font::MONOSPACE)
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
                        .font(Font::MONOSPACE)
                        .size(56)
                        .color(Color::from_rgb(1.0, 0.3, 0.3)),
                    text("connection error").font(Font::MONOSPACE).size(22),
                    text(msg.to_lowercase())
                        .font(Font::MONOSPACE)
                        .size(14)
                        .color(GREY),
                    button(
                        text("back to menu")
                            .font(Font::MONOSPACE)
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
                if self.initial_data_load.is_some() {
                    self.loading_view("loading headphone data", model.name.to_lowercase())
                } else if let Some(device) = &self.connected_device {
                    let device_images: Element<'_, Message> =
                        if !model.duo_img.is_empty() {
                            row![container(
                                image::<image::Handle>(image::Handle::from_path(
                                    model.duo_img.clone()
                                ),)
                                .width(260)
                                .filter_method(image::FilterMethod::Linear)
                            )]
                            .align_y(Alignment::Center)
                            .into()
                        } else {
                            row![
                                container(
                                    image::<image::Handle>(image::Handle::from_path(
                                        model.left_img.clone()
                                    ),)
                                    .width(90)
                                    .filter_method(image::FilterMethod::Linear)
                                ),
                                container(
                                    image::<image::Handle>(image::Handle::from_path(
                                        model.case_img.clone()
                                    ),)
                                    .width(90)
                                    .filter_method(image::FilterMethod::Linear)
                                ),
                                container(
                                    image::<image::Handle>(image::Handle::from_path(
                                        model.right_img.clone()
                                    ),)
                                    .width(90)
                                    .filter_method(image::FilterMethod::Linear)
                                ),
                            ]
                            .spacing(16)
                            .align_y(Alignment::Center)
                            .into()
                        };

                    let batt_box = |label: &str, val: Option<u8>| {
                        container(
                            column![
                                text(label.to_lowercase())
                                    .font(Font::MONOSPACE)
                                    .size(12)
                                    .color(GREY),
                                text(format!("{}%", val.unwrap_or(0)))
                                    .font(Font::MONOSPACE)
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
                    };

                    let battery_info = row![
                        batt_box("left", device.battery_left),
                        batt_box("case", device.battery_case),
                        batt_box("right", device.battery_right),
                    ]
                    .spacing(8)
                    .width(Length::Fill);

                    let section_title = |t: &str| {
                        container(
                            text(t.to_lowercase())
                                .font(Font::MONOSPACE)
                                .size(12)
                                .color(GREY),
                        )
                        .padding(Padding {
                            top: 0.0,
                            right: 0.0,
                            bottom: 4.0,
                            left: 0.0,
                        })
                    };

                    let make_btn = |label: &str, is_active: bool, msg: Message| {
                        let t = text(label.to_lowercase())
                            .font(Font::MONOSPACE)
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

                    let make_small_btn = |label: &str, is_active: bool, msg: Message| {
                        let t = text(label.to_lowercase())
                            .font(Font::MONOSPACE)
                            .size(12)
                            .width(Length::Fill)
                            .align_x(Alignment::Center);

                        let b = button(t).on_press(msg).width(Length::Fill).padding(10);

                        if is_active {
                            b.style(btn_style_active)
                        } else {
                            b.style(btn_style_default)
                        }
                    };

                    let anc_ui = column![
                        section_title("noise control"),
                        row![
                            make_btn(
                                "noise cancellation",
                                device.anc_status >= 3,
                                Message::SetANC(4)
                            ),
                            make_btn("transparent", device.anc_status == 2, Message::SetANC(2)),
                            make_btn("off", device.anc_status == 1, Message::SetANC(1)),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                        if device.anc_status >= 3 {
                            EarNative::anc_strength_options(model).into_iter().fold(
                                row![].spacing(8).width(Length::Fill),
                                |row, (preset, label)| {
                                    row.push(make_small_btn(
                                        label,
                                        device.anc_status == preset,
                                        Message::SetANC(preset),
                                    ))
                                },
                            )
                        } else {
                            row![]
                        },
                        if EarNative::supports_personalized_anc(model) {
                            row![make_btn(
                                if device.personalized_anc_enabled {
                                    "personalized anc [ on ]"
                                } else {
                                    "personalized anc [ off ]"
                                },
                                device.personalized_anc_enabled,
                                Message::SetPersonalizedANC(!device.personalized_anc_enabled)
                            )]
                        } else {
                            row![]
                        }
                    ]
                    .spacing(8);

                    let eq_ui = column![
                        section_title("equalizer"),
                        EarNative::eq_presets(model).chunks(2).fold(
                            column![].spacing(8),
                            |column, chunk| {
                                let mut current_row = row![].spacing(8).width(Length::Fill);

                                for (preset, label) in chunk {
                                    let message =
                                        if EarNative::supports_advanced_eq(model) && *preset == 6 {
                                            Message::ToggleAdvancedEQ(true)
                                        } else {
                                            Message::SetEQ(*preset)
                                        };

                                    current_row = current_row.push(make_btn(
                                        label,
                                        EarNative::eq_button_active(
                                            model,
                                            *preset,
                                            device.eq_preset,
                                            device.advanced_eq_enabled,
                                        ),
                                        message,
                                    ));
                                }

                                if chunk.len() == 1 {
                                    current_row =
                                        current_row.push(Space::new().width(Length::Fill));
                                }

                                column.push(current_row)
                            }
                        )
                    ]
                    .spacing(8);

                    let custom_eq_ui = if EarNative::supports_custom_eq(&device.model)
                        && EarNative::custom_eq_active(
                            model,
                            device.eq_preset,
                            device.advanced_eq_enabled,
                        ) {
                        let band_editor = |label: &'static str, value: f32, band_index: usize| {
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
                                        mouse_area(container(text("")).width(24).height(6).style(
                                            move |_theme| {
                                                container::Style {
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
                                                        color: if is_center {
                                                            GREY
                                                        } else {
                                                            BORDER_GREY
                                                        },
                                                        width: 1.0,
                                                        radius: 0.0.into(),
                                                    },
                                                    ..Default::default()
                                                }
                                            },
                                        ))
                                        .interaction(mouse::Interaction::Pointer)
                                        .on_press(
                                            Message::SetCustomEQLevel(band_index, step_value),
                                        ),
                                    )
                                },
                            );

                            mouse_area(
                                container(
                                    column![
                                        text(label).font(Font::MONOSPACE).size(12).color(GREY),
                                        text(format!("{:+.0} dB", value))
                                            .font(Font::MONOSPACE)
                                            .size(16),
                                        meter,
                                        row![
                                            button(
                                                text("-")
                                                    .font(Font::MONOSPACE)
                                                    .size(16)
                                                    .align_x(Alignment::Center)
                                            )
                                            .on_press(Message::DecCustomEQ(band_index))
                                            .width(Length::Fill)
                                            .padding(8)
                                            .style(btn_style_default),
                                            button(
                                                text("+")
                                                    .font(Font::MONOSPACE)
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
                        };

                        column![
                            section_title("custom eq"),
                            container(
                                column![row![
                                    band_editor("low", device.custom_eq[0], 0),
                                    band_editor("mid", device.custom_eq[1], 1),
                                    band_editor("high", device.custom_eq[2], 2),
                                ]
                                .spacing(8)
                                .width(Length::Fill),]
                                .spacing(16),
                            )
                            .padding(16)
                            .width(Length::Fill)
                            .style(|_theme| container::Style {
                                background: Some(Color::from_rgb(0.03, 0.03, 0.03).into()),
                                border: Border {
                                    color: if EarNative::custom_eq_active(
                                        model,
                                        device.eq_preset,
                                        device.advanced_eq_enabled,
                                    ) {
                                        PURE_WHITE
                                    } else {
                                        BORDER_GREY
                                    },
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

                    let advanced_eq_ui = if device.advanced_eq_enabled {
                        column![
                            section_title("advanced eq"),
                            container(
                                column![
                                    text("advanced eq is enabled")
                                        .font(Font::MONOSPACE)
                                        .size(14),
                                    text("select another preset to leave advanced mode")
                                        .font(Font::MONOSPACE)
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
                    } else {
                        column![]
                    };

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
                                    .font(Font::MONOSPACE)
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
                                    text(label).font(Font::MONOSPACE).size(12).color(GREY),
                                    text(EarNative::ear_tip_status_label(value))
                                        .font(Font::MONOSPACE)
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
                                    .font(Font::MONOSPACE)
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
                                text("firmware:").font(Font::MONOSPACE).size(12).color(GREY),
                                text(&device.firmware_version)
                                    .font(Font::MONOSPACE)
                                    .size(12)
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
                            .font(Font::MONOSPACE)
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
                                .font(Font::MONOSPACE)
                                .size(28)
                                .width(Length::Fill)
                                .align_x(Alignment::Center),
                            device_images,
                            battery_info,
                            if model.is_anc { anc_ui } else { column![] },
                            eq_ui,
                            custom_eq_ui,
                            advanced_eq_ui,
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
                        .font(Font::MONOSPACE)
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

    fn send_command(&mut self, command: u16, payload: Vec<u8>) -> Task<Message> {
        self.send_delayed_command(command, payload, 0)
    }

    fn format_float_for_eq(f: f32, total: bool) -> [u8; 4] {
        let mut b = f.to_be_bytes();
        if f != 0.0 && b[0] == 0 && b[1] == 0 && b[2] == 0 {
            b[3] = (b[3] | 0x80) as u8;
        }

        b.reverse();
        if total {
            if f >= 0.0 {
                return [0x00, 0x00, 0x00, 0x80];
            }
        }
        b
    }

    fn from_format_float_for_eq(mut bytes: [u8; 4]) -> f32 {
        bytes.reverse();

        if bytes[0] == 0 && bytes[1] == 0 && bytes[2] == 0 && (bytes[3] & 0x80) != 0 {
            bytes[3] &= 0x7f;
            -f32::from_be_bytes(bytes)
        } else {
            f32::from_be_bytes(bytes)
        }
    }

    fn supports_custom_eq(model: &ModelInfo) -> bool {
        model.base != "B181"
    }

    fn build_custom_eq_payload(custom_eq: [f32; 3]) -> Vec<u8> {
        let mut payload: Vec<u8> = vec![
            0x03, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x75, 0x44,
            0xc3, 0xf5, 0x28, 0x3f, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc0, 0x5a, 0x45, 0x00,
            0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x43, 0xcd, 0xcc,
            0x4c, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        let mut highest = 0.0f32;
        for value in custom_eq {
            highest = highest.max(value.abs());
        }

        let top_bytes = EarNative::format_float_for_eq(-highest, true);
        for j in 0..4 {
            payload[1 + j] = top_bytes[j];
        }

        for (index, band) in custom_eq.into_iter().enumerate() {
            let arr = EarNative::format_float_for_eq(band, false);
            let base = 6 + (index * 13);
            for j in 0..4 {
                payload[base + j] = arr[j];
            }
        }

        payload
    }

    fn send_custom_eq_commands(&mut self, custom_eq: [f32; 3]) -> Task<Message> {
        self.send_command(
            commands::SET_CUSTOM_EQ,
            EarNative::build_custom_eq_payload(custom_eq),
        )
    }

    fn loading_view(
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
                text(spinner).font(Font::MONOSPACE).size(56),
                text(title).font(Font::MONOSPACE).size(22),
                text(subtitle).font(Font::MONOSPACE).size(14).color(GREY),
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

    fn start_initial_data_load(&mut self, model: ModelInfo) -> Task<Message> {
        self.state = AppState::Connected(model.clone());
        self.initial_data_load = Some(InitialDataLoad::for_model(&model));

        if let Some(device) = &mut self.connected_device {
            device.model = model.clone();
        }

        Task::batch(vec![
            self.send_delayed_command(commands::READ_BATTERY, vec![], 100),
            self.send_delayed_command(commands::READ_ANC, vec![], 300),
            self.send_delayed_command(
                if model.base == "B172" || model.base == "B168" {
                    commands::READ_LISTENING_MODE
                } else {
                    commands::READ_EQ
                },
                vec![],
                500,
            ),
            if EarNative::supports_personalized_anc(&model) {
                self.send_delayed_command(commands::READ_PERSONALIZED_ANC, vec![], 650)
            } else {
                Task::none()
            },
            self.send_delayed_command(commands::READ_IN_EAR, vec![], 700),
            self.send_delayed_command(commands::READ_LATENCY, vec![], 900),
            self.send_delayed_command(commands::READ_ENHANCED_BASS, vec![], 1100),
            if EarNative::supports_custom_eq(&model) {
                self.send_delayed_command(commands::READ_ADVANCED_EQ, vec![], 1300)
            } else {
                Task::none()
            },
            if EarNative::supports_custom_eq(&model) {
                self.send_delayed_command(commands::READ_CUSTOM_EQ, vec![], 1500)
            } else {
                Task::none()
            },
            Task::perform(
                async {
                    tokio::time::sleep(std::time::Duration::from_millis(2600)).await;
                },
                |_| Message::InitialDataLoadTimedOut,
            ),
        ])
    }

    fn mark_initial_data_loaded(&mut self, update: impl FnOnce(&mut InitialDataLoad)) {
        if let Some(load) = &mut self.initial_data_load {
            update(load);
            if load.is_complete() {
                self.initial_data_load = None;
            }
        }
    }

    fn send_delayed_command(
        &mut self,
        command: u16,
        payload: Vec<u8>,
        delay_ms: u64,
    ) -> Task<Message> {
        self.operation_id = self.operation_id.wrapping_add(1);
        let packet = Packet::new(command, payload.clone(), self.operation_id);
        if let Some(tx) = &self.cmd_tx {
            let tx = tx.clone();
            return Task::perform(
                async move {
                    if delay_ms > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                    let _ = tx.send(ManagerCommand::SendPacket(packet.clone())).await;
                    packet
                },
                Message::CommandSent,
            );
        }
        Task::none()
    }

    fn send_manager_command(&self, command: ManagerCommand) -> Task<Message> {
        if let Some(tx) = &self.cmd_tx {
            let tx = tx.clone();
            return Task::perform(
                async move {
                    let _ = tx.send(command).await;
                },
                |_| Message::CommandSent(Packet::new(0, vec![], 0)),
            );
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let bluetooth = Subscription::run(|| {
            iced::stream::channel(
                100,
                |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
                    let (tx, mut rx) = mpsc::channel(100);
                    let (cmd_tx, cmd_rx) = mpsc::channel(100);
                    let _ = output.send(Message::Ready(cmd_tx)).await;
                    if let Ok(manager) = BluetoothManager::new(tx, cmd_rx).await {
                        let _ = manager.start_discovery().await;
                        let run_m = manager.run();
                        let out_l = async {
                            while let Some(e) = rx.recv().await {
                                let _ = output.send(Message::Bluetooth(e)).await;
                            }
                        };
                        tokio::select! { _ = run_m => {}, _ = out_l => {}, }
                    }
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                },
            )
        });

        if matches!(
            self.state,
            AppState::Connecting(_) | AppState::Identifying(_)
        ) || self.initial_data_load.is_some()
        {
            Subscription::batch(vec![
                bluetooth,
                time::every(std::time::Duration::from_millis(120)).map(|_| Message::LoadingTick),
            ])
        } else {
            bluetooth
        }
    }

    fn handle_packet(&mut self, packet: Packet) -> Task<Message> {
        log::info!(
            "Received packet: cmd=0x{:04x}, cmd-decimal={}, payload={:02x?}",
            packet.command,
            packet.command,
            packet.payload
        );

        match packet.command {
            // read SKU / model ID
            commands::READ_SKU | 16392 | 57352 => {
                if let AppState::Identifying(name) = &self.state {
                    let sku = if packet.payload.len() >= 2 {
                        format!("{:02x}", packet.payload[1])
                    } else if !packet.payload.is_empty() {
                        format!("{:02x}", packet.payload[0])
                    } else {
                        "unknown".to_string()
                    };

                    log::info!("Received SKU: {}", sku);

                    let model_key = self
                        .sku_map
                        .get(&sku)
                        .cloned()
                        .unwrap_or_else(|| self.inferred_model_key(name));

                    let model = self.models.get(&model_key).unwrap().clone();
                    log::info!(
                        "Identified model: {} ({}) via SKU: {}",
                        model.name,
                        model.base,
                        sku
                    );

                    return self.start_initial_data_load(model);
                }
            }
            _ => {}
        }

        if let Some(device) = &mut self.connected_device {
            match packet.command {
                // read battery
                49159 | 57345 | 16391 => {
                    let p = packet.payload.clone();
                    if p.len() >= 1 {
                        for i in 0..p[0] as usize {
                            if p.len() >= 3 + (i * 2) {
                                let val = p[2 + (i * 2)] & 127;
                                match p[1 + (i * 2)] {
                                    2 => device.battery_left = Some(val),
                                    3 => device.battery_right = Some(val),
                                    4 => device.battery_case = Some(val),
                                    _ => {}
                                }
                            }
                        }

                        log::info!(
                            "Parsed battery status - Left: {:?}%, Right: {:?}%, Case: {:?}%",
                            device.battery_left,
                            device.battery_right,
                            device.battery_case
                        );
                    } else {
                        log::warn!(
                            "Unexpected payload length for battery status: {}",
                            packet.payload.len()
                        );
                    }
                    self.mark_initial_data_loaded(|load| load.battery = true);
                }
                // read anc mode
                49182 | 57347 | 16414 => {
                    if packet.payload.len() >= 2 {
                        let val = packet.payload[1];
                        device.anc_status = match val {
                            0x05 => 1,
                            0x07 => 2,
                            0x03 => 3,
                            0x01 => 4,
                            0x02 => 5,
                            0x04 => 6,
                            _ => device.anc_status,
                        };

                        log::info!("Parsed ANC status: {}", device.anc_status);
                    } else {
                        log::warn!(
                            "Unexpected payload length for ANC status: {}",
                            packet.payload.len()
                        );
                    }
                    self.mark_initial_data_loaded(|load| load.anc = true);
                }
                // read equaliser mode or listening mode
                49183 | 16415 | 16464 | 49232 => {
                    if !packet.payload.is_empty() {
                        device.eq_preset = packet.payload[0];
                        device.eq_mode = EarNative::eq_mode_from_raw(packet.payload[0]);
                        device.advanced_eq_enabled = false;

                        log::info!("Parsed EQ/Listening mode raw: {}", device.eq_preset);
                    } else {
                        log::warn!(
                            "Unexpected payload length for EQ/Listening mode: {}",
                            packet.payload.len()
                        );
                    }
                    self.mark_initial_data_loaded(|load| load.eq = true);
                }
                commands::READ_PERSONALIZED_ANC | commands::RESP_PERSONALIZED_ANC => {
                    if !packet.payload.is_empty() {
                        device.personalized_anc_enabled = packet.payload[0] == 0x01;
                    }
                    self.mark_initial_data_loaded(|load| load.personalized_anc = true);
                }
                // read advanced eq status
                49228 | 16460 => {
                    if !packet.payload.is_empty() {
                        device.advanced_eq_enabled = packet.payload[0] == 0x01;
                    }
                }
                // read firmware version
                49218 | 16450 => {
                    device.firmware_version = String::from_utf8_lossy(&packet.payload)
                        .trim_matches(char::from(0))
                        .trim()
                        .to_lowercase();

                    if let AppState::Identifying(name) = &self.state {
                        log::info!("Identifying via Firmware fallback for: {}", name);
                        let model_key = self.inferred_model_key(name);

                        let model = self.models.get(&model_key).unwrap().clone();
                        log::info!(
                            "Identified model via fallback: {} ({})",
                            model.name,
                            model.base
                        );

                        return self.start_initial_data_load(model);
                    }
                }
                // read in-ear detect status
                49166 | 16398 => {
                    if packet.payload.len() >= 3 {
                        device.in_ear_enabled = packet.payload[2] == 0x01;

                        log::info!("Parsed in-ear detect status: {}", device.in_ear_enabled);
                    } else {
                        log::warn!(
                            "Unexpected payload length for in-ear status: {}",
                            packet.payload.len()
                        );
                    }
                    self.mark_initial_data_loaded(|load| load.in_ear = true);
                }
                // read low latency mode status
                49217 | 16449 => {
                    if !packet.payload.is_empty() {
                        device.latency_low = packet.payload[0] == 0x01;

                        log::info!("Parsed latency mode status: {}", device.latency_low);
                    } else {
                        log::warn!(
                            "Unexpected payload length for latency status: {}",
                            packet.payload.len()
                        );
                    }
                    self.mark_initial_data_loaded(|load| load.latency = true);
                }
                // read bass boost level
                49230 | commands::RESP_ENHANCED_BASS => {
                    if packet.payload.len() >= 2 {
                        device.bass_enhance_enabled = packet.payload[0] == 0x01;
                        device.bass_level = packet.payload[1] / 2;

                        log::info!(
                            "Parsed bass boost status: enabled={}, level={}",
                            device.bass_enhance_enabled,
                            device.bass_level
                        );
                    } else {
                        log::warn!(
                            "Unexpected payload length for bass boost status: {}",
                            packet.payload.len()
                        );
                    }
                    self.mark_initial_data_loaded(|load| load.enhanced_bass = true);
                }
                // read custom eq response
                49220 | 16452 => {
                    if packet.payload.len() >= 45 && EarNative::supports_custom_eq(&device.model) {
                        let mut values = [0.0f32; 3];
                        for (index, slot) in values.iter_mut().enumerate() {
                            let base = 6 + (index * 13);
                            let bytes = [
                                packet.payload[base],
                                packet.payload[base + 1],
                                packet.payload[base + 2],
                                packet.payload[base + 3],
                            ];
                            *slot = EarNative::from_format_float_for_eq(bytes)
                                .round()
                                .clamp(-6.0, 6.0);
                        }
                        // JS reorders to [level[2], level[0], level[1]]
                        device.custom_eq = [values[2], values[0], values[1]];
                        log::info!("Parsed custom EQ: {:?}", device.custom_eq);
                    } else {
                        log::warn!(
                            "Unexpected payload length for custom EQ: {}",
                            packet.payload.len()
                        );
                    }
                    self.mark_initial_data_loaded(|load| load.custom_eq = true);
                }
                commands::RESP_EAR_FIT_TEST => {
                    if packet.payload.len() >= 2 {
                        device.ear_tip_left = Some(packet.payload[0]);
                        device.ear_tip_right = Some(packet.payload[1]);
                        device.ear_tip_test_running = false;
                    }
                }
                _ => {}
            }
        }
        Task::none()
    }
}
