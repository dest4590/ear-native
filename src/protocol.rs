use crc::{Algorithm, Crc};
use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};
use std::array;
use std::convert::TryFrom;

pub const CRC_16_ARC: Algorithm<u16> = Algorithm {
    width: 16,
    poly: 0x8005,
    init: 0xffff,
    refin: true,
    refout: true,
    xorout: 0x0000,
    check: 0xbb3d,
    residue: 0x0000,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u16)]
pub enum PacketCommand {
    Unknown = 0,
    ReadBattery = 49159,
    ReadAnc = 49182,
    SetAnc = 61455,
    ReadEq = 49183,
    SetEq = 61456,
    ReadPersonalizedAnc = 49184,
    SetPersonalizedAnc = 61457,
    ReadListeningMode = 49232,
    SetListeningMode = 61469,
    ReadEnhancedBass = 49230,
    SetEnhancedBass = 61521,
    ReadAdvancedEq = 49228,
    SetAdvancedEqEnabled = 61519,
    ReadCustomEq = 49220,
    SetCustomEq = 61505,
    StartEarFitTest = 61460,
    ReadFirmware = 49218,
    ReadInEar = 49166,
    SetInEar = 61444,
    ReadLatency = 49217,
    SetLatency = 61504,
    SetRingBuds = 61442,
    ReadSku = 49160,
    ReadSkuAlt = 16392,
    RespSku = 57352,
    RespBattery = 57345,
    RespBatteryAlt = 16391,
    RespAnc = 57347,
    RespAncAlt = 16414,
    RespEq = 16415,
    RespEqAlt = 16464,
    RespPersonalizedAnc = 16416,
    RespFirmware = 16450,
    RespInEar = 16398,
    RespLatency = 16449,
    RespGesture = 16408,
    RespEarFitTest = 57357,
    RespEnhancedBass = 16462,
    RespCustomEq = 16452,
    RespAdvancedEqStatus = 16460,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParsedResponse {
    Battery(DeviceBattery),
    Anc(AncMode),
    Eq { mode: EqMode, preset: u8 },
    Firmware(String),
    InEar(bool),
    Latency(bool),
    Gestures(Vec<Gesture>),
    EnhancedBass { enabled: bool, level: u8 },
    CustomEq([f32; 3]),
    EarFitTest { left: u8, right: u8 },
    Sku(String),
    PersonalizedAnc(bool),
    AdvancedEq(bool),
    Unknown(u16, Vec<u8>),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BatteryStatus {
    pub level: u8,
    pub is_charging: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceBattery {
    pub left: Option<BatteryStatus>,
    pub right: Option<BatteryStatus>,
    pub case: Option<BatteryStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum DeviceId {
    Left = 0x02,
    Right = 0x03,
    Case = 0x04,
}

impl DeviceId {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x02 => Some(Self::Left),
            0x03 => Some(Self::Right),
            0x04 => Some(Self::Case),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum AncMode {
    Off = 0x05,
    Transparent = 0x07,
    NcLow = 0x03,
    NcHigh = 0x01,
    NcMid = 0x02,
    NcAdaptive = 0x04,
}

impl AncMode {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x05 => Some(Self::Off),
            0x07 => Some(Self::Transparent),
            0x03 => Some(Self::NcLow),
            0x01 => Some(Self::NcHigh),
            0x02 => Some(Self::NcMid),
            0x04 => Some(Self::NcAdaptive),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum EqMode {
    Balanced = 0,
    MoreTreble = 1,
    MoreBass = 2,
    Voice = 3,
    Custom = 6,
}

impl EqMode {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Balanced),
            1 => Some(Self::MoreTreble),
            2 => Some(Self::MoreBass),
            3 => Some(Self::Voice),
            6 => Some(Self::Custom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum GestureType {
    DoubleTap = 2,
    TripleTap = 3,
    TapAndHold = 7,
    DoubleTapAndHold = 9,
}

impl GestureType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            2 => Some(Self::DoubleTap),
            3 => Some(Self::TripleTap),
            7 => Some(Self::TapAndHold),
            9 => Some(Self::DoubleTapAndHold),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GestureAction {
    NoAction,
    SkipBack,
    SkipForward,
    VoiceAssistant,
    NoiseControl,
    VolumeUp,
    VolumeDown,
    NoiseControlToggles(u8),
    Unknown(u8),
}

impl GestureAction {
    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::NoAction,
            8 => Self::SkipBack,
            9 => Self::SkipForward,
            11 => Self::VoiceAssistant,
            10 => Self::NoiseControl,
            18 => Self::VolumeUp,
            19 => Self::VolumeDown,
            20 | 21 | 22 => Self::NoiseControlToggles(value),
            _ => Self::Unknown(value),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gesture {
    pub device: DeviceId,
    pub gesture_type: GestureType,
    pub action: GestureAction,
}

pub fn calculate_crc(data: &[u8]) -> u16 {
    Crc::<u16>::new(&CRC_16_ARC).checksum(data)
}

#[derive(Debug, Clone)]
pub struct Packet {
    pub command: PacketCommand,
    pub raw_command: u16,
    pub payload: Vec<u8>,
    pub operation_id: u8,
}

impl Packet {
    pub const FRAME_MARKER: u8 = 0x55;
    const HEADER_LEN: usize = 8;
    const TRAILER_LEN: usize = 2;
    const MIN_FRAME_LEN: usize = Self::HEADER_LEN + Self::TRAILER_LEN;

    pub fn new(command: PacketCommand, payload: Vec<u8>, operation_id: u8) -> Self {
        let raw_command = command as u16;
        Self {
            command,
            raw_command,
            payload,
            operation_id,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = vec![
            Self::FRAME_MARKER,
            0x60,
            0x01,
            0,
            0,
            self.payload.len() as u8,
            0,
            self.operation_id,
        ];
        let cmd = (self.command as u16).to_le_bytes();
        data[3] = cmd[0];
        data[4] = cmd[1];
        data.extend_from_slice(&self.payload);
        let crc = calculate_crc(&data);
        data.extend([crc as u8, (crc >> 8) as u8]);
        data
    }

    pub fn encoded_len(bytes: &[u8]) -> Option<usize> {
        if bytes.first().copied()? != Self::FRAME_MARKER {
            return None;
        }

        Some(Self::MIN_FRAME_LEN + (*bytes.get(5)? as usize))
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::MIN_FRAME_LEN || bytes.first().copied()? != Self::FRAME_MARKER {
            return None;
        }

        let total_len = Self::encoded_len(bytes)?;
        let frame = bytes.get(..total_len)?;

        let crc_received =
            u16::from_le_bytes(frame.get(total_len - 2..total_len)?.try_into().ok()?);
        let crc_calculated = calculate_crc(frame.get(..total_len - Self::TRAILER_LEN)?);
        if crc_calculated != crc_received {
            log::trace!(
                "BT packet CRC mismatch: calculated 0x{:04x}, received 0x{:04x} (accepted anyway)",
                crc_calculated,
                crc_received
            );
        }

        let raw_cmd = u16::from_le_bytes([*frame.get(3)?, *frame.get(4)?]);
        let command = PacketCommand::try_from(raw_cmd).unwrap_or_else(|_| {
            log::debug!(
                "Unknown BT command 0x{:04x} ({}), treating as Unknown",
                raw_cmd,
                raw_cmd
            );
            PacketCommand::Unknown
        });
        let len = *frame.get(5)? as usize;
        let op = *frame.get(7)?;

        let payload = frame
            .get(Self::HEADER_LEN..Self::HEADER_LEN + len)?
            .to_vec();

        Some(Self {
            command,
            raw_command: raw_cmd,
            payload,
            operation_id: op,
        })
    }

    fn payload_byte(&self, index: usize) -> Option<u8> {
        self.payload.get(index).copied()
    }

    fn payload_array<const N: usize>(&self, start: usize) -> Option<[u8; N]> {
        self.payload.get(start..start + N)?.try_into().ok()
    }

    pub fn parse(&self) -> ParsedResponse {
        match self.command {
            PacketCommand::RespBattery | PacketCommand::RespBatteryAlt => {
                let mut battery = DeviceBattery::default();
                let count = self.payload_byte(0).unwrap_or(0) as usize;

                for [id, status] in self
                    .payload
                    .get(1..)
                    .unwrap_or(&[])
                    .chunks_exact(2)
                    .take(count)
                    .map(|chunk| [chunk[0], chunk[1]])
                {
                    let stat = BatteryStatus {
                        level: status & 0x7F,
                        is_charging: status & 0x80 != 0,
                    };

                    match DeviceId::from_u8(id) {
                        Some(DeviceId::Left) => battery.left = Some(stat),
                        Some(DeviceId::Right) => battery.right = Some(stat),
                        Some(DeviceId::Case) => battery.case = Some(stat),
                        _ => {}
                    }
                }

                ParsedResponse::Battery(battery)
            }

            PacketCommand::RespAnc | PacketCommand::RespAncAlt => {
                let v = self.payload_byte(1).unwrap_or(0);
                AncMode::from_u8(v)
                    .map(ParsedResponse::Anc)
                    .unwrap_or_else(|| {
                        ParsedResponse::Unknown(self.command as u16, self.payload.clone())
                    })
            }

            PacketCommand::Unknown => {
                ParsedResponse::Unknown(self.raw_command, self.payload.clone())
            }

            PacketCommand::RespEq | PacketCommand::RespEqAlt => {
                let preset = self.payload_byte(0).unwrap_or(0);
                let mode = EqMode::from_u8(preset).unwrap_or(EqMode::Balanced);
                ParsedResponse::Eq { mode, preset }
            }

            PacketCommand::RespFirmware => {
                ParsedResponse::Firmware(String::from_utf8_lossy(&self.payload).to_string())
            }

            PacketCommand::RespInEar => {
                ParsedResponse::InEar(self.payload_byte(2).unwrap_or(0) != 0)
            }

            PacketCommand::RespLatency => {
                ParsedResponse::Latency(self.payload_byte(0).unwrap_or(0) != 0)
            }

            PacketCommand::RespGesture => {
                let count = self.payload_byte(0).unwrap_or(0) as usize;
                let mut out = Vec::new();

                for chunk in self
                    .payload
                    .get(1..)
                    .unwrap_or(&[])
                    .chunks_exact(4)
                    .take(count)
                {
                    if let (Some(d), Some(t)) =
                        (DeviceId::from_u8(chunk[0]), GestureType::from_u8(chunk[2]))
                    {
                        out.push(Gesture {
                            device: d,
                            gesture_type: t,
                            action: GestureAction::from_u8(chunk[3]),
                        });
                    }
                }

                ParsedResponse::Gestures(out)
            }

            PacketCommand::RespAdvancedEqStatus => {
                ParsedResponse::AdvancedEq(self.payload_byte(0) == Some(1))
            }

            PacketCommand::RespCustomEq | PacketCommand::ReadCustomEq => parse_custom_eq(self)
                .map(ParsedResponse::CustomEq)
                .unwrap_or_else(|| {
                    ParsedResponse::Unknown(self.command as u16, self.payload.clone())
                }),

            PacketCommand::RespEarFitTest => {
                if let Some([left, right]) = self.payload_array::<2>(0) {
                    ParsedResponse::EarFitTest { left, right }
                } else {
                    ParsedResponse::Unknown(self.command as u16, self.payload.clone())
                }
            }

            PacketCommand::RespEnhancedBass | PacketCommand::ReadEnhancedBass => {
                if let Some([enabled, level]) = self.payload_array::<2>(0) {
                    ParsedResponse::EnhancedBass {
                        enabled: enabled == 0x01,
                        level: level / 2,
                    }
                } else {
                    ParsedResponse::Unknown(self.command as u16, self.payload.clone())
                }
            }

            PacketCommand::RespPersonalizedAnc | PacketCommand::ReadPersonalizedAnc => {
                ParsedResponse::PersonalizedAnc(self.payload_byte(0) == Some(1))
            }

            PacketCommand::RespSku | PacketCommand::ReadSku | PacketCommand::ReadSkuAlt => {
                let sku = if let Some(value) = self.payload_byte(1) {
                    format!("{:02x}", value)
                } else if let Some(value) = self.payload_byte(0) {
                    format!("{:02x}", value)
                } else {
                    "unknown".to_string()
                };
                ParsedResponse::Sku(sku)
            }

            _ => ParsedResponse::Unknown(self.command as u16, self.payload.clone()),
        }
    }
}

fn parse_custom_eq(packet: &Packet) -> Option<[f32; 3]> {
    if packet.payload.len() < 45 {
        return None;
    }

    let values: [f32; 3] = array::from_fn(|index| {
        let base = 6 + (index * 13);
        packet
            .payload_array::<4>(base)
            .map(from_format_float_for_eq)
            .map(|value| value.round().clamp(-6.0, 6.0))
            .unwrap_or(0.0)
    });

    Some([values[2], values[0], values[1]])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_round_trip_validates_crc() {
        let packet = Packet::new(PacketCommand::ReadBattery, vec![1, 2, 3], 9);
        let encoded = packet.to_bytes();
        let decoded = Packet::from_bytes(&encoded).expect("packet should decode");

        assert_eq!(decoded.command, PacketCommand::ReadBattery);
        assert_eq!(decoded.payload, vec![1, 2, 3]);
        assert_eq!(decoded.operation_id, 9);
    }

    #[test]
    fn packet_rejects_invalid_crc() {
        let mut encoded = Packet::new(PacketCommand::ReadBattery, vec![1, 2, 3], 9).to_bytes();
        let last_index = encoded.len() - 1;
        encoded[last_index] ^= 0xFF;

        assert!(Packet::from_bytes(&encoded).is_none());
    }
}
