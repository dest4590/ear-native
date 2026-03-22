use bytes::{Buf, BytesMut};
use tokio::{sync::mpsc, time};

use crate::protocol::Packet;

mod traits;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

pub use traits::{
    BluetoothAdapter, BluetoothResult, BluetoothStream, DiscoveredDevice, StreamRead,
};

const MAX_RX_BUF: usize = 8192;

#[derive(Debug, Clone)]
pub enum BluetoothEvent {
    DeviceDiscovered(DiscoveredDevice),
    Connected(String),
    Disconnected,
    PacketReceived(Packet),
    Error(String),
}

#[derive(Debug, Clone)]
pub enum ManagerCommand {
    Connect(String),
    Disconnect,
    SendPacket(Packet),
}

pub struct BluetoothManager {
    adapter: Box<dyn BluetoothAdapter>,
    tx: mpsc::Sender<BluetoothEvent>,
    cmd_rx: mpsc::Receiver<ManagerCommand>,
}

impl BluetoothManager {
    pub fn new(
        adapter: Box<dyn BluetoothAdapter>,
        tx: mpsc::Sender<BluetoothEvent>,
        cmd_rx: mpsc::Receiver<ManagerCommand>,
    ) -> Self {
        Self {
            adapter,
            tx,
            cmd_rx,
        }
    }

    pub async fn start_discovery(&self) -> BluetoothResult<()> {
        self.adapter.start_discovery(self.tx.clone()).await
    }

    pub async fn run(mut self) -> BluetoothResult<()> {
        let mut current_stream: Option<Box<dyn BluetoothStream>> = None;
        let mut rx_buf = BytesMut::new();
        let mut monitor = time::interval(std::time::Duration::from_secs(5));

        loop {
            tokio::select! {
                Some(cmd) = self.cmd_rx.recv() => {
                    match cmd {
                        ManagerCommand::Connect(id) => {
                            close_stream(&mut current_stream).await;
                            rx_buf.clear();

                            match self.adapter.connect(&id).await {
                                Ok(stream) => {
                                    current_stream = Some(stream);
                                    let _ = self.tx.send(BluetoothEvent::Connected(id)).await;
                                }
                                Err(error) => {
                                    let _ = self.tx.send(BluetoothEvent::Error(format!("Connect err: {}", error))).await;
                                }
                            }
                        }
                        ManagerCommand::Disconnect => {
                            close_stream(&mut current_stream).await;
                            rx_buf.clear();
                            let _ = self.tx.send(BluetoothEvent::Disconnected).await;
                        }
                        ManagerCommand::SendPacket(packet) => {
                            if let Some(stream) = &mut current_stream {
                                if let Err(error) = stream.send(&packet).await {
                                    log::error!("BT write error: {}", error);
                                    close_stream(&mut current_stream).await;
                                    rx_buf.clear();
                                    let _ = self.tx.send(BluetoothEvent::Error(format!("Write err: {}", error))).await;
                                    let _ = self.tx.send(BluetoothEvent::Disconnected).await;
                                }
                            }
                        }
                    }
                }
                _ = monitor.tick() => {
                    match self.adapter.paired_devices().await {
                        Ok(devices) => {
                            for device in devices {
                                let _ = self.tx.send(BluetoothEvent::DeviceDiscovered(device)).await;
                            }
                        }
                        Err(error) => {
                            log::warn!("Bluetooth monitor unavailable: {}", error);
                        }
                    }
                }
                result = async {
                    if let Some(stream) = &mut current_stream {
                        stream.read().await
                    } else {
                        futures::future::pending::<BluetoothResult<StreamRead>>().await
                    }
                } => {
                    match result {
                        Ok(StreamRead::Data(data)) => {
                            rx_buf.extend_from_slice(&data);
                            let tx = &self.tx;
                            drain_packets(&mut rx_buf, |packet| {
                                log::info!(
                                    "Received packet: cmd=0x{:04x}, cmd-decimal={}, payload=[{}]",
                                    packet.raw_command,
                                    packet.raw_command,
                                    packet.payload.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(", ")
                                );
                                let _ = tx.try_send(BluetoothEvent::PacketReceived(packet));
                            });
                        }
                        Ok(StreamRead::Closed) => {
                            close_stream(&mut current_stream).await;
                            rx_buf.clear();
                            let _ = self.tx.send(BluetoothEvent::Disconnected).await;
                        }
                        Err(error) => {
                            log::error!("BT read error: {}", error);
                            close_stream(&mut current_stream).await;
                            rx_buf.clear();
                            let _ = self.tx.send(BluetoothEvent::Disconnected).await;
                            let _ = self.tx.send(BluetoothEvent::Error(format!("Read err: {}", error))).await;
                        }
                    }
                }
            }
        }
    }
}

async fn close_stream(current_stream: &mut Option<Box<dyn BluetoothStream>>) {
    if let Some(mut stream) = current_stream.take() {
        let _ = stream.close().await;
    }
}

fn drain_packets(buf: &mut BytesMut, mut emit: impl FnMut(Packet)) {
    if buf.len() > MAX_RX_BUF {
        let excess = buf.len() - MAX_RX_BUF;
        log::warn!(
            "rx_buf exceeded {} bytes, discarding {} stale bytes",
            MAX_RX_BUF,
            excess
        );
        buf.advance(excess);
    }

    loop {
        let Some(start) = buf.iter().position(|byte| *byte == Packet::FRAME_MARKER) else {
            buf.clear();
            break;
        };

        if start > 0 {
            buf.advance(start);
        }

        let Some(total_len) = Packet::encoded_len(buf.as_ref()) else {
            break;
        };

        if buf.len() < total_len {
            break;
        }

        if let Some(packet) = Packet::from_bytes(&buf[..total_len]) {
            emit(packet);
            buf.advance(total_len);
        } else {
            buf.advance(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::PacketCommand;

    #[test]
    fn drain_packets_skips_noise_and_keeps_partial_tail() {
        let packet = Packet::new(PacketCommand::ReadBattery, vec![0x01, 0x02], 7).to_bytes();
        let mut buffer = BytesMut::from(&[0x00, 0x01][..]);
        buffer.extend_from_slice(&packet);
        buffer.extend_from_slice(&packet[..6]);

        let mut emitted = Vec::new();
        drain_packets(&mut buffer, |packet| emitted.push(packet));

        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].command, PacketCommand::ReadBattery);
        assert_eq!(buffer.as_ref(), &packet[..6]);
    }
}

#[cfg(target_os = "linux")]
pub use linux::create_adapter;
#[cfg(target_os = "windows")]
pub use windows::create_adapter;

#[cfg(target_os = "macos")]
compile_error!("macOS is not yet supported. Please use Linux or Windows.");
