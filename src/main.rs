use futures::{channel::mpsc::Sender, SinkExt};
use iced::{
    time,
    widget::{button, column, container, image, row, scrollable, text},
    Alignment, Border, Color, Element, Font, Length, Padding, Subscription, Task, Theme,
};

use std::collections::HashMap;
use tokio::sync::mpsc;

mod app;
mod bluetooth;
mod components;
mod config;
mod models;
mod protocol;
mod ui;

use bluetooth::{
    create_adapter, BluetoothEvent, BluetoothManager, DiscoveredDevice, ManagerCommand,
};
use components::{anc, battery, equalizer};
use config::AppConfig;
use models::{
    embedded_image_handle, get_models, get_sku_map, preload_model_images_in_background, ModelInfo,
};
use protocol::Packet;
use ui::{
    app_font, btn_style_active, btn_style_default, btn_style_red, APP_FONT_NAME, BORDER_GREY, GREY,
    PURE_BLACK, PURE_WHITE,
};

use std::time::{Duration, Instant};

use crate::{
    app::state::{
        AppState, ConnectedDevice, InitialDataLoad, Message, PendingConfirmation, RingTarget,
    },
    protocol::{AncMode, DeviceId, EqMode, PacketCommand, ParsedResponse},
};

struct EarNative {
    models: HashMap<String, ModelInfo>,
    sku_map: HashMap<String, String>,
    config: AppConfig,
    last_auto_connect_at: Option<Instant>,
    active_model_assets_ready: bool,
    state: AppState,
    discovered_devices: Vec<(String, String)>,
    connected_device: Option<ConnectedDevice>,
    initial_data_load: Option<InitialDataLoad>,
    loading_frame: usize,
    cmd_tx: Option<mpsc::Sender<ManagerCommand>>,
    operation_id: u8,
    pending_confirmation: Option<PendingConfirmation>,
}

fn tray_event_stream() -> impl futures::Stream<Item = Message> + 'static {
    iced::stream::channel(100, |mut output: Sender<Message>| async move {
        use iced::futures::SinkExt;

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        tray_icon::TrayIconEvent::set_event_handler(Some(move |event| {
            let _ = tx.send(event);
        }));

        let mut last_click = Instant::now() - Duration::from_secs(1);

        while let Some(event) = rx.recv().await {
            match event {
                tray_icon::TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    ..
                } => {
                    if last_click.elapsed() < Duration::from_millis(120) {
                        continue;
                    }

                    last_click = Instant::now();
                    let _ = output.send(Message::TrayIconClicked).await;
                }
                _ => {}
            }
        }
    })
}

pub fn main() -> iced::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let _tray_icon = {
        let size = 32u32;
        let cx = size as f32 / 2.0;
        let cy = size as f32 / 2.0;
        let r = size as f32 / 2.0 - 1.5;
        let rgba: Vec<u8> = (0..size)
            .flat_map(|y| {
                (0..size).flat_map(move |x| {
                    let dx = x as f32 - cx;
                    let dy = y as f32 - cy;
                    if (dx * dx + dy * dy).sqrt() <= r {
                        [255u8, 255, 255, 255]
                    } else {
                        [0u8, 0, 0, 0]
                    }
                })
            })
            .collect();
        tray_icon::TrayIconBuilder::new()
            .with_tooltip("ear-native")
            .with_icon(tray_icon::Icon::from_rgba(rgba, size, size).expect("tray icon rgba"))
            .build()
            .expect("failed to create tray icon")
    };

    let mut settings = iced::window::Settings::default();
    settings.size = iced::Size::new(450.0, 700.0);

    iced::application(EarNative::boot, EarNative::update, EarNative::view)
        .theme(EarNative::theme)
        .subscription(EarNative::subscription)
        .font(include_bytes!("../res/fonts/Silkscreen-Regular.ttf").as_slice())
        .default_font(Font::with_name(APP_FONT_NAME))
        .window(settings)
        .run()
}

impl Default for EarNative {
    fn default() -> Self {
        Self {
            models: get_models(),
            sku_map: get_sku_map(),
            config: AppConfig::default(),
            last_auto_connect_at: None,
            active_model_assets_ready: true,
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
        (
            Self::default(),
            Task::perform(
                async { AppConfig::load_or_default() },
                Message::ConfigLoaded,
            ),
        )
    }

    fn eq_command_for_model(model: &ModelInfo) -> crate::protocol::PacketCommand {
        if model.base == "B172" || model.base == "B168" {
            PacketCommand::SetListeningMode
        } else {
            PacketCommand::SetEq
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
            Message::ActiveModelAssetsPreloaded => {
                self.active_model_assets_ready = true;
            }
            Message::ConfigLoaded(config) => {
                self.config = config;
            }
            Message::ConfigPersisted(result) => {
                if let Err(error) = result {
                    log::error!("failed to persist config: {}", error);
                }
            }
            Message::Ready(tx) => self.cmd_tx = Some(tx),
            Message::LoadingTick => {
                self.loading_frame = (self.loading_frame + 1) % 4;
            }
            Message::InitialDataLoadTimedOut => {
                self.initial_data_load = None;
            }
            Message::Bluetooth(event) => match event {
                BluetoothEvent::DeviceDiscovered(device) => {
                    let mut tasks = Vec::new();

                    if let Some(existing) = self
                        .discovered_devices
                        .iter_mut()
                        .find(|(addr, _)| addr == &device.id)
                    {
                        existing.1 = device.name.clone();
                    } else {
                        self.discovered_devices
                            .push((device.id.clone(), device.name.clone()));
                    }

                    if self.config.remember_device_name(&device.id, &device.name) {
                        tasks.push(self.persist_config());
                    }

                    if let Some(task) = self.maybe_auto_connect(&device) {
                        tasks.push(task);
                    }

                    if !tasks.is_empty() {
                        return Task::batch(tasks);
                    }
                }
                BluetoothEvent::Error(err) => {
                    log::error!("Bluetooth error event: {}", err);
                    self.state = AppState::Error(err.clone());
                    self.active_model_assets_ready = true;
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

                    let mut tasks = Vec::new();
                    if self.config.remember_connected_device(&addr) {
                        tasks.push(self.persist_config());
                    }

                    let initial_model_key = self.inferred_model_key(&name);
                    let initial_model = self.models.get(&initial_model_key).unwrap().clone();

                    self.connected_device = Some(ConnectedDevice {
                        id: addr.clone(),
                        model: initial_model,
                        battery_left: None,
                        sku_attempts: 0,
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

                    if let Some(cached_model_key) = self.config.known_model_key(&addr) {
                        if let Some(cached_model) = self.models.get(cached_model_key).cloned() {
                            log::info!(
                                "Using cached model for {}: {} ({})",
                                addr,
                                cached_model.name,
                                cached_model.base
                            );

                            tasks.push(self.start_initial_data_load(cached_model));
                            tasks.push(self.send_delayed_command(
                                PacketCommand::ReadFirmware,
                                vec![],
                                1700,
                            ));

                            return Task::batch(tasks);
                        }
                    }

                    tasks.extend(vec![
                        self.send_delayed_command(PacketCommand::ReadSku, vec![], 100),
                        self.send_delayed_command(PacketCommand::ReadSkuAlt, vec![], 300),
                        self.send_delayed_command(PacketCommand::RespSku, vec![], 500),
                        self.send_delayed_command(PacketCommand::ReadFirmware, vec![], 700),
                    ]);

                    return Task::batch(tasks);
                }
                BluetoothEvent::Disconnected => {
                    self.state = AppState::Disconnected;
                    self.active_model_assets_ready = true;
                    self.connected_device = None;
                    self.initial_data_load = None;
                    self.loading_frame = 0;
                    self.pending_confirmation = None;
                }
                BluetoothEvent::PacketReceived(packet) => return self.handle_packet(packet),
            },
            Message::Connect(addr) => {
                log::info!("Manual connect requested for {}", addr);
                self.state = AppState::Connecting("initializing".to_string());
                self.active_model_assets_ready = true;
                self.loading_frame = 0;
                return self.send_manager_command(ManagerCommand::Connect(addr));
            }
            Message::Disconnect => {
                self.state = AppState::Disconnected;
                self.active_model_assets_ready = true;
                self.connected_device = None;
                self.initial_data_load = None;
                self.loading_frame = 0;
                self.pending_confirmation = None;
                return self.send_manager_command(ManagerCommand::Disconnect);
            }
            Message::SetANC(l) => {
                if let Some(d) = &mut self.connected_device {
                    if d.anc_status == l {
                        return Task::none();
                    }

                    // Apply change locally immediately and do not wait for device confirmation.
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

                    return Task::batch(vec![
                        self.send_command(PacketCommand::SetAnc, vec![0x01, proto, 0x00]),
                        self.send_command(PacketCommand::SetAnc, vec![0x01, proto, 0x00]),
                    ]);
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
                                    PacketCommand::SetAdvancedEqEnabled,
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
                            PacketCommand::SetAdvancedEqEnabled,
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
                    return self.send_command(PacketCommand::SetEnhancedBass, p);
                }
            }
            Message::ToggleBassEnhance(e) => {
                let mut payload = None;
                if let Some(d) = &mut self.connected_device {
                    d.bass_enhance_enabled = e;
                    payload = Some(vec![if e { 0x01 } else { 0x00 }, d.bass_level * 2]);
                }
                if let Some(p) = payload {
                    return self.send_command(PacketCommand::SetEnhancedBass, p);
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
                        PacketCommand::SetRingBuds,
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
                            PacketCommand::SetRingBuds,
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
                            PacketCommand::SetPersonalizedAnc,
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
                        return self.send_command(PacketCommand::StartEarFitTest, vec![0x01]);
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
                        PacketCommand::SetInEar,
                        vec![0x01, 0x01, if e { 0x01 } else { 0x00 }],
                    );
                }
            }
            Message::ToggleLatency(e) => {
                if let Some(d) = &mut self.connected_device {
                    d.latency_low = e;
                    return self.send_command(
                        PacketCommand::SetLatency,
                        vec![if e { 0x01 } else { 0x00 }, 0x00],
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

            Message::TrayIconClicked => {
                log::info!("tray icon clicked");
            }

            _ => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
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

    fn send_command(
        &mut self,
        command: crate::protocol::PacketCommand,
        payload: Vec<u8>,
    ) -> Task<Message> {
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

    fn supports_custom_eq(model: &ModelInfo) -> bool {
        equalizer::supports_custom_eq(model)
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
            PacketCommand::SetCustomEq,
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

    fn start_initial_data_load(&mut self, model: ModelInfo) -> Task<Message> {
        self.state = AppState::Connected(model.clone());
        self.initial_data_load = Some(InitialDataLoad::for_model(&model));
        self.active_model_assets_ready = false;

        if let Some(device) = &mut self.connected_device {
            device.model = model.clone();
        }

        let asset_model = model.clone();

        Task::batch(vec![
            Task::perform(
                async move {
                    preload_model_images_in_background(vec![asset_model]).await;
                },
                |_| Message::ActiveModelAssetsPreloaded,
            ),
            self.send_delayed_command(PacketCommand::ReadBattery, vec![], 100),
            self.send_delayed_command(PacketCommand::ReadAnc, vec![], 300),
            self.send_delayed_command(
                if model.base == "B172" || model.base == "B168" {
                    PacketCommand::ReadListeningMode
                } else {
                    PacketCommand::ReadEq
                },
                vec![],
                500,
            ),
            if EarNative::supports_personalized_anc(&model) {
                self.send_delayed_command(PacketCommand::ReadPersonalizedAnc, vec![], 650)
            } else {
                Task::none()
            },
            self.send_delayed_command(PacketCommand::ReadInEar, vec![], 700),
            self.send_delayed_command(PacketCommand::ReadLatency, vec![], 900),
            self.send_delayed_command(PacketCommand::ReadEnhancedBass, vec![], 1100),
            if EarNative::supports_custom_eq(&model) {
                self.send_delayed_command(PacketCommand::ReadAdvancedEq, vec![], 1300)
            } else {
                Task::none()
            },
            if EarNative::supports_custom_eq(&model) {
                self.send_delayed_command(PacketCommand::ReadCustomEq, vec![], 1500)
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
        command: crate::protocol::PacketCommand,
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
                |_| Message::CommandSent,
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
                |_| Message::CommandSent,
            );
        }
        Task::none()
    }

    fn persist_config(&self) -> Task<Message> {
        let config = self.config.clone();
        Task::perform(async move { config.save() }, Message::ConfigPersisted)
    }

    fn maybe_auto_connect(&mut self, device: &DiscoveredDevice) -> Option<Task<Message>> {
        let Some(last_connected) = self.config.last_connected_device_id.as_deref() else {
            return None;
        };

        if !matches!(self.state, AppState::Disconnected) || self.connected_device.is_some() {
            return None;
        }

        if let Some(last_attempt) = self.last_auto_connect_at {
            if last_attempt.elapsed() < Duration::from_secs(30) {
                log::info!(
                    "Skipping auto-connect for {} because the last auto-connect attempt was {}s ago",
                    device.name,
                    last_attempt.elapsed().as_secs()
                );
                return None;
            }
        }

        if !device.system_connected
            || !device.paired
            || device.id != last_connected
            || self.cmd_tx.is_none()
        {
            return None;
        }

        log::info!(
            "Auto-connecting to system-connected device: {}",
            device.name
        );
        self.last_auto_connect_at = Some(Instant::now());
        self.state = AppState::Connecting("system reconnect".to_string());
        self.active_model_assets_ready = true;
        self.loading_frame = 0;

        Some(self.send_manager_command(ManagerCommand::Connect(device.id.clone())))
    }

    fn subscription(&self) -> Subscription<Message> {
        let bluetooth = Subscription::run(|| {
            iced::stream::channel(
                100,
                |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
                    let (tx, mut rx) = mpsc::channel(100);
                    let (cmd_tx, cmd_rx) = mpsc::channel(100);
                    let _ = output.send(Message::Ready(cmd_tx)).await;
                    let manager = loop {
                        match create_adapter().await {
                            Ok(adapter) => {
                                break BluetoothManager::new(adapter, tx.clone(), cmd_rx)
                            }
                            Err(error) => {
                                log::warn!("Bluetooth adapter not ready yet: {}", error);
                                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                            }
                        }
                    };

                    let _ = manager.start_discovery().await;
                    let run_m = manager.run();
                    let out_l = async {
                        while let Some(e) = rx.recv().await {
                            let _ = output.send(Message::Bluetooth(e)).await;
                        }
                    };
                    tokio::select! { _ = run_m => {}, _ = out_l => {}, }
                },
            )
        });

        let tray = Subscription::run(tray_event_stream);

        let is_loading = matches!(
            self.state,
            AppState::Connecting(_) | AppState::Identifying(_)
        ) || self.initial_data_load.is_some()
            || !self.active_model_assets_ready;

        let mut subs = vec![bluetooth, tray];

        if is_loading {
            subs.push(
                time::every(std::time::Duration::from_millis(120)).map(|_| Message::LoadingTick),
            );
        }

        Subscription::batch(subs)
    }

    fn handle_packet(&mut self, packet: Packet) -> Task<Message> {
        let parsed = packet.parse();

        if let ParsedResponse::Sku(sku) = parsed.clone() {
            if let AppState::Identifying(name) = &self.state {
                log::info!("Received SKU: {}", sku);

                if sku == "unknown" {
                    if let Some(dev) = &mut self.connected_device {
                        if dev.sku_attempts < 3 {
                            dev.sku_attempts = dev.sku_attempts.saturating_add(1);
                            log::warn!(
                                "SKU unknown, retrying read (attempt {}/3)",
                                dev.sku_attempts
                            );

                            return Task::batch(vec![self.send_delayed_command(
                                PacketCommand::ReadSku,
                                vec![],
                                300,
                            )]);
                        } else {
                            log::warn!("SKU unknown after retries, falling back to inference");
                        }
                    }
                }

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

                if let Some(device_id) = self
                    .connected_device
                    .as_ref()
                    .map(|device| device.id.clone())
                {
                    if self.config.remember_device_metadata(
                        &device_id,
                        Some(&model.name),
                        Some(&model_key),
                        Some(&sku),
                    ) {
                        return Task::batch(vec![
                            self.persist_config(),
                            self.start_initial_data_load(model),
                        ]);
                    }
                }

                return self.start_initial_data_load(model);
            }
        }

        if let Some(device) = &mut self.connected_device {
            match parsed {
                ParsedResponse::Battery(b) => {
                    device.battery_left = b.left.map(|s| s.level);
                    device.battery_right = b.right.map(|s| s.level);
                    device.battery_case = b.case.map(|s| s.level);

                    log::info!(
                        "Parsed battery status - Left: {:?}%, Right: {:?}%, Case: {:?}%",
                        device.battery_left,
                        device.battery_right,
                        device.battery_case
                    );
                    self.mark_initial_data_loaded(|load| load.battery = true);
                }
                ParsedResponse::Anc(mode) => {
                    device.anc_status = match mode {
                        AncMode::Off => 1,
                        AncMode::Transparent => 2,
                        AncMode::NcLow => 3,
                        AncMode::NcHigh => 4,
                        AncMode::NcMid => 5,
                        AncMode::NcAdaptive => 6,
                    };
                    log::info!("Parsed ANC status: {}", device.anc_status);
                    self.mark_initial_data_loaded(|load| load.anc = true);
                }
                ParsedResponse::Eq { mode: _, preset } => {
                    device.eq_preset = preset;
                    device.eq_mode = EarNative::eq_mode_from_raw(preset);
                    device.advanced_eq_enabled = false;
                    log::info!("Parsed EQ/Listening mode raw: {}", device.eq_preset);
                    self.mark_initial_data_loaded(|load| load.eq = true);
                }
                ParsedResponse::PersonalizedAnc(enabled) => {
                    device.personalized_anc_enabled = enabled;
                    self.mark_initial_data_loaded(|load| load.personalized_anc = true);
                }
                ParsedResponse::AdvancedEq(enabled) => {
                    device.advanced_eq_enabled = enabled;
                }
                ParsedResponse::Firmware(s) => {
                    device.firmware_version = s.trim_matches(char::from(0)).trim().to_lowercase();

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
                ParsedResponse::InEar(b) => {
                    device.in_ear_enabled = b;
                    log::info!("Parsed in-ear detect status: {}", device.in_ear_enabled);
                    self.mark_initial_data_loaded(|load| load.in_ear = true);
                }
                ParsedResponse::Latency(b) => {
                    device.latency_low = b;
                    log::info!("Parsed latency mode status: {}", device.latency_low);
                    self.mark_initial_data_loaded(|load| load.latency = true);
                }
                ParsedResponse::EnhancedBass { enabled, level } => {
                    device.bass_enhance_enabled = enabled;
                    device.bass_level = level;
                    log::info!(
                        "Parsed bass boost status: enabled={}, level={}",
                        device.bass_enhance_enabled,
                        device.bass_level
                    );
                    self.mark_initial_data_loaded(|load| load.enhanced_bass = true);
                }
                ParsedResponse::CustomEq(values) => {
                    if EarNative::supports_custom_eq(&device.model) {
                        device.custom_eq = values;
                        log::info!("Parsed custom EQ: {:?}", device.custom_eq);
                    }
                    self.mark_initial_data_loaded(|load| load.custom_eq = true);
                }
                ParsedResponse::EarFitTest { left, right } => {
                    device.ear_tip_left = Some(left);
                    device.ear_tip_right = Some(right);
                    device.ear_tip_test_running = false;
                }
                _ => {}
            }
        }
        Task::none()
    }
}
