use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
    SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO,
    SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
};
use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetHidGuid, HidD_GetPreparsedData,
    HidP_GetCaps, HIDD_ATTRIBUTES, HIDP_CAPS, HIDP_STATUS_SUCCESS,
};
use windows_sys::Win32::Foundation::GENERIC_READ;
use windows_sys::Win32::Foundation::GENERIC_WRITE;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

const LOGITECH_VID: u16 = 0x046D;
const PID_G502_LIGHTSPEED: u16 = 0xC08D;
const PID_PRO_X_WIRELESS: u16 = 0x0ABA;
const REPORT_SHORT: u8 = 0x10;
const REPORT_LONG: u8 = 0x11;
const DEVICE_INDEX_DIRECT: u8 = 0xFF;
const SOFTWARE_ID: u8 = 0x0E;
const FEATURE_ROOT: u8 = 0x00;
const FEATURE_BATTERY_VOLTAGE: u16 = 0x1001;
const FEATURE_BATTERY_STATUS: u16 = 0x1000;
const FEATURE_UNIFIED_BATTERY: u16 = 0x1004;
const FEATURE_ADC_MEASUREMENT: u16 = 0x1F20;
const MAX_FRAME_SIZE: usize = 20;
const IO_TIMEOUT_MS: u32 = 180;

#[derive(Clone, Debug)]
pub struct BatteryReading {
    pub key: String,
    pub name: String,
    pub percent: Option<u8>,
    pub approximate: bool,
    pub voltage_mv: Option<u16>,
    pub status: String,
}

#[derive(Clone, Debug)]
struct HidInterface {
    path: String,
    group: String,
    vendor_id: u16,
    product_id: u16,
    usage_page: u16,
    usage: u16,
    input_len: usize,
    output_len: usize,
}

#[derive(Clone, Copy)]
struct BatteryValue {
    percent: u8,
    approximate: bool,
    voltage_mv: Option<u16>,
}

struct HidTransport {
    long: OpenHid,
    short: Option<OpenHid>,
}

struct OpenHid {
    handle: HANDLE,
    input_len: usize,
    output_len: usize,
}

impl Drop for OpenHid {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

impl HidTransport {
    fn open(long_info: &HidInterface, short_info: Option<&HidInterface>) -> Option<Self> {
        let long = OpenHid::open(long_info)?;
        let short = short_info.and_then(OpenHid::open);
        Some(Self { long, short })
    }

    fn transact(
        &self,
        device_index: u8,
        feature_index: u8,
        function_id: u8,
        params: &[u8],
        timeout: Duration,
    ) -> Option<Vec<u8>> {
        let use_short = feature_index == FEATURE_ROOT && self.short.is_some();
        let writer = if use_short {
            self.short.as_ref()?
        } else {
            &self.long
        };
        let report_id = if use_short { REPORT_SHORT } else { REPORT_LONG };
        let mut request = vec![0_u8; writer.output_len];
        if request.len() < 7 || request.len() > MAX_FRAME_SIZE {
            return None;
        }
        request[0] = report_id;
        request[1] = device_index;
        request[2] = feature_index;
        request[3] = (function_id << 4) | SOFTWARE_ID;
        for (offset, value) in params.iter().enumerate() {
            if offset + 4 >= request.len() {
                break;
            }
            request[offset + 4] = *value;
        }
        if !write_report(writer.handle, &request, 250) {
            return None;
        }

        let listeners: Vec<&OpenHid> = match self.short.as_ref() {
            Some(short) => vec![short, &self.long],
            None => vec![&self.long],
        };
        let deadline = Instant::now() + timeout;
        let mut frames_seen = 0;
        while Instant::now() < deadline && frames_seen < 8 {
            for listener in &listeners {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                let slice = remaining.min(Duration::from_millis(20));
                let mut response = vec![0_u8; listener.input_len.max(7).min(MAX_FRAME_SIZE)];
                if let Some(count) =
                    read_report(listener.handle, &mut response, slice.as_millis() as u32)
                {
                    frames_seen += 1;
                    response.truncate(count);
                    if response.len() >= 5 && response[1] == device_index {
                        if response[2] == feature_index && response[3] == request[3] {
                            return Some(response);
                        }
                        if (response[2] == 0xFF || response[2] == 0x8F)
                            && response.len() >= 6
                            && response[3] == feature_index
                            && response[4] == request[3]
                        {
                            return None;
                        }
                    }
                }
            }
        }
        None
    }

    fn connected_slot_bitmap(&self) -> Option<u8> {
        let short = self.short.as_ref()?;
        let mut request = vec![0_u8; short.output_len];
        if request.len() < 7 {
            return None;
        }
        request[0] = REPORT_SHORT;
        request[1] = DEVICE_INDEX_DIRECT;
        request[2] = 0x81;
        request[3] = 0x02;
        if !write_report(short.handle, &request, 250) {
            return None;
        }
        let deadline = Instant::now() + Duration::from_millis(IO_TIMEOUT_MS as u64);
        while Instant::now() < deadline {
            for listener in [Some(short), Some(&self.long)].into_iter().flatten() {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                let mut response = vec![0_u8; listener.input_len.max(7).min(MAX_FRAME_SIZE)];
                if let Some(count) = read_report(
                    listener.handle,
                    &mut response,
                    remaining.min(Duration::from_millis(20)).as_millis() as u32,
                ) {
                    if count >= 5
                        && response[1] == DEVICE_INDEX_DIRECT
                        && response[2] == 0x81
                        && response[3] == 0x02
                    {
                        return Some(response[4]);
                    }
                }
            }
        }
        None
    }

    fn feature_index(&self, device_index: u8, feature_id: u16) -> Option<u8> {
        let response = self.transact(
            device_index,
            FEATURE_ROOT,
            0,
            &[(feature_id >> 8) as u8, feature_id as u8, 0],
            Duration::from_millis(IO_TIMEOUT_MS as u64),
        )?;
        let index = *response.get(4)?;
        (index != 0 && index != 0xFF && index != 0x8F).then_some(index)
    }

    fn read_battery(&self, device_index: u8) -> Option<BatteryValue> {
        let feature_order = [
            FEATURE_UNIFIED_BATTERY,
            FEATURE_BATTERY_STATUS,
            FEATURE_BATTERY_VOLTAGE,
            FEATURE_ADC_MEASUREMENT,
        ];

        for feature_id in feature_order {
            let Some(feature_index) = self.feature_index(device_index, feature_id) else {
                continue;
            };
            let value = match feature_id {
                FEATURE_UNIFIED_BATTERY => {
                    // 0x1004 reports a percentage only when its capabilities
                    // advertise state of charge. Otherwise its first status
                    // byte is a battery-level bitmask, not a percentage.
                    let Some(capabilities) = self.transact(
                        device_index,
                        feature_index,
                        0,
                        &[],
                        Duration::from_millis(IO_TIMEOUT_MS as u64),
                    ) else {
                        continue;
                    };
                    if capabilities.len() < 6 || capabilities[5] & 0x02 == 0 {
                        continue;
                    }
                    let Some(response) = self.transact(
                        device_index,
                        feature_index,
                        1,
                        &[],
                        Duration::from_millis(IO_TIMEOUT_MS as u64),
                    ) else {
                        continue;
                    };
                    response
                        .get(4)
                        .copied()
                        .filter(|percent| *percent <= 100)
                        .map(|percent| BatteryValue {
                            percent,
                            approximate: false,
                            voltage_mv: None,
                        })
                }
                FEATURE_BATTERY_STATUS => {
                    let Some(response) = self.transact(
                        device_index,
                        feature_index,
                        0,
                        &[],
                        Duration::from_millis(IO_TIMEOUT_MS as u64),
                    ) else {
                        continue;
                    };
                    if response.len() < 7 {
                        continue;
                    }
                    match response[6] {
                        0 if response[4] <= 100 => Some(BatteryValue {
                            percent: response[4],
                            approximate: false,
                            voltage_mv: None,
                        }),
                        3 => Some(BatteryValue {
                            percent: 100,
                            approximate: false,
                            voltage_mv: None,
                        }),
                        _ => None,
                    }
                }
                FEATURE_BATTERY_VOLTAGE | FEATURE_ADC_MEASUREMENT => {
                    let Some(response) = self.transact(
                        device_index,
                        feature_index,
                        0,
                        &[],
                        Duration::from_millis(IO_TIMEOUT_MS as u64),
                    ) else {
                        continue;
                    };
                    if response.len() < 7 {
                        continue;
                    }
                    let millivolts = u16::from_be_bytes([response[4], response[5]]) as i32;
                    (2000..=5000).contains(&millivolts).then_some(BatteryValue {
                        percent: voltage_to_percent(millivolts),
                        approximate: true,
                        voltage_mv: Some(millivolts as u16),
                    })
                }
                _ => None,
            };
            if value.is_some() {
                return value;
            }
        }
        None
    }
}

impl OpenHid {
    fn open(info: &HidInterface) -> Option<Self> {
        let path: Vec<u16> = info.path.encode_utf16().chain(Some(0)).collect();
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
                null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE || handle.is_null() {
            return None;
        }
        Some(Self {
            handle,
            input_len: info.input_len,
            output_len: info.output_len,
        })
    }
}

pub fn read_supported_devices() -> Vec<BatteryReading> {
    let interfaces = enumerate_logitech_interfaces();
    let mut mouse = BatteryReading {
        key: "g502".to_string(),
        name: "Logitech G502 LIGHTSPEED".to_string(),
        percent: None,
        approximate: true,
        voltage_mv: None,
        status: "No detectado".to_string(),
    };
    let mut headset = BatteryReading {
        key: "pro-x-wireless".to_string(),
        name: "Logitech PRO X Wireless".to_string(),
        percent: None,
        approximate: true,
        voltage_mv: None,
        status: "No detectado".to_string(),
    };

    let mut checked_mouse_paths = Vec::new();
    let mut checked_headset_paths = Vec::new();
    for long_info in interfaces.iter().filter(|info| is_long_collection(info)) {
        let is_headset = long_info.product_id == PID_PRO_X_WIRELESS;
        let is_mouse = is_mouse_pid(long_info.product_id);
        if (!is_headset && !is_mouse) || long_info.vendor_id != LOGITECH_VID {
            continue;
        }
        if is_headset && !checked_headset_paths.contains(&long_info.path) {
            checked_headset_paths.push(long_info.path.clone());
            headset.status = "Conectado; consultando batería…".to_string();
        }
        if is_mouse && !checked_mouse_paths.contains(&long_info.path) {
            checked_mouse_paths.push(long_info.path.clone());
            mouse.status = "Conectado; consultando batería…".to_string();
        }

        let short_info = interfaces.iter().find(|candidate| {
            candidate.group == long_info.group
                && candidate.vendor_id == long_info.vendor_id
                && candidate.product_id == long_info.product_id
                && candidate.usage_page == 0xFF00
                && candidate.usage == 0x0001
                && candidate.output_len >= 7
                && candidate.input_len >= 7
        });
        let Some(transport) = HidTransport::open(long_info, short_info) else {
            if is_headset {
                headset.status = "No se pudo abrir la interfaz HID".to_string();
            } else {
                mouse.status = "No se pudo abrir la interfaz HID".to_string();
            }
            continue;
        };

        if is_headset {
            if let Some(value) = transport.read_battery(DEVICE_INDEX_DIRECT) {
                headset.percent = Some(value.percent);
                headset.approximate = value.approximate;
                headset.voltage_mv = value.voltage_mv;
                headset.status = "Lectura disponible".to_string();
            } else {
                headset.status = "Sin respuesta (puede estar dormido o ocupado)".to_string();
            }
        } else {
            let mut indices = Vec::new();
            if transport.short.is_some() {
                if let Some(bitmap) = transport.connected_slot_bitmap() {
                    for slot in 1..=6 {
                        if bitmap & (1 << (slot - 1)) != 0 {
                            indices.push(slot);
                        }
                    }
                }
            }
            if indices.is_empty() {
                indices.push(DEVICE_INDEX_DIRECT);
                indices.extend(1..=6);
            }
            for index in indices {
                if let Some(value) = transport.read_battery(index) {
                    mouse.percent = Some(value.percent);
                    mouse.approximate = value.approximate;
                    mouse.voltage_mv = value.voltage_mv;
                    mouse.status = if index == DEVICE_INDEX_DIRECT {
                        "Lectura disponible".to_string()
                    } else {
                        format!("Lectura disponible; receptor, dispositivo {index}")
                    };
                    break;
                }
            }
            if mouse.percent.is_none() {
                mouse.status = "Sin respuesta (puede estar dormido o ocupado)".to_string();
            }
        }
    }

    vec![mouse, headset]
}

fn enumerate_logitech_interfaces() -> Vec<HidInterface> {
    let mut hid_guid = unsafe { zeroed() };
    unsafe { HidD_GetHidGuid(&mut hid_guid) };
    let device_info_set: HDEVINFO = unsafe {
        SetupDiGetClassDevsW(
            &hid_guid,
            null(),
            null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    };
    if device_info_set == -1 {
        return Vec::new();
    }

    let mut interfaces = Vec::new();
    let mut index = 0;
    loop {
        let mut interface_data = SP_DEVICE_INTERFACE_DATA::default();
        interface_data.cbSize = size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
        let has_interface = unsafe {
            SetupDiEnumDeviceInterfaces(
                device_info_set,
                null(),
                &hid_guid,
                index,
                &mut interface_data,
            )
        };
        if has_interface == 0 {
            break;
        }
        index += 1;

        let mut required_size = 0_u32;
        unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                device_info_set,
                &interface_data,
                null_mut(),
                0,
                &mut required_size,
                null_mut(),
            );
        }
        if required_size < 8 {
            continue;
        }
        let units = (required_size as usize + size_of::<usize>() - 1) / size_of::<usize>();
        let mut aligned_buffer = vec![0_usize; units];
        let detail = aligned_buffer.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
        unsafe { (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32 };
        let succeeded = unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                device_info_set,
                &interface_data,
                detail,
                required_size,
                &mut required_size,
                null_mut(),
            )
        };
        if succeeded == 0 {
            continue;
        }
        let path = unsafe {
            let path_ptr = (*detail).DevicePath.as_ptr();
            let available = required_size.saturating_sub(4) as usize / size_of::<u16>();
            let slice = std::slice::from_raw_parts(path_ptr, available);
            let end = slice
                .iter()
                .position(|value| *value == 0)
                .unwrap_or(slice.len());
            String::from_utf16_lossy(&slice[..end])
        };
        let vendor_id = parse_hid_id(&path, "vid_");
        let product_id = parse_hid_id(&path, "pid_");
        if vendor_id != Some(LOGITECH_VID) || !product_id.is_some_and(is_supported_pid) {
            continue;
        }
        if let Some(info) = inspect_interface(path) {
            interfaces.push(info);
        }
    }
    unsafe {
        SetupDiDestroyDeviceInfoList(device_info_set);
    }
    interfaces
}

fn inspect_interface(path: String) -> Option<HidInterface> {
    let wide_path: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    let handle = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        return None;
    }
    let mut attributes = HIDD_ATTRIBUTES::default();
    attributes.Size = size_of::<HIDD_ATTRIBUTES>() as u32;
    let has_attributes = unsafe { HidD_GetAttributes(handle, &mut attributes) };
    let mut preparsed = 0_isize;
    let has_preparsed = unsafe { HidD_GetPreparsedData(handle, &mut preparsed) };
    let mut caps = HIDP_CAPS::default();
    let caps_status = if has_preparsed {
        unsafe { HidP_GetCaps(preparsed, &mut caps) }
    } else {
        0
    };
    if has_preparsed {
        unsafe { HidD_FreePreparsedData(preparsed) };
    }
    unsafe { CloseHandle(handle) };

    if !has_attributes || caps_status != HIDP_STATUS_SUCCESS {
        return None;
    }
    let vendor_id = attributes.VendorID;
    let product_id = attributes.ProductID;
    if vendor_id != LOGITECH_VID || !is_supported_pid(product_id) {
        return None;
    }
    Some(HidInterface {
        group: collection_group(&path),
        path,
        vendor_id,
        product_id,
        usage_page: caps.UsagePage,
        usage: caps.Usage,
        input_len: caps.InputReportByteLength as usize,
        output_len: caps.OutputReportByteLength as usize,
    })
}

fn is_long_collection(info: &HidInterface) -> bool {
    info.input_len >= MAX_FRAME_SIZE
        && info.output_len >= MAX_FRAME_SIZE
        && ((info.usage_page == 0xFF43 && info.usage == 0x0202)
            || (info.usage_page == 0xFF00 && info.usage == 0x0002))
}

fn is_supported_pid(product_id: u16) -> bool {
    product_id == PID_G502_LIGHTSPEED
        || product_id == PID_PRO_X_WIRELESS
        || matches!(
            product_id,
            0xC539 | 0xC53A | 0xC53F | 0xC541 | 0xC545 | 0xC547 | 0xC54D | 0xC532
        )
}

fn is_mouse_pid(product_id: u16) -> bool {
    product_id != PID_PRO_X_WIRELESS && is_supported_pid(product_id)
}

fn parse_hid_id(path: &str, marker: &str) -> Option<u16> {
    let lower = path.to_ascii_lowercase();
    let start = lower.find(marker)? + marker.len();
    let digits = lower.get(start..start + 4)?;
    u16::from_str_radix(digits, 16).ok()
}

fn collection_group(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    let mut result = String::with_capacity(lower.len());
    let chars: Vec<char> = lower.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if index + 6 <= chars.len()
            && chars[index] == '&'
            && chars[index + 1..index + 4] == ['c', 'o', 'l']
            && chars[index + 4].is_ascii_hexdigit()
            && chars[index + 5].is_ascii_hexdigit()
        {
            index += 6;
        } else {
            result.push(chars[index]);
            index += 1;
        }
    }
    result
}

fn write_report(handle: HANDLE, report: &[u8], timeout_ms: u32) -> bool {
    let event = unsafe { CreateEventW(null(), 1, 0, null()) };
    if event.is_null() {
        return false;
    }
    let mut overlapped = OVERLAPPED::default();
    overlapped.hEvent = event;
    let mut written = 0_u32;
    let immediate = unsafe {
        WriteFile(
            handle,
            report.as_ptr(),
            report.len() as u32,
            &mut written,
            &mut overlapped,
        )
    };
    let success = if immediate != 0 {
        written as usize == report.len()
    } else if unsafe { GetLastError() } == 997 {
        if unsafe { WaitForSingleObject(event, timeout_ms) } == WAIT_OBJECT_0 {
            (unsafe { GetOverlappedResult(handle, &overlapped, &mut written, 0) }) != 0
                && written as usize == report.len()
        } else {
            unsafe {
                CancelIoEx(handle, &overlapped);
                GetOverlappedResult(handle, &overlapped, &mut written, 1);
            }
            false
        }
    } else {
        false
    };
    unsafe { CloseHandle(event) };
    success
}

fn read_report(handle: HANDLE, buffer: &mut [u8], timeout_ms: u32) -> Option<usize> {
    let event = unsafe { CreateEventW(null(), 1, 0, null()) };
    if event.is_null() {
        return None;
    }
    let mut overlapped = OVERLAPPED::default();
    overlapped.hEvent = event;
    let mut read = 0_u32;
    let immediate = unsafe {
        ReadFile(
            handle,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            &mut read,
            &mut overlapped,
        )
    };
    let success = if immediate != 0 {
        true
    } else if unsafe { GetLastError() } == 997 {
        if unsafe { WaitForSingleObject(event, timeout_ms.max(1)) } == WAIT_OBJECT_0 {
            (unsafe { GetOverlappedResult(handle, &overlapped, &mut read, 0) }) != 0
        } else {
            unsafe {
                CancelIoEx(handle, &overlapped);
                GetOverlappedResult(handle, &overlapped, &mut read, 1);
            }
            false
        }
    } else {
        false
    };
    unsafe { CloseHandle(event) };
    success.then_some(read as usize)
}

fn voltage_to_percent(millivolts: i32) -> u8 {
    const CURVE: [(i32, i32); 13] = [
        (4186, 100),
        (4067, 90),
        (3989, 80),
        (3922, 70),
        (3859, 60),
        (3811, 50),
        (3778, 40),
        (3751, 30),
        (3717, 20),
        (3671, 10),
        (3646, 5),
        (3579, 2),
        (3500, 0),
    ];
    if millivolts >= CURVE[0].0 {
        return 100;
    }
    if millivolts <= CURVE[CURVE.len() - 1].0 {
        return 0;
    }
    for pair in CURVE.windows(2) {
        let (high_mv, high_pct) = pair[0];
        let (low_mv, low_pct) = pair[1];
        if millivolts > low_mv {
            let span = high_mv - low_mv;
            let percent = low_pct + ((millivolts - low_mv) * (high_pct - low_pct)) / span;
            return percent.clamp(0, 100) as u8;
        }
    }
    0
}
