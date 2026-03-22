use crate::protocol::Packet;

const MAX_RX_BUF: usize = 8192;

#[derive(Debug, Clone)]
pub enum BluetoothEvent {
    DeviceDiscovered(String, String),
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

fn drain_packets(buf: &mut Vec<u8>, mut emit: impl FnMut(Packet)) {
    if buf.len() > MAX_RX_BUF {
        log::warn!("rx_buf exceeded {} bytes, discarding tail", MAX_RX_BUF);
        buf.truncate(MAX_RX_BUF);
    }

    let mut pos = 0;
    while pos < buf.len() {
        if buf[pos] != 0x55 {
            pos += 1;
            continue;
        }

        if buf.len() - pos < 10 {
            break;
        }

        let payload_len = buf[pos + 5] as usize;
        let total_len = 10 + payload_len;

        if buf.len() - pos < total_len {
            break;
        }

        if let Some(packet) = Packet::from_bytes(&buf[pos..pos + total_len]) {
            emit(packet);
            pos += total_len;
        } else {
            pos += 1;
        }
    }

    if pos > 0 {
        buf.drain(..pos);
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use bluer::{rfcomm, Adapter, AdapterEvent, Address};
    use futures::{pin_mut, StreamExt};
    use std::error::Error;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::mpsc;

    pub struct BluetoothManager {
        adapter: Adapter,
        tx: mpsc::Sender<BluetoothEvent>,
        cmd_rx: mpsc::Receiver<ManagerCommand>,
    }

    impl BluetoothManager {
        pub async fn new(
            tx: mpsc::Sender<BluetoothEvent>,
            cmd_rx: mpsc::Receiver<ManagerCommand>,
        ) -> Result<Self, Box<dyn Error + Send + Sync>> {
            let session = bluer::Session::new().await?;
            let adapter = session.default_adapter().await?;
            adapter.set_powered(true).await?;

            Ok(Self {
                adapter,
                tx,
                cmd_rx,
            })
        }

        pub async fn start_discovery(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
            for addr in self.adapter.device_addresses().await? {
                if let Ok(device) = self.adapter.device(addr) {
                    let name = device.name().await?.unwrap_or_else(|| addr.to_string());
                    let _ = self
                        .tx
                        .send(BluetoothEvent::DeviceDiscovered(addr.to_string(), name))
                        .await;
                }
            }

            let events = self.adapter.discover_devices().await?;
            let tx = self.tx.clone();
            let adapter = self.adapter.clone();

            tokio::spawn(async move {
                pin_mut!(events);
                while let Some(event) = events.next().await {
                    if let AdapterEvent::DeviceAdded(addr) = event {
                        if let Ok(device) = adapter.device(addr) {
                            let mut name = addr.to_string();
                            if let Ok(Some(n)) = device.name().await {
                                name = n;
                            }
                            let _ = tx
                                .send(BluetoothEvent::DeviceDiscovered(addr.to_string(), name))
                                .await;
                        }
                    }
                }
            });

            Ok(())
        }

        pub async fn run(mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
            let mut current_stream: Option<rfcomm::Stream> = None;
            let mut current_addr = String::new();
            let mut buffer = [0u8; 4096];
            let mut rx_buf = Vec::new();

            loop {
                tokio::select! {
                    Some(cmd) = self.cmd_rx.recv() => {
                        match cmd {
                            ManagerCommand::Connect(addr_str) => {
                                let addr: Address = match addr_str.parse() {
                                    Ok(a) => a,
                                    Err(e) => {
                                        let _ = self.tx.send(BluetoothEvent::Error(format!("Invalid address '{}': {}", addr_str, e))).await;
                                        continue;
                                    }
                                };
                                current_stream = None;
                                rx_buf.clear();
                                current_addr = addr_str;
                                match self.connect(addr).await {
                                    Ok(stream) => { current_stream = Some(stream); }
                                    Err(e) => {
                                        let _ = self.tx.send(BluetoothEvent::Error(format!("Connect err: {}", e))).await;
                                    }
                                }
                            }
                            ManagerCommand::Disconnect => {
                                let _ = self.tx.send(BluetoothEvent::Disconnected).await;
                            }
                            ManagerCommand::SendPacket(packet) => {
                                let bytes = packet.to_bytes();
                                if let Some(stream) = &mut current_stream {
                                    if let Err(e) = stream.write_all(&bytes).await {
                                        log::error!("BT write error: {}", e);
                                        let _ = self.tx.send(BluetoothEvent::Error(format!("Write err: {}", e))).await;
                                        current_stream = None;
                                        rx_buf.clear();
                                        let _ = self.tx.send(BluetoothEvent::Disconnected).await;
                                    }
                                }
                            }
                        }
                    }
                    result = async {
                        if let Some(stream) = &mut current_stream {
                            stream.read(&mut buffer).await
                        } else {
                            futures::future::pending().await
                        }
                    } => {
                        match result {
                            Ok(0) => {
                                current_stream = None;
                                rx_buf.clear();
                                let _ = self.tx.send(BluetoothEvent::Disconnected).await;
                            }
                            Ok(n) => {
                                rx_buf.extend_from_slice(&buffer[..n]);
                                let tx = &self.tx;
                                drain_packets(&mut rx_buf, |packet| {
                                    let _ = tx.try_send(BluetoothEvent::PacketReceived(packet));
                                });
                            }
                            Err(e) => {
                                log::error!("BT read error: {}", e);
                                current_stream = None;
                                rx_buf.clear();
                                let _ = self.tx.send(BluetoothEvent::Disconnected).await;
                                let _ = self.tx.send(BluetoothEvent::Error(format!("Read err: {}", e))).await;
                            }
                        }
                    }
                }
            }
        }

        async fn connect(
            &mut self,
            addr: Address,
        ) -> Result<rfcomm::Stream, Box<dyn Error + Send + Sync>> {
            let device = self.adapter.device(addr)?;
            if !device.is_connected().await.unwrap_or(false) {
                let _ = device.connect().await;
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

            let mut stream = None;
            for c in 1..=30 {
                if let Ok(socket) = rfcomm::Socket::new() {
                    if let Ok(s) = socket.connect(rfcomm::SocketAddr::new(addr, c)).await {
                        stream = Some(s);
                        break;
                    }
                }
            }

            if let Some(s) = stream {
                let _ = self
                    .tx
                    .send(BluetoothEvent::Connected(addr.to_string()))
                    .await;
                Ok(s)
            } else {
                Err("Failed to connect to any RFCOMM channel".into())
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use std::error::Error as StdError;
    use tokio::sync::mpsc;
    use windows::{
        core::*,
        Devices::Bluetooth::BluetoothDevice,
        Devices::Enumeration::DeviceInformation,
        Networking::Sockets::StreamSocket,
        Storage::Streams::{DataReader, DataWriter, InputStreamOptions},
    };

    pub struct BluetoothManager {
        tx: mpsc::Sender<BluetoothEvent>,
        cmd_rx: mpsc::Receiver<ManagerCommand>,
    }

    impl BluetoothManager {
        pub async fn new(
            tx: mpsc::Sender<BluetoothEvent>,
            cmd_rx: mpsc::Receiver<ManagerCommand>,
        ) -> std::result::Result<Self, Box<dyn StdError + Send + Sync>> {
            Ok(Self { tx, cmd_rx })
        }

        pub async fn start_discovery(
            &self,
        ) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
            let selector = BluetoothDevice::GetDeviceSelectorFromPairingState(true)
                .map_err(|e| format!("Failed to get device selector: {:?}", e))?;
            let tx = self.tx.clone();

            tokio::spawn(async move {
                if let Ok(operation) = DeviceInformation::FindAllAsyncAqsFilter(&selector) {
                    if let Ok(devices) = operation.await {
                        if let Ok(size) = devices.Size() {
                            for i in 0..size {
                                if let Ok(device_info) = devices.GetAt(i) {
                                    let name = device_info
                                        .Name()
                                        .map(|h: HSTRING| h.to_string())
                                        .unwrap_or_default();
                                    let id = device_info
                                        .Id()
                                        .map(|h: HSTRING| h.to_string())
                                        .unwrap_or_default();
                                    let _ =
                                        tx.send(BluetoothEvent::DeviceDiscovered(id, name)).await;
                                }
                            }
                        }
                    }
                }
            });
            Ok(())
        }

        pub async fn run(mut self) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
            let mut current_socket: Option<StreamSocket> = None;
            let mut current_writer: Option<DataWriter> = None;
            let mut reader_abort: Option<tokio::task::AbortHandle> = None;
            let (read_tx, mut read_rx) = mpsc::channel::<Option<Vec<u8>>>(100);
            let mut rx_buf: Vec<u8> = Vec::new();

            loop {
                tokio::select! {
                    Some(cmd) = self.cmd_rx.recv() => {
                        match cmd {
                            ManagerCommand::Connect(device_id) => {
                                if let Some(handle) = reader_abort.take() {
                                    handle.abort();
                                }
                                if let Some(socket) = current_socket.take() {
                                    let _ = socket.Close();
                                }
                                current_writer = None;
                                rx_buf.clear();

                                match Self::connect_device(&device_id).await {
                                    Ok((socket, writer, reader)) => {
                                        current_socket = Some(socket);
                                        current_writer = Some(writer);

                                        let read_tx_clone = read_tx.clone();
                                        let handle = tokio::spawn(async move {
                                            loop {
                                                match reader.LoadAsync(4096) {
                                                    Ok(op) => {
                                                        match op.await {
                                                            Ok(loaded) if loaded > 0 => {
                                                                let mut buffer = vec![0u8; loaded as usize];
                                                                if reader.ReadBytes(&mut buffer).is_ok() {
                                                                    if read_tx_clone.send(Some(buffer)).await.is_err() {
                                                                        break;
                                                                    }
                                                                } else {
                                                                    break;
                                                                }
                                                            }
                                                            Ok(_) | Err(_) => {
                                                                let _ = read_tx_clone.send(None).await;
                                                                break;
                                                            }
                                                        }
                                                    }
                                                    Err(_) => {
                                                        let _ = read_tx_clone.send(None).await;
                                                        break;
                                                    }
                                                }
                                            }
                                        });
                                        reader_abort = Some(handle.abort_handle());

                                        let _ = self.tx.send(BluetoothEvent::Connected(device_id)).await;
                                    }
                                    Err(e) => {
                                        log::error!("BT connect error: {}", e);
                                        let _ = self.tx.send(BluetoothEvent::Error(e.to_string())).await;
                                    }
                                }
                            }
                            ManagerCommand::Disconnect => {
                                if let Some(handle) = reader_abort.take() {
                                    handle.abort();
                                }
                                if let Some(socket) = current_socket.take() {
                                    let _ = socket.Close();
                                }
                                current_writer = None;
                                rx_buf.clear();
                                let _ = self.tx.send(BluetoothEvent::Disconnected).await;
                            }
                            ManagerCommand::SendPacket(packet) => {
                                let bytes = packet.to_bytes();
                                if let Some(writer) = &current_writer {
                                    let store_ok = if writer.WriteBytes(&bytes).is_ok() {
                                        match writer.StoreAsync() {
                                            Ok(op) => op.await.is_ok(),
                                            Err(_) => false,
                                        }
                                    } else {
                                        false
                                    };
                                    if !store_ok {
                                        log::error!("BT write failed, disconnecting");
                                        if let Some(handle) = reader_abort.take() {
                                            handle.abort();
                                        }
                                        if let Some(socket) = current_socket.take() {
                                            let _ = socket.Close();
                                        }
                                        current_writer = None;
                                        rx_buf.clear();
                                        let _ = self.tx.send(BluetoothEvent::Error("Write failed".into())).await;
                                        let _ = self.tx.send(BluetoothEvent::Disconnected).await;
                                    }
                                }
                            }
                        }
                    }
                    Some(msg) = read_rx.recv() => {
                        match msg {
                            Some(data) => {
                                rx_buf.extend_from_slice(&data);
                                let tx = &self.tx;
                                drain_packets(&mut rx_buf, |packet| {
                                    let _ = tx.try_send(BluetoothEvent::PacketReceived(packet));
                                });
                            }
                            None => {
                                reader_abort = None;
                                if let Some(socket) = current_socket.take() {
                                    let _ = socket.Close();
                                }
                                current_writer = None;
                                rx_buf.clear();
                                let _ = self.tx.send(BluetoothEvent::Disconnected).await;
                            }
                        }
                    }
                }
            }
        }

        async fn connect_device(
            device_id: &str,
        ) -> std::result::Result<
            (StreamSocket, DataWriter, DataReader),
            Box<dyn StdError + Send + Sync>,
        > {
            let device_id_hstring = HSTRING::from(device_id);
            let device = BluetoothDevice::FromIdAsync(&device_id_hstring)?.await?;
            let rfcomm_result = device.GetRfcommServicesAsync()?.await?;

            let (hostname, service_name) = {
                let services_vector = rfcomm_result.Services()?;
                let service_count = services_vector.Size()?;

                if service_count == 0 {
                    return Err("No RFCOMM services found".into());
                }

                let mut spp_service = None;
                let mut fallback_service = None;

                for i in 0..service_count {
                    if let Ok(service) = services_vector.GetAt(i) {
                        if let Ok(id) = service.ServiceId() {
                            let uuid = id
                                .Uuid()
                                .map(|u| format!("{:?}", u))
                                .unwrap_or_default()
                                .to_lowercase();
                            if uuid.contains("aeac") {
                                spp_service = Some(service.clone());
                                break;
                            } else if uuid.contains("1101") {
                                fallback_service = Some(service.clone());
                            }
                        }
                    }
                }

                let service = spp_service
                    .or(fallback_service)
                    .unwrap_or_else(|| services_vector.GetAt(0).unwrap());

                let hostname = service.ConnectionHostName()?;
                let service_name = service.ConnectionServiceName()?;
                (hostname, service_name)
            };

            let socket = StreamSocket::new()?;
            socket.ConnectAsync(&hostname, &service_name)?.await?;

            let output_stream = socket.OutputStream()?;
            let input_stream = socket.InputStream()?;
            let writer = DataWriter::CreateDataWriter(&output_stream)?;
            let reader = DataReader::CreateDataReader(&input_stream)?;

            let _ = reader.SetInputStreamOptions(InputStreamOptions::Partial);
            Ok((socket, writer, reader))
        }
    }
}

#[cfg(target_os = "macos")]
compile_error!("macOS is not yet supported. Please use Linux or Windows.");

pub use platform::BluetoothManager;
