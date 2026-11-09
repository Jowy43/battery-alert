use std::collections::HashMap;
use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    GetStockObject, InvalidateRect, RoundRect, SelectObject, SetBkMode, SetTextColor,
    CLEARTYPE_QUALITY, DEFAULT_CHARSET, DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT, DT_SINGLELINE,
    DT_VCENTER, FW_NORMAL, FW_SEMIBOLD, NULL_PEN, PAINTSTRUCT, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_WARNING, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconFromResourceEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyIcon, DestroyMenu, DestroyWindow, DispatchMessageW, DrawIconEx, GetCursorPos,
    GetMessageW, GetSystemMetrics, GetWindow, LoadIconW, PostMessageW, PostQuitMessage,
    RegisterClassW, SetForegroundWindow, SetTimer, ShowWindow, TrackPopupMenu, TranslateMessage,
    DI_NORMAL, GW_OWNER, IDI_APPLICATION, MF_CHECKED, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG,
    SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNORMAL, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_CLOSE,
    WM_DESTROY, WM_ERASEBKGND, WM_KILLFOCUS, WM_LBUTTONUP, WM_NULL, WM_PAINT, WM_RBUTTONUP,
    WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_OVERLAPPED, WS_POPUP,
};

use crate::hid::{self, BatteryReading};

const TRAY_ID_MOUSE: u32 = 1;
const TRAY_ID_HEADSET: u32 = 2;
const ICON_LEVEL_COUNT: usize = 11;
const PANEL_WIDTH: i32 = 384;
const PANEL_HEIGHT: i32 = 320;
const PANEL_CLASS_NAME: &str = "BatteryAlert.BatteryPanel";
const TIMER_ID: usize = 1;
const POLL_INTERVAL_MS: u32 = 60_000;
const CALLBACK_MESSAGE: u32 = WM_APP + 1;
const COMMAND_REFRESH: i32 = 100;
const COMMAND_STARTUP: i32 = 101;
const COMMAND_EXIT: i32 = 102;
const LOW_BATTERY_THRESHOLD: u8 = 20;
const RESET_NOTIFICATION_THRESHOLD: u8 = 26;
const STARTUP_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const STARTUP_VALUE_NAME: &str = "BatteryAlert";

static APP: OnceLock<Mutex<AppState>> = OnceLock::new();

struct AppState {
    readings: Vec<BatteryReading>,
    low_notified: HashMap<String, bool>,
    mouse_icons: Vec<isize>,
    headset_icons: Vec<isize>,
    mouse_unknown_icon: isize,
    headset_unknown_icon: isize,
    panel_hwnd: isize,
}

impl AppState {
    fn new() -> Self {
        Self {
            readings: vec![
                BatteryReading {
                    key: "g502".to_string(),
                    name: "Logitech G502 LIGHTSPEED".to_string(),
                    percent: None,
                    approximate: true,
                    voltage_mv: None,
                    status: "Buscando dispositivo".to_string(),
                },
                BatteryReading {
                    key: "pro-x-wireless".to_string(),
                    name: "Logitech PRO X Wireless".to_string(),
                    percent: None,
                    approximate: true,
                    voltage_mv: None,
                    status: "Buscando dispositivo".to_string(),
                },
            ],
            low_notified: HashMap::new(),
            mouse_icons: (0..ICON_LEVEL_COUNT)
                .map(|step| {
                    create_device_icon(DeviceIconKind::Mouse, Some((step * 10) as u8)) as isize
                })
                .collect(),
            headset_icons: (0..ICON_LEVEL_COUNT)
                .map(|step| {
                    create_device_icon(DeviceIconKind::Headset, Some((step * 10) as u8)) as isize
                })
                .collect(),
            mouse_unknown_icon: create_device_icon(DeviceIconKind::Mouse, None) as isize,
            headset_unknown_icon: create_device_icon(DeviceIconKind::Headset, None) as isize,
            panel_hwnd: 0,
        }
    }

    fn refresh(&mut self, hwnd: HWND) {
        self.readings = hid::read_supported_devices();
        for reading in &self.readings {
            if let Some(percent) = reading.percent {
                let already_notified = self.low_notified.entry(reading.key.clone()).or_default();
                if percent <= LOW_BATTERY_THRESHOLD && !*already_notified {
                    *already_notified = true;
                    show_low_battery_notification(hwnd, reading);
                } else if percent >= RESET_NOTIFICATION_THRESHOLD {
                    *already_notified = false;
                }
            }
        }
        self.update_tray(hwnd);
        if self.panel_hwnd != 0 {
            unsafe { InvalidateRect(self.panel_hwnd as _, null(), 0) };
        }
    }

    fn update_tray(&mut self, hwnd: HWND) {
        for (key, tray_id) in [("g502", TRAY_ID_MOUSE), ("pro-x-wireless", TRAY_ID_HEADSET)] {
            let mut data = notify_data(hwnd, tray_id, self.icon(key));
            data.uFlags = NIF_ICON | NIF_TIP;
            copy_wide(&mut data.szTip, &self.tooltip(key));
            unsafe {
                Shell_NotifyIconW(NIM_MODIFY, &data);
            }
        }
    }

    fn icon(&self, key: &str) -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
        let percent = self
            .readings
            .iter()
            .find(|reading| reading.key == key)
            .and_then(|reading| reading.percent);
        let (icons, unknown_icon) = if key == "g502" {
            (&self.mouse_icons, self.mouse_unknown_icon)
        } else {
            (&self.headset_icons, self.headset_unknown_icon)
        };
        percent
            .map(|value| (value as usize / 10).min(ICON_LEVEL_COUNT - 1))
            .and_then(|index| icons.get(index).copied())
            .or(Some(unknown_icon))
            .filter(|handle| *handle != 0)
            .map(|handle| handle as _)
            .unwrap_or_else(|| unsafe { LoadIconW(null_mut(), IDI_APPLICATION) })
    }

    fn tooltip(&self, key: &str) -> String {
        let reading = self.readings.iter().find(|reading| reading.key == key);
        let short_name = if key == "g502" { "Ratón" } else { "Cascos" };
        match reading {
            Some(reading) => match reading.percent {
                Some(percent) => {
                    let estimate = if reading.approximate { "~" } else { "" };
                    format!("Battery Alert | {short_name} {estimate}{percent}%")
                }
                None => format!("Battery Alert | {short_name}: {}", reading.status),
            },
            None => format!("Battery Alert | {short_name}: Sin lectura"),
        }
    }

    fn shutdown(&mut self) {
        for icon in self
            .mouse_icons
            .drain(..)
            .chain(self.headset_icons.drain(..))
            .chain([self.mouse_unknown_icon, self.headset_unknown_icon])
        {
            if icon != 0 {
                unsafe { DestroyIcon(icon as _) };
            }
        }
    }
}

pub fn run() -> Result<(), String> {
    let instance_mutex_name = wide("Local\\BatteryAlert.Singleton");
    let instance_mutex = unsafe { CreateMutexW(null(), 1, instance_mutex_name.as_ptr()) };
    if instance_mutex.is_null() {
        return Err("No se pudo crear el control de instancia única".to_string());
    }
    if unsafe { GetLastError() } == 183 {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(instance_mutex) };
        return Ok(());
    }

    let app = AppState::new();
    APP.set(Mutex::new(app))
        .map_err(|_| "La aplicación ya está iniciada".to_string())?;

    let instance = unsafe { GetModuleHandleW(null()) };
    if instance.is_null() {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(instance_mutex) };
        return Err("No se pudo obtener la instancia de Windows".to_string());
    }
    let class_name = wide("BatteryAlert.TrayWindow");
    let window_class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&window_class) } == 0 {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(instance_mutex) };
        return Err("No se pudo registrar la ventana de la bandeja".to_string());
    }
    let panel_class_name = wide(PANEL_CLASS_NAME);
    let panel_class = WNDCLASSW {
        lpfnWndProc: Some(panel_window_proc),
        hInstance: instance,
        lpszClassName: panel_class_name.as_ptr(),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&panel_class) } == 0 {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(instance_mutex) };
        return Err("No se pudo registrar el panel de batería".to_string());
    }
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            wide("Battery Alert").as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        )
    };
    if hwnd.is_null() {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(instance_mutex) };
        return Err("No se pudo crear la ventana oculta de la bandeja".to_string());
    }

    if !add_tray_icons(hwnd) {
        unsafe {
            DestroyWindow(hwnd);
            windows_sys::Win32::Foundation::CloseHandle(instance_mutex);
        }
        return Err("Windows rechazó el icono de la bandeja".to_string());
    }
    if unsafe { SetTimer(hwnd, TIMER_ID, POLL_INTERVAL_MS, None) } == 0 {
        unsafe { DestroyWindow(hwnd) };
        return Err("No se pudo activar el temporizador de batería".to_string());
    }
    if let Some(state) = APP.get() {
        if let Ok(mut state) = state.lock() {
            state.refresh(hwnd);
        }
    }

    let mut message = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
        if result <= 0 {
            break;
        }
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    if let Some(state) = APP.get() {
        if let Ok(mut state) = state.lock() {
            state.shutdown();
        }
    }
    unsafe { windows_sys::Win32::Foundation::CloseHandle(instance_mutex) };
    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    match message {
        CALLBACK_MESSAGE if lparam as u32 == WM_LBUTTONUP => {
            toggle_battery_panel(hwnd);
            0
        }
        CALLBACK_MESSAGE if lparam as u32 == WM_RBUTTONUP => {
            show_context_menu(hwnd);
            0
        }
        WM_TIMER if wparam == TIMER_ID => {
            if let Some(state) = APP.get() {
                if let Ok(mut state) = state.lock() {
                    state.refresh(hwnd);
                }
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            let panel = APP
                .get()
                .and_then(|state| state.lock().ok().map(|state| state.panel_hwnd as HWND))
                .unwrap_or(null_mut());
            if !panel.is_null() {
                unsafe { DestroyWindow(panel) };
            }
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::KillTimer(hwnd, TIMER_ID);
                for tray_id in [TRAY_ID_MOUSE, TRAY_ID_HEADSET] {
                    let data = notify_data(hwnd, tray_id, null_mut());
                    Shell_NotifyIconW(NIM_DELETE, &data);
                }
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn toggle_battery_panel(owner: HWND) {
    let existing = APP
        .get()
        .and_then(|state| state.lock().ok().map(|state| state.panel_hwnd as HWND))
        .unwrap_or(null_mut());
    if !existing.is_null() {
        unsafe { DestroyWindow(existing) };
        return;
    }

    let mut cursor = POINT::default();
    unsafe { GetCursorPos(&mut cursor) };
    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) }.max(PANEL_WIDTH);
    let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) }.max(PANEL_HEIGHT);
    let x = (cursor.x - PANEL_WIDTH / 2).clamp(0, screen_width - PANEL_WIDTH);
    let candidate_y = if cursor.y >= PANEL_HEIGHT + 16 {
        cursor.y - PANEL_HEIGHT - 12
    } else {
        cursor.y + 20
    };
    let y = candidate_y.clamp(0, screen_height - PANEL_HEIGHT);
    let class_name = wide(PANEL_CLASS_NAME);
    let title = wide("Battery Alert");
    let instance = unsafe { GetModuleHandleW(null()) };
    let panel = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            x,
            y,
            PANEL_WIDTH,
            PANEL_HEIGHT,
            owner,
            null_mut(),
            instance,
            null(),
        )
    };
    if panel.is_null() {
        return;
    }
    let stored = APP
        .get()
        .and_then(|state| {
            state
                .lock()
                .ok()
                .map(|mut state| state.panel_hwnd = panel as isize)
        })
        .is_some();
    if !stored {
        unsafe { DestroyWindow(panel) };
        return;
    }
    unsafe {
        ShowWindow(panel, SW_SHOWNORMAL);
        SetForegroundWindow(panel);
        InvalidateRect(panel, null(), 0);
    }
}

unsafe extern "system" fn panel_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    match message {
        WM_PAINT => {
            draw_battery_panel(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_LBUTTONUP => {
            handle_panel_click(hwnd, lparam);
            0
        }
        WM_KILLFOCUS => {
            DestroyWindow(hwnd);
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            if let Some(state) = APP.get() {
                if let Ok(mut state) = state.lock() {
                    if state.panel_hwnd == hwnd as isize {
                        state.panel_hwnd = 0;
                    }
                }
            }
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

fn handle_panel_click(hwnd: HWND, lparam: LPARAM) {
    let x = (lparam as u16) as i16 as i32;
    let y = ((lparam >> 16) as u16) as i16 as i32;
    if (344..376).contains(&x) && (8..44).contains(&y) {
        unsafe { DestroyWindow(hwnd) };
        return;
    }

    if (16..152).contains(&x) && (256..298).contains(&y) {
        let owner = unsafe { GetWindow(hwnd, GW_OWNER) };
        refresh_now(owner);
    } else if (158..368).contains(&x) && (256..298).contains(&y) {
        if is_startup_enabled() {
            remove_startup_entry();
        } else {
            add_startup_entry();
        }
        unsafe { InvalidateRect(hwnd, null(), 0) };
    }
}

fn draw_battery_panel(hwnd: HWND) {
    let mut paint = PAINTSTRUCT::default();
    let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
    if hdc.is_null() {
        return;
    }

    fill_solid_rect(
        hdc,
        RECT {
            left: 0,
            top: 0,
            right: PANEL_WIDTH,
            bottom: PANEL_HEIGHT,
        },
        rgb(24, 27, 32),
    );
    unsafe { SetBkMode(hdc, TRANSPARENT as i32) };

    draw_panel_text(
        hdc,
        "Battery Alert",
        RECT {
            left: 20,
            top: 14,
            right: 260,
            bottom: 40,
        },
        rgb(242, 245, 249),
        19,
        FW_SEMIBOLD,
        DT_LEFT,
    );
    draw_panel_text(
        hdc,
        "BATERÍA DE TUS DISPOSITIVOS",
        RECT {
            left: 21,
            top: 38,
            right: 282,
            bottom: 54,
        },
        rgb(145, 154, 164),
        9,
        FW_SEMIBOLD,
        DT_LEFT,
    );
    fill_rounded_rect(hdc, 342, 10, 30, 30, rgb(39, 44, 51), 10);
    draw_panel_text(
        hdc,
        "×",
        RECT {
            left: 342,
            top: 8,
            right: 372,
            bottom: 40,
        },
        rgb(190, 198, 207),
        20,
        FW_NORMAL,
        DT_SINGLELINE | DT_VCENTER,
    );

    let devices = APP.get().and_then(|state| {
        state.lock().ok().map(|state| {
            (
                state.readings.clone(),
                state.icon("g502"),
                state.icon("pro-x-wireless"),
            )
        })
    });
    if let Some((readings, mouse_icon, headset_icon)) = devices {
        if let Some(mouse) = readings.iter().find(|reading| reading.key == "g502") {
            draw_panel_card(hdc, 14, 62, mouse, mouse_icon);
        }
        if let Some(headset) = readings
            .iter()
            .find(|reading| reading.key == "pro-x-wireless")
        {
            draw_panel_card(hdc, 14, 151, headset, headset_icon);
        }
    }

    fill_rounded_rect(hdc, 14, 256, 138, 42, rgb(39, 128, 226), 12);
    draw_panel_text(
        hdc,
        "Actualizar ahora",
        RECT {
            left: 20,
            top: 256,
            right: 146,
            bottom: 298,
        },
        rgb(255, 255, 255),
        12,
        FW_SEMIBOLD,
        DT_SINGLELINE | DT_VCENTER,
    );

    let startup_enabled = is_startup_enabled();
    fill_rounded_rect(hdc, 160, 256, 210, 42, rgb(39, 44, 51), 12);
    draw_panel_text(
        hdc,
        "Iniciar con Windows",
        RECT {
            left: 173,
            top: 256,
            right: 300,
            bottom: 298,
        },
        rgb(225, 231, 237),
        11,
        FW_SEMIBOLD,
        DT_SINGLELINE | DT_VCENTER,
    );
    draw_panel_text(
        hdc,
        if startup_enabled {
            "Activado"
        } else {
            "Desactivado"
        },
        RECT {
            left: 294,
            top: 256,
            right: 359,
            bottom: 298,
        },
        if startup_enabled {
            rgb(95, 207, 145)
        } else {
            rgb(150, 160, 169)
        },
        9,
        FW_NORMAL,
        DT_RIGHT | DT_SINGLELINE | DT_VCENTER,
    );

    unsafe { EndPaint(hwnd, &paint) };
}

fn draw_panel_card(
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    top: i32,
    card_top: i32,
    reading: &BatteryReading,
    icon: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
) {
    const CARD_HEIGHT: i32 = 80;
    const BAR_LEFT: i32 = 82;
    const BAR_WIDTH: i32 = 272;
    fill_rounded_rect(hdc, top, card_top, 356, CARD_HEIGHT, rgb(34, 38, 44), 14);
    unsafe {
        DrawIconEx(
            hdc,
            top + 13,
            card_top + 17,
            icon,
            46,
            46,
            0,
            null_mut(),
            DI_NORMAL,
        );
    }

    let device_name = if reading.key == "g502" {
        "G502 LIGHTSPEED"
    } else {
        "PRO X Wireless"
    };
    draw_panel_text(
        hdc,
        device_name,
        RECT {
            left: top + 68,
            top: card_top + 12,
            right: top + 248,
            bottom: card_top + 34,
        },
        rgb(235, 239, 244),
        13,
        FW_SEMIBOLD,
        DT_LEFT,
    );

    let detail = match reading.percent {
        Some(_) if reading.approximate => reading
            .voltage_mv
            .map(|millivolts| format!("Estimación · {millivolts} mV"))
            .unwrap_or_else(|| "Porcentaje estimado".to_string()),
        Some(_) => "Porcentaje comunicado por el dispositivo".to_string(),
        None => reading.status.clone(),
    };
    draw_panel_text(
        hdc,
        &detail,
        RECT {
            left: top + 68,
            top: card_top + 35,
            right: top + 248,
            bottom: card_top + 54,
        },
        rgb(151, 160, 170),
        9,
        FW_NORMAL,
        DT_LEFT,
    );

    let (value_text, value_color) = match reading.percent {
        Some(percent) => (
            format!("{}{}%", if reading.approximate { "~" } else { "" }, percent),
            battery_color(percent),
        ),
        None => ("—".to_string(), rgb(150, 160, 169)),
    };
    draw_panel_text(
        hdc,
        &value_text,
        RECT {
            left: top + 270,
            top: card_top + 11,
            right: top + 339,
            bottom: card_top + 44,
        },
        value_color,
        22,
        FW_SEMIBOLD,
        DT_RIGHT,
    );

    fill_rounded_rect(
        hdc,
        top + BAR_LEFT,
        card_top + 61,
        BAR_WIDTH,
        7,
        rgb(58, 64, 72),
        5,
    );
    if let Some(percent) = reading.percent {
        let fill_width = BAR_WIDTH * percent as i32 / 100;
        if fill_width > 0 {
            fill_rounded_rect(
                hdc,
                top + BAR_LEFT,
                card_top + 61,
                fill_width.max(4),
                7,
                battery_color(percent),
                5,
            );
        }
    }
}

fn battery_color(percent: u8) -> u32 {
    if percent <= 20 {
        rgb(235, 82, 82)
    } else if percent <= 40 {
        rgb(239, 179, 73)
    } else {
        rgb(68, 194, 130)
    }
}

fn rgb(red: u8, green: u8, blue: u8) -> u32 {
    red as u32 | ((green as u32) << 8) | ((blue as u32) << 16)
}

fn draw_panel_text(
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    text: &str,
    mut rect: RECT,
    color: u32,
    height: i32,
    weight: u32,
    alignment: u32,
) {
    let face = wide("Segoe UI");
    let font = unsafe {
        CreateFontW(
            -height,
            0,
            0,
            0,
            weight as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            face.as_ptr(),
        )
    };
    if font.is_null() {
        return;
    }
    let previous_font = unsafe { SelectObject(hdc, font as _) };
    unsafe {
        SetBkMode(hdc, TRANSPARENT as i32);
        SetTextColor(hdc, color);
    }
    let text = wide(text);
    unsafe {
        DrawTextW(
            hdc,
            text.as_ptr(),
            text.len().saturating_sub(1) as i32,
            &mut rect,
            alignment | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        SelectObject(hdc, previous_font);
        DeleteObject(font as _);
    }
}

fn fill_solid_rect(hdc: windows_sys::Win32::Graphics::Gdi::HDC, rect: RECT, color: u32) {
    let brush = unsafe { CreateSolidBrush(color) };
    if brush.is_null() {
        return;
    }
    unsafe {
        FillRect(hdc, &rect, brush);
        DeleteObject(brush as _);
    }
}

fn fill_rounded_rect(
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    color: u32,
    radius: i32,
) {
    let brush = unsafe { CreateSolidBrush(color) };
    if brush.is_null() {
        return;
    }
    let old_brush = unsafe { SelectObject(hdc, brush as _) };
    let null_pen = unsafe { GetStockObject(NULL_PEN) };
    let old_pen = unsafe { SelectObject(hdc, null_pen) };
    unsafe {
        RoundRect(hdc, left, top, left + width, top + height, radius, radius);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        DeleteObject(brush as _);
    }
}

fn add_tray_icons(hwnd: HWND) -> bool {
    let Some((mouse_icon, mouse_tip, headset_icon, headset_tip)) = APP.get().and_then(|state| {
        state.lock().ok().map(|state| {
            (
                state.icon("g502"),
                state.tooltip("g502"),
                state.icon("pro-x-wireless"),
                state.tooltip("pro-x-wireless"),
            )
        })
    }) else {
        return false;
    };

    let mouse_added = add_tray_icon(hwnd, TRAY_ID_MOUSE, mouse_icon, &mouse_tip);
    let headset_added =
        mouse_added && add_tray_icon(hwnd, TRAY_ID_HEADSET, headset_icon, &headset_tip);
    if !mouse_added || !headset_added {
        for tray_id in [TRAY_ID_MOUSE, TRAY_ID_HEADSET] {
            let data = notify_data(hwnd, tray_id, null_mut());
            unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
        }
        return false;
    }
    true
}

fn add_tray_icon(
    hwnd: HWND,
    tray_id: u32,
    icon: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
    tooltip: &str,
) -> bool {
    let mut data = notify_data(hwnd, tray_id, icon);
    data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    data.uCallbackMessage = CALLBACK_MESSAGE;
    copy_wide(&mut data.szTip, tooltip);
    unsafe { Shell_NotifyIconW(NIM_ADD, &data) != 0 }
}

fn show_context_menu(hwnd: HWND) {
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return;
    }
    let readings = APP
        .get()
        .and_then(|state| state.lock().ok().map(|state| state.readings.clone()))
        .unwrap_or_default();
    for reading in &readings {
        let status = match reading.percent {
            Some(percent) => {
                let estimate = if reading.approximate { "~" } else { "" };
                let voltage = reading
                    .voltage_mv
                    .map(|millivolts| format!(" · {millivolts} mV"))
                    .unwrap_or_default();
                format!("{}: {estimate}{percent}%{voltage}", reading.name)
            }
            None => format!("{}: {}", reading.name, reading.status),
        };
        append_menu_item(menu, MF_STRING | MF_GRAYED, 0, &status);
    }
    unsafe {
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
    }
    append_menu_item(
        menu,
        MF_STRING,
        COMMAND_REFRESH as usize,
        "Actualizar ahora",
    );
    let startup_flag = if is_startup_enabled() { MF_CHECKED } else { 0 };
    append_menu_item(
        menu,
        MF_STRING | startup_flag,
        COMMAND_STARTUP as usize,
        "Iniciar con Windows",
    );
    append_menu_item(menu, MF_STRING, COMMAND_EXIT as usize, "Salir");

    let mut point = POINT::default();
    unsafe {
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
    }
    let command = unsafe {
        TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            hwnd,
            null(),
        )
    };
    unsafe {
        DestroyMenu(menu);
        PostMessageW(hwnd, WM_NULL, 0, 0);
    }
    match command {
        COMMAND_REFRESH => refresh_now(hwnd),
        COMMAND_STARTUP => {
            if is_startup_enabled() {
                remove_startup_entry();
            } else {
                add_startup_entry();
            }
        }
        COMMAND_EXIT => unsafe {
            DestroyWindow(hwnd);
        },
        _ => {}
    }
}

fn refresh_now(hwnd: HWND) {
    if let Some(state) = APP.get() {
        if let Ok(mut state) = state.lock() {
            state.refresh(hwnd);
        }
    }
}

fn append_menu_item(
    menu: windows_sys::Win32::UI::WindowsAndMessaging::HMENU,
    flags: u32,
    id: usize,
    text: &str,
) {
    let text = wide(text);
    unsafe {
        AppendMenuW(menu, flags, id, text.as_ptr());
    }
}

fn notify_data(
    hwnd: HWND,
    tray_id: u32,
    icon: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW::default();
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = tray_id;
    data.hIcon = icon;
    data
}

fn show_low_battery_notification(hwnd: HWND, reading: &BatteryReading) {
    let percent = reading.percent.unwrap_or_default();
    let estimate = if reading.approximate {
        "aproximada "
    } else {
        ""
    };
    let message = format!("{} tiene {estimate}{percent}% de batería.", reading.name);
    let tray_id = if reading.key == "g502" {
        TRAY_ID_MOUSE
    } else {
        TRAY_ID_HEADSET
    };
    let mut data = notify_data(hwnd, tray_id, null_mut());
    data.uFlags = NIF_INFO;
    data.dwInfoFlags = NIIF_WARNING;
    copy_wide(&mut data.szInfoTitle, "Batería baja");
    copy_wide(&mut data.szInfo, &message);
    unsafe {
        Shell_NotifyIconW(NIM_MODIFY, &data);
    }
}

fn copy_wide<const N: usize>(destination: &mut [u16; N], text: &str) {
    let encoded: Vec<u16> = text.encode_utf16().collect();
    let length = encoded.len().min(N.saturating_sub(1));
    destination[..length].copy_from_slice(&encoded[..length]);
    destination[length] = 0;
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn is_startup_enabled() -> bool {
    let subkey = wide(STARTUP_SUBKEY);
    let value_name = wide(STARTUP_VALUE_NAME);
    let mut key = null_mut();
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        )
    };
    if opened != 0 {
        return false;
    }
    let result = unsafe {
        let mut bytes = 0_u32;
        let status = windows_sys::Win32::System::Registry::RegQueryValueExW(
            key,
            value_name.as_ptr(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut bytes,
        );
        RegCloseKey(key);
        status == 0 && bytes > 0
    };
    result
}

fn add_startup_entry() {
    let subkey = wide(STARTUP_SUBKEY);
    let value_name = wide(STARTUP_VALUE_NAME);
    let mut key = null_mut();
    let mut disposition = 0_u32;
    let created = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            null(),
            &mut key,
            &mut disposition,
        )
    };
    if created != 0 {
        return;
    }
    let mut path_buffer = vec![0_u16; 32_768];
    let length = unsafe {
        GetModuleFileNameW(
            null_mut(),
            path_buffer.as_mut_ptr(),
            path_buffer.len() as u32,
        )
    } as usize;
    if length == 0 || length >= path_buffer.len() {
        unsafe { RegCloseKey(key) };
        return;
    }
    let executable = String::from_utf16_lossy(&path_buffer[..length]);
    let command = wide(&format!("\"{executable}\""));
    unsafe {
        RegSetValueExW(
            key,
            value_name.as_ptr(),
            0,
            REG_SZ,
            command.as_ptr().cast(),
            (command.len() * size_of::<u16>()) as u32,
        );
        RegCloseKey(key);
    }
}

fn remove_startup_entry() {
    let subkey = wide(STARTUP_SUBKEY);
    let value_name = wide(STARTUP_VALUE_NAME);
    let mut key = null_mut();
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut key,
        )
    };
    if opened == 0 {
        unsafe {
            RegDeleteValueW(key, value_name.as_ptr());
            RegCloseKey(key);
        }
    }
}

#[derive(Clone, Copy)]
enum DeviceIconKind {
    Mouse,
    Headset,
}

fn create_device_icon(
    kind: DeviceIconKind,
    percent: Option<u8>,
) -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
    const WIDTH: usize = 32;
    const HEIGHT: usize = 32;
    const HEADER_SIZE: usize = 40;
    const COLOR_SIZE: usize = WIDTH * HEIGHT * 4;
    const MASK_SIZE: usize = WIDTH * 4 * HEIGHT;
    let mut icon_data = vec![0_u8; HEADER_SIZE + COLOR_SIZE + MASK_SIZE];
    write_u32(&mut icon_data, 0, HEADER_SIZE as u32);
    write_u32(&mut icon_data, 4, WIDTH as u32);
    write_u32(&mut icon_data, 8, (HEIGHT * 2) as u32);
    write_u16(&mut icon_data, 12, 1);
    write_u16(&mut icon_data, 14, 32);
    write_u32(&mut icon_data, 20, COLOR_SIZE as u32);
    icon_data[HEADER_SIZE + COLOR_SIZE..].fill(0xFF);

    match kind {
        DeviceIconKind::Mouse => draw_mouse(&mut icon_data),
        DeviceIconKind::Headset => draw_headset(&mut icon_data),
    }
    draw_battery_strip(&mut icon_data, percent);

    unsafe {
        CreateIconFromResourceEx(
            icon_data.as_ptr(),
            icon_data.len() as u32,
            1,
            0x0003_0000,
            WIDTH as i32,
            HEIGHT as i32,
            0,
        )
    }
}

fn draw_mouse(icon_data: &mut [u8]) {
    let outline = [28, 31, 35, 255];
    let body = [224, 230, 235, 255];
    let detail = [92, 103, 113, 255];
    let accent = [75, 177, 225, 255];

    fill_icon_rect(icon_data, 12, 2, 20, 5, outline);
    fill_icon_rect(icon_data, 10, 4, 22, 9, outline);
    fill_icon_rect(icon_data, 8, 9, 24, 19, outline);
    fill_icon_rect(icon_data, 10, 19, 22, 26, outline);
    fill_icon_rect(icon_data, 13, 3, 19, 5, body);
    fill_icon_rect(icon_data, 11, 5, 21, 8, body);
    fill_icon_rect(icon_data, 9, 10, 23, 18, body);
    fill_icon_rect(icon_data, 11, 19, 21, 25, body);
    fill_icon_rect(icon_data, 15, 5, 17, 9, outline);
    fill_icon_rect(icon_data, 15, 6, 17, 9, accent);
    fill_icon_rect(icon_data, 15, 11, 17, 17, detail);
    fill_icon_rect(icon_data, 10, 13, 12, 17, detail);
    fill_icon_rect(icon_data, 20, 13, 22, 17, detail);
}

fn draw_headset(icon_data: &mut [u8]) {
    let outline = [28, 31, 35, 255];
    let body = [224, 230, 235, 255];
    let detail = [92, 103, 113, 255];
    let accent = [75, 177, 225, 255];

    fill_icon_rect(icon_data, 9, 4, 23, 8, outline);
    fill_icon_rect(icon_data, 6, 7, 11, 16, outline);
    fill_icon_rect(icon_data, 21, 7, 26, 16, outline);
    fill_icon_rect(icon_data, 3, 13, 11, 26, outline);
    fill_icon_rect(icon_data, 21, 13, 29, 26, outline);

    fill_icon_rect(icon_data, 11, 5, 21, 7, accent);
    fill_icon_rect(icon_data, 7, 8, 10, 15, body);
    fill_icon_rect(icon_data, 22, 8, 25, 15, body);
    fill_icon_rect(icon_data, 4, 15, 10, 24, body);
    fill_icon_rect(icon_data, 22, 15, 28, 24, body);
    fill_icon_rect(icon_data, 5, 17, 9, 22, detail);
    fill_icon_rect(icon_data, 23, 17, 27, 22, detail);
}

fn draw_battery_strip(icon_data: &mut [u8], percent: Option<u8>) {
    let outline = [24, 27, 31, 255];
    let empty = [90, 98, 107, 255];
    fill_icon_rect(icon_data, 3, 27, 29, 31, outline);
    fill_icon_rect(icon_data, 5, 28, 27, 30, empty);
    match percent {
        Some(percent) => {
            let color = if percent <= 20 {
                [238, 76, 76, 255]
            } else if percent <= 40 {
                [241, 182, 67, 255]
            } else {
                [70, 202, 124, 255]
            };
            let width = (22 * percent as usize) / 100;
            if width > 0 {
                fill_icon_rect(icon_data, 5, 28, 5 + width, 30, color);
            }
        }
        None => {
            let unavailable = [185, 65, 65, 255];
            draw_icon_line(icon_data, 13, 28, 19, 29, unavailable);
            draw_icon_line(icon_data, 19, 28, 13, 29, unavailable);
        }
    }
}

fn fill_icon_rect(
    icon_data: &mut [u8],
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
    color: [u8; 4],
) {
    for y in top..bottom {
        for x in left..right {
            set_icon_pixel(icon_data, x, y, color);
        }
    }
}

fn draw_icon_line(icon_data: &mut [u8], x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 4]) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut error = dx + dy;
    let (mut x, mut y) = (x0, y0);
    loop {
        if x >= 0 && y >= 0 {
            set_icon_pixel(icon_data, x as usize, y as usize, color);
        }
        if x == x1 && y == y1 {
            break;
        }
        let doubled_error = 2 * error;
        if doubled_error >= dy {
            error += dy;
            x += sx;
        }
        if doubled_error <= dx {
            error += dx;
            y += sy;
        }
    }
}

fn set_icon_pixel(icon_data: &mut [u8], x: usize, y: usize, color: [u8; 4]) {
    const WIDTH: usize = 32;
    const HEIGHT: usize = 32;
    const HEADER_SIZE: usize = 40;
    const COLOR_SIZE: usize = WIDTH * HEIGHT * 4;
    if x >= WIDTH || y >= HEIGHT {
        return;
    }
    let row = HEIGHT - 1 - y;
    let pixel = HEADER_SIZE + (row * WIDTH + x) * 4;
    icon_data[pixel..pixel + 4].copy_from_slice(&color);
    let mask = HEADER_SIZE + COLOR_SIZE + row * 4 + x / 8;
    icon_data[mask] &= !(0x80 >> (x % 8));
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
