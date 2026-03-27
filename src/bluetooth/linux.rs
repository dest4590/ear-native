use async_trait::async_trait;
use bluer::{rfcomm, Adapter, AdapterEvent, Address};
use futures::{pin_mut, StreamExt};
use log::{info, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::protocol::Packet;

use super::{
    BluetoothAdapter, BluetoothEvent, BluetoothResult, BluetoothStream, DiscoveredDevice,
    StreamRead,
};

pub async fn create_adapter() -> BluetoothResult<Box<dyn BluetoothAdapter>> {
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;

    if !adapter.is_powered().await? {
        adapter.set_powered(true).await?;
    }

    Ok(Box::new(LinuxBluetoothAdapter { adapter }))
}

pub struct LinuxBluetoothAdapter {
    adapter: Adapter,
}

#[async_trait]
impl BluetoothAdapter for LinuxBluetoothAdapter {
    async fn start_discovery(&self, tx: mpsc::Sender<BluetoothEvent>) -> BluetoothResult<()> {
        self.adapter.set_powered(true).await?;

        for addr in self.adapter.device_addresses().await? {
            if let Ok(device) = self.adapter.device(addr) {
                let name = device.name().await?.unwrap_or_else(|| addr.to_string());
                let _ = tx
                    .send(BluetoothEvent::DeviceDiscovered(DiscoveredDevice {
                        id: addr.to_string(),
                        name,
                        paired: true,
                        system_connected: device.is_connected().await.unwrap_or(false),
                    }))
                    .await;
            }
        }

        let events = self.adapter.discover_devices().await?;
        let adapter = self.adapter.clone();

        tokio::spawn(async move {
            pin_mut!(events);
            while let Some(event) = events.next().await {
                if let AdapterEvent::DeviceAdded(addr) = event {
                    if let Ok(device) = adapter.device(addr) {
                        let mut name = addr.to_string();
                        if let Ok(Some(device_name)) = device.name().await {
                            name = device_name;
                        }

                        let _ = tx
                            .send(BluetoothEvent::DeviceDiscovered(DiscoveredDevice {
                                id: addr.to_string(),
                                name,
                                paired: true,
                                system_connected: device.is_connected().await.unwrap_or(false),
                            }))
                            .await;
                    }
                }
            }
        });

        Ok(())
    }

    async fn paired_devices(&self) -> BluetoothResult<Vec<DiscoveredDevice>> {
        self.adapter.set_powered(true).await?;

        let mut devices = Vec::new();

        for addr in self.adapter.device_addresses().await? {
            if let Ok(device) = self.adapter.device(addr) {
                devices.push(DiscoveredDevice {
                    id: addr.to_string(),
                    name: device.name().await?.unwrap_or_else(|| addr.to_string()),
                    paired: true,
                    system_connected: device.is_connected().await.unwrap_or(false),
                });
            }
        }

        Ok(devices)
    }

    /// Connects to a Bluetooth device using RFCOMM, with special handling for the "Audio Profile Trap" issue on Linux.
    ///
    /// ### The "Audio Profile Trap" Problem (Linux Specific)
    /// On Linux, the Bluetooth daemon (BlueZ) often intercepts "standard" RFCOMM channels
    /// for system profiles like Hands-Free (HFP) or Headset (HSP). On CMF Buds Pro 2,
    /// Channel 12 is usually assigned to HFP.
    ///
    /// If we connect to Channel 12:
    /// 1. The connection succeeds, and we can send bytes.
    /// 2. The Buds receive the bytes but don't recognize them as HFP AT-commands.
    /// 3. BlueZ intercepts any incoming response bytes before they reach our socket.
    /// 4. Our app hits a "Read Timeout" even though the hardware is physically connected.
    ///
    /// ### The Solution: Vendor Channel Probing
    /// Nothing/CMF devices expose a proprietary "Nothing Protocol" (Service UUID `aeac`)
    /// usually on Channel 13, 14, or 15. This function probes these specific candidates
    /// to find the "Vendor Command Channel" which BlueZ does not intercept.
    ///
    /// ### Hardware Timing & OS Error 111
    /// Unlike Windows, Linux raw RFCOMM sockets communicate very closely with the hardware.
    /// 1. **Probing:** We use a short timeout for probes to find open ports quickly.
    /// 2. **Cooldown:** We MUST sleep (approx 500-800ms) after probing. If we attempt a
    ///    "real" connection too quickly after a probe disconnect, the Buds' Bluetooth
    ///    controller will return `ECONNREFUSED` (OS Error 111).
    /// 3. **Prioritization:** We prefer Channel 15 and 13 as they are the most common
    ///    assignments for the B172 (Buds Pro 2) SKU.
    async fn connect(&self, id: &str) -> BluetoothResult<Box<dyn BluetoothStream>> {
        let addr: Address = id.parse()?;
        let device = self.adapter.device(addr)?;

        log::info!("Connecting to CMF Buds at {}", addr);

        if !device.is_connected().await.unwrap_or(false) {
            let _ =
                tokio::time::timeout(tokio::time::Duration::from_secs(5), device.connect()).await;
        }

        let mut target_channel = 15;
        let candidates = [15, 13, 14, 16];
        let mut open_channels = Vec::new();

        info!("Probing for CMF Command Channel...");
        for &channel in &candidates {
            if let Ok(socket) = rfcomm::Socket::new() {
                let sa = rfcomm::SocketAddr::new(addr, channel);
                if let Ok(Ok(_)) = tokio::time::timeout(
                    tokio::time::Duration::from_millis(300),
                    socket.connect(sa),
                )
                .await
                {
                    log::info!("Channel {} is open", channel);
                    open_channels.push(channel);
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        }

        if !open_channels.is_empty() {
            target_channel = open_channels[0];
        } else {
            warn!("No preferred channels found, falling back to Channel 15");
        }

        info!("Targeting Channel {} (Nothing Protocol)", target_channel);

        tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;

        let socket = rfcomm::Socket::new()?;
        let sa = rfcomm::SocketAddr::new(addr, target_channel);

        let stream = tokio::time::timeout(tokio::time::Duration::from_secs(3), socket.connect(sa))
            .await
            .map_err(|_| "Connection timed out during final handshake")??;

        info!(
            "Successfully connected to Nothing Command Channel on {}!",
            target_channel
        );
        Ok(Box::new(LinuxBluetoothStream { stream }))
    }
}

struct LinuxBluetoothStream {
    stream: rfcomm::Stream,
}

#[async_trait]
impl BluetoothStream for LinuxBluetoothStream {
    async fn send(&mut self, packet: &Packet) -> BluetoothResult<()> {
        let bytes = packet.to_bytes();
        // log::info!("Sending bytes: {:02X?}", bytes);
        self.stream.write_all(&bytes).await?;
        Ok(())
    }

    async fn read(&mut self) -> BluetoothResult<StreamRead> {
        let mut buffer = [0u8; 4096];
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(2),
            self.stream.read(&mut buffer),
        )
        .await
        {
            Ok(Ok(0)) => Ok(StreamRead::Closed),
            Ok(Ok(read)) => Ok(StreamRead::Data(buffer[..read].to_vec())),
            Ok(Err(e)) => Err(e.into()),
            Err(_) => {
                println!("Read timed out, assuming stream is closed");
                Ok(StreamRead::Closed)
            }
        }
    }

    async fn close(&mut self) -> BluetoothResult<()> {
        self.stream.shutdown().await?;
        Ok(())
    }
}
