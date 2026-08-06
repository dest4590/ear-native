use iced::{window::icon, Font, Task, Theme};

use std::collections::HashMap;
use tokio::sync::mpsc;

mod app;
mod bluetooth;
mod components;
mod config;
mod models;
mod protocol;
mod tray;
mod ui;

use bluetooth::ManagerCommand;
use config::AppConfig;
use models::{get_models, get_sku_map, ModelInfo};
use ui::{APP_FONT_NAME, PURE_BLACK};

use std::time::Instant;

use crate::app::state::{AppState, ConnectedDevice, InitialDataLoad, Message, PendingConfirmation};

pub struct EarNative {
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
    bluetooth_init_failed: bool,
    window_id: Option<iced::window::Id>,
}

pub fn main() -> iced::Result {
    let mut env_b =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    env_b.filter_module("iced_winit", log::LevelFilter::Off);
    env_b.init();

    tray::spawn();

    let icon_data = include_bytes!("../res/icon/logo.png");
    let img = ::image::load_from_memory(icon_data)
        .expect("Failed to load icon")
        .resize(64, 64, ::image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let (w, h) = img.dimensions();
    let icon = icon::from_rgba(img.into_raw(), w, h).expect("Failed to create icon");

    let settings = iced::window::Settings {
        size: iced::Size::new(450.0, 700.0),
        icon: Some(icon),
        exit_on_close_request: false,
        ..Default::default()
    };

    iced::application(EarNative::boot, EarNative::update, EarNative::view)
        .theme(EarNative::theme)
        .subscription(EarNative::subscription)
        .font(include_bytes!("../res/fonts/Silkscreen-Regular.ttf").as_slice())
        .default_font(Font::with_name(APP_FONT_NAME))
        .window(settings)
        .title("ear (native)")
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
            bluetooth_init_failed: false,
            window_id: None,
        }
    }
}

impl EarNative {
    pub fn boot() -> (Self, Task<Message>) {
        (
            Self::default(),
            Task::batch([
                Task::perform(
                    async { AppConfig::load_or_default() },
                    Message::ConfigLoaded,
                ),
                iced::window::latest().map(Message::WindowId),
            ]),
        )
    }

    pub fn theme(&self) -> Theme {
        Theme::custom(
            "Dark".to_string(),
            iced::theme::Palette {
                background: PURE_BLACK,
                ..Theme::Dark.palette()
            },
        )
    }
}
