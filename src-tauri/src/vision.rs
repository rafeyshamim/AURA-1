use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenCaptureResult {
    pub base64_image: String,
    pub width: u32,
    pub height: u32,
    pub timestamp: i64,
    pub foreground_process_id: u32,
    pub foreground_window_title: String,
}

pub struct VisionEngine;

impl VisionEngine {
    pub fn new() -> Self { Self }

    /// Capture the primary monitor directly through Win32 GDI. This avoids
    /// launching PowerShell, which Windows security software may block.
    pub fn capture_screen() -> Result<ScreenCaptureResult, String> {
        let (before_process_id, before_window_title) = Self::foreground_window_info();
        let (bgra, width, height) = native_capture::capture_primary()?;
        let (foreground_process_id, foreground_window_title) = Self::foreground_window_info();
        if before_process_id != foreground_process_id || before_window_title != foreground_window_title {
            return Err("The foreground window changed while the screenshot was being captured. Please retry.".to_string());
        }
        let pixel_count = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| "Screen dimensions are too large.".to_string())?;
        if bgra.len() != pixel_count.saturating_mul(4) {
            return Err("Windows returned an incomplete screen image.".to_string());
        }

        let mut rgb = Vec::with_capacity(pixel_count.saturating_mul(3));
        for pixel in bgra.chunks_exact(4) {
            rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }

        use image::ImageEncoder;
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 65)
            .write_image(&rgb, width, height, image::ExtendedColorType::Rgb8)
            .map_err(|error| format!("Could not encode the screen image: {error}"))?;

        Ok(ScreenCaptureResult {
            base64_image: BASE64.encode(jpeg),
            width,
            height,
            timestamp: chrono::Utc::now().timestamp(),
            foreground_process_id,
            foreground_window_title,
        })
    }

    fn foreground_window_info() -> (u32, String) {
        #[cfg(target_os = "windows")]
        {
            use std::ffi::c_void;
            #[link(name = "user32")]
            extern "system" {
                fn GetForegroundWindow() -> *mut c_void;
                fn GetWindowThreadProcessId(window: *mut c_void, process_id: *mut u32) -> u32;
                fn GetWindowTextW(window: *mut c_void, text: *mut u16, max_count: i32) -> i32;
            }

            unsafe {
                let window = GetForegroundWindow();
                if window.is_null() { return (0, String::new()); }
                let mut process_id = 0;
                GetWindowThreadProcessId(window, &mut process_id);
                let mut title = [0u16; 512];
                let length = GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32);
                (process_id, String::from_utf16_lossy(&title[..length.max(0) as usize]))
            }
        }
        #[cfg(not(target_os = "windows"))]
        { (0, String::new()) }
    }
}

#[cfg(target_os = "windows")]
mod native_capture {
    use std::{ffi::c_void, ptr, slice};

    const SRCCOPY: u32 = 0x00CC0020;
    const CAPTUREBLT: u32 = 0x40000000;
    const DIB_RGB_COLORS: u32 = 0;
    const BI_RGB: u32 = 0;

    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bit_count: u16,
        compression: u32,
        image_size: u32,
        x_pixels_per_meter: i32,
        y_pixels_per_meter: i32,
        colors_used: u32,
        colors_important: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct RgbQuad { blue: u8, green: u8, red: u8, reserved: u8 }

    #[repr(C)]
    struct BitmapInfo { header: BitmapInfoHeader, colors: [RgbQuad; 1] }

    #[link(name = "user32")]
    extern "system" {
        fn GetDC(window: *mut c_void) -> *mut c_void;
        fn ReleaseDC(window: *mut c_void, dc: *mut c_void) -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
    }

    #[link(name = "gdi32")]
    extern "system" {
        fn CreateCompatibleDC(dc: *mut c_void) -> *mut c_void;
        fn DeleteDC(dc: *mut c_void) -> i32;
        fn CreateDIBSection(dc: *mut c_void, info: *const BitmapInfo, usage: u32, bits: *mut *mut c_void, section: *mut c_void, offset: u32) -> *mut c_void;
        fn SelectObject(dc: *mut c_void, object: *mut c_void) -> *mut c_void;
        fn BitBlt(dest_dc: *mut c_void, x: i32, y: i32, width: i32, height: i32, src_dc: *mut c_void, src_x: i32, src_y: i32, operation: u32) -> i32;
        fn DeleteObject(object: *mut c_void) -> i32;
    }

    pub fn capture_primary() -> Result<(Vec<u8>, u32, u32), String> {
        unsafe {
            let width = GetSystemMetrics(0);
            let height = GetSystemMetrics(1);
            if width <= 0 || height <= 0 || width > 16384 || height > 16384 {
                return Err("Windows reported an invalid primary-monitor size.".to_string());
            }
            let byte_count = (width as usize)
                .checked_mul(height as usize)
                .and_then(|count| count.checked_mul(4))
                .filter(|count| *count <= 512 * 1024 * 1024)
                .ok_or_else(|| "Primary-monitor screenshot is too large.".to_string())?;

            let screen_dc = GetDC(ptr::null_mut());
            if screen_dc.is_null() { return Err("Could not access the Windows desktop.".to_string()); }
            let memory_dc = CreateCompatibleDC(screen_dc);
            if memory_dc.is_null() {
                ReleaseDC(ptr::null_mut(), screen_dc);
                return Err("Could not allocate a Windows screen-capture context.".to_string());
            }

            let info = BitmapInfo {
                header: BitmapInfoHeader {
                    size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                    width,
                    height: -height,
                    planes: 1,
                    bit_count: 32,
                    compression: BI_RGB,
                    image_size: byte_count as u32,
                    x_pixels_per_meter: 0,
                    y_pixels_per_meter: 0,
                    colors_used: 0,
                    colors_important: 0,
                },
                colors: [RgbQuad { blue: 0, green: 0, red: 0, reserved: 0 }],
            };
            let mut bits = ptr::null_mut();
            let bitmap = CreateDIBSection(screen_dc, &info, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0);
            if bitmap.is_null() || bits.is_null() {
                if !bitmap.is_null() { DeleteObject(bitmap); }
                DeleteDC(memory_dc);
                ReleaseDC(ptr::null_mut(), screen_dc);
                return Err("Windows could not create the screenshot bitmap.".to_string());
            }

            let old_bitmap = SelectObject(memory_dc, bitmap);
            if old_bitmap.is_null() || old_bitmap as isize == -1 {
                DeleteObject(bitmap);
                DeleteDC(memory_dc);
                ReleaseDC(ptr::null_mut(), screen_dc);
                return Err("Windows could not prepare the screenshot bitmap.".to_string());
            }

            let copied = BitBlt(memory_dc, 0, 0, width, height, screen_dc, 0, 0, SRCCOPY | CAPTUREBLT) != 0;
            let pixels = if copied { Some(slice::from_raw_parts(bits.cast::<u8>(), byte_count).to_vec()) } else { None };
            SelectObject(memory_dc, old_bitmap);
            DeleteObject(bitmap);
            DeleteDC(memory_dc);
            ReleaseDC(ptr::null_mut(), screen_dc);

            match pixels {
                Some(data) => Ok((data, width as u32, height as u32)),
                None => Err("Windows could not copy pixels from the primary monitor.".to_string()),
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod native_capture {
    pub fn capture_primary() -> Result<(Vec<u8>, u32, u32), String> {
        Err("Native screen capture is currently available on Windows only.".to_string())
    }
}
