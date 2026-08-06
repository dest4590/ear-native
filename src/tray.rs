use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIconBuilder, TrayIconEvent,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    Show,
    Hide,
    Quit,
}

static SPAWNED: AtomicBool = AtomicBool::new(false);
static RX: Mutex<Option<std::sync::mpsc::Receiver<TrayEvent>>> = Mutex::new(None);
static BATTERY_TX: OnceLock<tokio::sync::mpsc::Sender<(String, String, String)>> = OnceLock::new();

pub fn spawn() {
    if SPAWNED.swap(true, Ordering::SeqCst) {
        return;
    }

    let (tx, rx) = std::sync::mpsc::channel();
    if let Ok(mut guard) = RX.lock() {
        *guard = Some(rx);
    }

    let thread = std::thread::Builder::new().name("tray".to_string());
    if let Err(error) = thread.spawn(move || run_tray(tx)) {
        log::error!("failed to spawn tray thread: {}", error);
    }
}

pub fn try_recv() -> Option<TrayEvent> {
    let mut guard = RX.lock().ok()?;
    guard.as_mut()?.try_recv().ok()
}

pub fn update_battery(left: Option<u8>, case: Option<u8>, right: Option<u8>) {
    let left_text = match left {
        Some(level) => format!("Left: {}%", level),
        None => "Left: --".to_string(),
    };
    let case_text = match case {
        Some(level) => format!("Case: {}%", level),
        None => "Case: --".to_string(),
    };
    let right_text = match right {
        Some(level) => format!("Right: {}%", level),
        None => "Right: --".to_string(),
    };

    if let Some(tx) = BATTERY_TX.get() {
        let _ = tx.try_send((left_text, case_text, right_text));
    }
}

fn run_tray(tx: std::sync::mpsc::Sender<TrayEvent>) {
    #[cfg(target_os = "linux")]
    {
        if !gtk::is_initialized() {
            if let Err(error) = gtk::init() {
                log::warn!("failed to initialize GTK for the tray icon: {}", error);
                return;
            }
        }
    }

    let icon = match load_icon() {
        Ok(icon) => icon,
        Err(error) => {
            log::warn!("failed to load tray icon: {}", error);
            return;
        }
    };

    let menu = match build_menu() {
        Ok(menu) => menu,
        Err(error) => {
            log::warn!("failed to build tray menu: {}", error);
            return;
        }
    };

    let show_id = menu.show.id().clone();
    let hide_id = menu.hide.id().clone();
    let quit_id = menu.quit.id().clone();
    let batt_left = menu.battery_left.clone();
    let batt_case = menu.battery_case.clone();
    let batt_right = menu.battery_right.clone();

    match TrayIconBuilder::new()
        .with_tooltip("ear (native)")
        .with_menu(Box::new(menu.menu))
        .with_icon(icon)
        .build()
    {
        Ok(tray) => {
            log::info!("tray icon created successfully");
            std::mem::forget(tray);
        }
        Err(error) => {
            log::warn!("failed to create tray icon: {}", error);
            return;
        }
    }

    let (battery_tx, mut battery_rx) = tokio::sync::mpsc::channel::<(String, String, String)>(8);
    let _ = BATTERY_TX.set(battery_tx);

    #[cfg(target_os = "linux")]
    {
        use gtk::glib;
        use gtk::glib::ControlFlow;
        let bl = batt_left.clone();
        let bc = batt_case.clone();
        let br = batt_right.clone();
        glib::timeout_add_local(Duration::from_millis(200), move || {
            while let Ok((l, c, r)) = battery_rx.try_recv() {
                bl.set_text(&l);
                bc.set_text(&c);
                br.set_text(&r);
            }
            ControlFlow::Continue
        });
    }

    let menu_tx = tx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let action = if event.id == show_id {
            Some(TrayEvent::Show)
        } else if event.id == hide_id {
            Some(TrayEvent::Hide)
        } else if event.id == quit_id {
            Some(TrayEvent::Quit)
        } else {
            None
        };
        if let Some(action) = action {
            let _ = menu_tx.send(action);
        }
    }));

    let click_tx = tx;
    TrayIconEvent::set_event_handler(Some(move |_event: TrayIconEvent| {
        let _ = click_tx.send(TrayEvent::Show);
    }));

    run_loop();
}

struct TrayMenuBuilder {
    menu: Menu,
    show: MenuItem,
    hide: MenuItem,
    quit: MenuItem,
    battery_left: MenuItem,
    battery_case: MenuItem,
    battery_right: MenuItem,
}

fn build_menu() -> Result<TrayMenuBuilder, tray_icon::menu::Error> {
    let battery_left = MenuItem::new("Left: --", false, None);
    let battery_case = MenuItem::new("Case: --", false, None);
    let battery_right = MenuItem::new("Right: --", false, None);
    let show = MenuItem::new("Show", true, None);
    let hide = MenuItem::new("Hide", true, None);
    let quit = MenuItem::new("Quit", true, None);

    let menu = Menu::new();
    menu.append_items(&[
        &battery_left,
        &battery_case,
        &battery_right,
        &PredefinedMenuItem::separator(),
        &show,
        &hide,
        &PredefinedMenuItem::separator(),
        &quit,
    ])?;

    Ok(TrayMenuBuilder {
        menu,
        show,
        hide,
        quit,
        battery_left,
        battery_case,
        battery_right,
    })
}

fn load_icon() -> Result<Icon, Box<dyn std::error::Error>> {
    let img = image::load_from_memory(include_bytes!("../res/icon/logo.png"))?;
    let img = img.resize(64, 64, image::imageops::FilterType::Lanczos3);
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok(Icon::from_rgba(rgba.into_raw(), width, height)?)
}

#[cfg(target_os = "linux")]
fn run_loop() {
    gtk::main();
}

#[cfg(target_os = "windows")]
fn run_loop() {
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, TranslateMessage, MSG,
    };

    unsafe {
        loop {
            let mut msg = MSG::default();
            let result = GetMessageW(&mut msg, None, 0, 0);
            if result.0 == 0 || result.0 == -1 {
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
