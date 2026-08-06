use crate::{
    app::state::{AppState, ConnectedDevice, Message, PendingConfirmation},
    bluetooth::{BluetoothEvent, ManagerCommand},
    protocol::{AncMode, EqMode, PacketCommand, ParsedResponse},
    tray::TrayEvent,
    EarNative,
};

use iced::Task;

impl EarNative {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ActiveModelAssetsPreloaded => {
                self.active_model_assets_ready = true;
            }
            Message::ConfigLoaded(config) => {
                self.config = config;
            }
            Message::ConfigPersisted(Err(error)) => {
                log::error!("failed to persist config: {}", error);
            }
            Message::ConfigPersisted(Ok(_)) => {}
            Message::Ready(tx) => {
                self.cmd_tx = Some(tx);
                self.bluetooth_init_failed = false;
            }
            Message::BluetoothInitializationFailed(error) => {
                log::error!("Bluetooth initialization failed: {}", error);
                self.bluetooth_init_failed = true;
                self.state = AppState::Error(format!("Bluetooth is disabled. {}", error));
            }
            Message::LoadingTick => {
                self.loading_frame = (self.loading_frame + 1) % 4;
            }
            Message::InitialDataLoadTimedOut => {
                self.initial_data_load = None;
            }
            Message::IdentificationTimeout => {
                if let AppState::Identifying(name) = &self.state {
                    log::warn!(
                        "Identification timed out for {}, falling back to inference",
                        name
                    );
                    let model_key = self.inferred_model_key(name);
                    let model = self.models.get(&model_key).unwrap().clone();
                    return self.start_initial_data_load(model);
                }
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
                    self.bluetooth_init_failed = false;
                    self.initial_data_load = None;
                    self.loading_frame = 0;

                    let mut tasks = Vec::new();
                    if self.config.remember_connected_device(&addr) {
                        log::debug!("Persisting config for connected device");
                        tasks.push(self.persist_config());
                    }

                    let initial_model_key = self.inferred_model_key(&name);
                    let initial_model = self.models.get(&initial_model_key).unwrap().clone();
                    log::debug!("Inferred model: {}", initial_model.name);

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

                    log::debug!("Sending SKU identification commands...");
                    tasks.extend(vec![
                        self.send_delayed_command(PacketCommand::ReadSku, vec![], 600),
                        self.send_delayed_command(PacketCommand::ReadSkuAlt, vec![], 1000),
                        self.send_delayed_command(PacketCommand::RespSku, vec![], 1400),
                        self.send_delayed_command(PacketCommand::ReadFirmware, vec![], 1800),
                        Task::perform(
                            async {
                                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            },
                            |_| Message::IdentificationTimeout,
                        ),
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
            Message::IncCustomEQ(i) => return self.custom_eq_delta(i, 1.0),
            Message::DecCustomEQ(i) => return self.custom_eq_delta(i, -1.0),
            Message::ScrollCustomEQ(i, delta) => return self.custom_eq_delta(i, delta as f32),
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

            Message::WindowId(id) => {
                self.window_id = id;
            }
            Message::CloseRequested(_id) => {
                if let Some(id) = self.window_id {
                    return iced::window::minimize(id, true);
                }
            }
            Message::Tray(TrayEvent::Quit) => {
                std::process::exit(0);
            }
            Message::Tray(TrayEvent::Show) => {
                if let Some(id) = self.window_id {
                    return Task::batch([
                        iced::window::minimize(id, false),
                        iced::window::gain_focus(id),
                        iced::window::request_user_attention(
                            id,
                            Some(iced::window::UserAttention::Critical),
                        ),
                    ]);
                }
            }
            Message::Tray(TrayEvent::Hide) => {
                if let Some(id) = self.window_id {
                    return iced::window::minimize(id, true);
                }
            }
            Message::BatteryRefreshTick => {
                if let Some(_d) = &self.connected_device {
                    if matches!(self.state, AppState::Connected(_)) {
                        return self.send_command(PacketCommand::ReadBattery, vec![]);
                    }
                }
            }

            _ => {}
        }
        Task::none()
    }

    fn custom_eq_delta(&mut self, i: usize, delta: f32) -> Task<Message> {
        let mut payload = None;
        if let Some(d) = &mut self.connected_device {
            if i < 3 && delta != 0.0 && EarNative::supports_custom_eq(&d.model) {
                d.custom_eq[i] = (d.custom_eq[i] + delta).clamp(-6.0, 6.0);
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
        Task::none()
    }

    pub fn handle_packet(&mut self, packet: crate::protocol::Packet) -> Task<Message> {
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

                    crate::tray::update_battery(
                        device.battery_left,
                        device.battery_case,
                        device.battery_right,
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
