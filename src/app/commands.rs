use futures::SinkExt;
use iced::{time, Subscription, Task};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::{
    app::state::{AppState, InitialDataLoad, Message},
    bluetooth::{create_adapter, BluetoothManager, DiscoveredDevice, ManagerCommand},
    models::preload_model_images_in_background,
    protocol::{Packet, PacketCommand},
    EarNative,
};

impl EarNative {
    pub fn send_command(
        &mut self,
        command: crate::protocol::PacketCommand,
        payload: Vec<u8>,
    ) -> Task<Message> {
        self.send_delayed_command(command, payload, 0)
    }

    pub fn format_float_for_eq(f: f32, total: bool) -> [u8; 4] {
        let mut b = f.to_be_bytes();
        if f != 0.0 && b[0] == 0 && b[1] == 0 && b[2] == 0 {
            b[3] |= 0x80;
        }

        b.reverse();
        if total && f >= 0.0 {
            return [0x00, 0x00, 0x00, 0x80];
        }
        b
    }

    pub fn build_custom_eq_payload(custom_eq: [f32; 3]) -> Vec<u8> {
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
        payload[1..5].copy_from_slice(&top_bytes);

        for (index, band) in custom_eq.into_iter().enumerate() {
            let arr = EarNative::format_float_for_eq(band, false);
            let base = 6 + (index * 13);
            payload[base..(base + 4)].copy_from_slice(&arr);
        }

        payload
    }

    pub fn custom_eq_payload_for_model(
        model: &crate::models::ModelInfo,
        custom_eq: [f32; 3],
    ) -> Vec<u8> {
        if model.base != "B185" {
            return Self::build_custom_eq_payload(custom_eq);
        }

        let mut payload = Self::build_custom_eq_payload([custom_eq[1], custom_eq[2], custom_eq[0]]);
        let highest = custom_eq.into_iter().fold(0.0f32, f32::max);
        payload[1..5].copy_from_slice(&Self::format_float_for_eq(-highest, true));
        payload[14..18].copy_from_slice(&0.7f32.to_le_bytes());
        payload[23..27].copy_from_slice(&6900.0f32.to_le_bytes());
        payload
    }

    pub fn send_custom_eq_commands(&mut self, custom_eq: [f32; 3]) -> Task<Message> {
        let Some(device) = &self.connected_device else {
            return Task::none();
        };
        let payload = Self::custom_eq_payload_for_model(&device.model, custom_eq);
        self.send_command(PacketCommand::SetCustomEq, payload)
    }

    pub fn start_initial_data_load(&mut self, model: crate::models::ModelInfo) -> Task<Message> {
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
                if EarNative::eq_command_for_model(&model) == PacketCommand::SetListeningMode {
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
            if EarNative::supports_in_ear(&model) {
                self.send_delayed_command(PacketCommand::ReadInEar, vec![], 700)
            } else {
                Task::none()
            },
            self.send_delayed_command(PacketCommand::ReadLatency, vec![], 900),
            self.send_delayed_command(PacketCommand::ReadEnhancedBass, vec![], 1100),
            if EarNative::supports_custom_eq(&model)
                && EarNative::supports_command(&model, PacketCommand::ReadAdvancedEq)
            {
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

    pub fn mark_initial_data_loaded(&mut self, update: impl FnOnce(&mut InitialDataLoad)) {
        if let Some(load) = &mut self.initial_data_load {
            update(load);
            if load.is_complete() {
                self.initial_data_load = None;
            }
        }
    }

    pub fn send_delayed_command(
        &mut self,
        command: crate::protocol::PacketCommand,
        payload: Vec<u8>,
        delay_ms: u64,
    ) -> Task<Message> {
        if let Some(device) = &self.connected_device {
            if !EarNative::supports_command(&device.model, command) {
                return Task::none();
            }
        }

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

    pub fn send_manager_command(&self, command: ManagerCommand) -> Task<Message> {
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

    pub fn persist_config(&self) -> Task<Message> {
        let config = self.config.clone();
        Task::perform(async move { config.save() }, Message::ConfigPersisted)
    }

    pub fn maybe_auto_connect(&mut self, device: &DiscoveredDevice) -> Option<Task<Message>> {
        let last_connected = self.config.last_connected_device_id.as_deref()?;

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

    pub fn subscription(&self) -> Subscription<Message> {
        let bluetooth = Subscription::run(|| {
            iced::stream::channel(
                100,
                |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
                    let (tx, mut rx) = mpsc::channel(100);
                    let (cmd_tx, cmd_rx) = mpsc::channel(100);
                    let _ = output.send(Message::Ready(cmd_tx)).await;

                    let mut retry_count = 0;
                    let max_retries_before_notify = 3;
                    let mut notified_failure = false;

                    let manager = loop {
                        match create_adapter().await {
                            Ok(adapter) => {
                                log::info!("Bluetooth adapter initialized successfully");
                                break BluetoothManager::new(adapter, tx.clone(), cmd_rx);
                            }
                            Err(error) => {
                                retry_count += 1;
                                let error_str = error.to_string();

                                if retry_count == 1 {
                                    log::warn!(
                                        "Bluetooth adapter failed to initialize: {}",
                                        error_str
                                    );
                                }

                                if retry_count == max_retries_before_notify && !notified_failure {
                                    notified_failure = true;
                                    let error_msg = if error_str.contains("not present")
                                        || error_str.contains("not found")
                                    {
                                        "Make sure Bluetooth is enabled in your system settings."
                                            .to_string()
                                    } else if error_str.contains("Permission") {
                                        "The application doesn't have permission to access Bluetooth. Check your system permissions.".to_string()
                                    } else {
                                        format!("Please enable Bluetooth: {}", error_str)
                                    };
                                    let _ = output
                                        .send(Message::BluetoothInitializationFailed(error_msg))
                                        .await;
                                }

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

        let is_loading = matches!(
            self.state,
            AppState::Connecting(_) | AppState::Identifying(_)
        ) || self.initial_data_load.is_some()
            || !self.active_model_assets_ready;

        let mut subs = vec![bluetooth];

        if is_loading {
            subs.push(
                time::every(std::time::Duration::from_millis(120)).map(|_| Message::LoadingTick),
            );
        }

        subs.push(iced::window::close_requests().map(Message::CloseRequested));

        subs.push(Subscription::run_with(true, |_id: &bool| {
            iced::stream::channel(
                1,
                |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        while let Some(event) = crate::tray::try_recv() {
                            let _ = output.send(Message::Tray(event)).await;
                        }
                    }
                },
            )
        }));

        if matches!(self.state, AppState::Connected(_)) {
            subs.push(
                time::every(std::time::Duration::from_secs(60))
                    .map(|_| Message::BatteryRefreshTick),
            );
        }

        Subscription::batch(subs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{models::get_models, protocol::ParsedResponse};

    #[test]
    fn b185_custom_eq_uses_reference_band_order_and_parameters() {
        let models = get_models();
        let model = &models["hoothoot_generic"];
        let values = [3.0, -2.0, 1.0];
        let payload = EarNative::custom_eq_payload_for_model(model, values);
        assert_eq!(payload.len(), 53);
        assert_eq!(payload[0], 3);
        assert_eq!(&payload[1..5], &(-3.0f32).to_le_bytes());
        for (index, (filter, gain, frequency, quality)) in [
            (1, -2.0f32, 980.0f32, 0.7f32),
            (2, 1.0f32, 6900.0f32, 1.0f32),
            (0, 3.0f32, 140.0f32, 0.8f32),
        ]
        .into_iter()
        .enumerate()
        {
            let offset = 5 + index * 13;
            assert_eq!(payload[offset], filter);
            assert_eq!(&payload[offset + 1..offset + 5], &gain.to_le_bytes());
            assert_eq!(&payload[offset + 5..offset + 9], &frequency.to_le_bytes());
            assert_eq!(&payload[offset + 9..offset + 13], &quality.to_le_bytes());
        }
        let response = Packet::new(PacketCommand::RespCustomEq, payload, 0);
        assert!(matches!(response.parse(), ParsedResponse::CustomEq(read) if read == values));
        let negative = EarNative::custom_eq_payload_for_model(model, [-3.0, -2.0, -1.0]);
        assert_eq!(&negative[1..5], &[0, 0, 0, 0x80]);
        for model in models.values().filter(|model| model.base != "B185") {
            assert_eq!(
                EarNative::custom_eq_payload_for_model(model, values),
                EarNative::build_custom_eq_payload(values)
            );
        }
    }
}
