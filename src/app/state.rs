use crate::mpsc;
use crate::protocol::EqMode;
use crate::{
    bluetooth::{BluetoothEvent, ManagerCommand},
    config::AppConfig,
    models::ModelInfo,
    EarNative,
};

#[derive(Debug, Clone)]
pub enum Message {
    ActiveModelAssetsPreloaded,
    Bluetooth(BluetoothEvent),
    Connect(String),
    Disconnect,
    IncCustomEQ(usize),
    DecCustomEQ(usize),
    ScrollCustomEQ(usize, i8),
    SetCustomEQLevel(usize, i8),
    LoadingTick,
    InitialDataLoadTimedOut,
    ConfigLoaded(AppConfig),
    ConfigPersisted(Result<(), String>),
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
    CommandSent,
    Ready(mpsc::Sender<ManagerCommand>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingTarget {
    Left,
    Right,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingConfirmation {
    StartRing(RingTarget),
}

#[derive(Debug, Clone)]
pub enum AppState {
    Disconnected,
    Connecting(String),
    Identifying(String),
    Connected(ModelInfo),
    Error(String),
}

pub struct ConnectedDevice {
    pub id: String,
    pub model: ModelInfo,
    pub battery_left: Option<u8>,
    pub sku_attempts: u8,
    pub battery_right: Option<u8>,
    pub battery_case: Option<u8>,
    pub anc_status: u8,
    pub eq_mode: EqMode,
    pub eq_preset: u8,
    pub advanced_eq_enabled: bool,
    pub bass_level: u8,
    pub bass_enhance_enabled: bool,
    pub ringing_left: bool,
    pub ringing_right: bool,
    pub in_ear_enabled: bool,
    pub latency_low: bool,
    pub personalized_anc_enabled: bool,
    pub firmware_version: String,
    pub custom_eq: [f32; 3],
    pub ear_tip_left: Option<u8>,
    pub ear_tip_right: Option<u8>,
    pub ear_tip_test_running: bool,
}

pub struct InitialDataLoad {
    pub battery: bool,
    pub anc: bool,
    pub eq: bool,
    pub personalized_anc: bool,
    pub in_ear: bool,
    pub latency: bool,
    pub enhanced_bass: bool,
    pub custom_eq: bool,
    pub require_anc: bool,
    pub require_personalized_anc: bool,
}

impl InitialDataLoad {
    pub fn for_model(model: &ModelInfo) -> Self {
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

    pub fn is_complete(&self) -> bool {
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
