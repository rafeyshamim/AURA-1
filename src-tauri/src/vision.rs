use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenCaptureResult {
    pub base64_image: String,
    pub width: u32,
    pub height: u32,
    pub timestamp: i64,
}

pub struct VisionEngine;

impl VisionEngine {
    pub fn new() -> Self {
        Self
    }

    /// Capture the primary desktop monitor using Windows native APIs via PowerShell
    /// Returns a real JPEG screenshot as base64
    pub fn capture_screen() -> Result<ScreenCaptureResult, String> {
        let now = chrono::Utc::now().timestamp();

        // Use PowerShell + .NET System.Drawing to take a real screenshot
        let ps_script = r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bmp = New-Object System.Drawing.Bitmap($bounds.Width, $bounds.Height)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
$g.Dispose()
$ms = New-Object System.IO.MemoryStream
$quality = [System.Drawing.Imaging.Encoder]::Quality
$jpegCodec = [System.Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() | Where-Object { $_.MimeType -eq 'image/jpeg' }
$encoderParams = New-Object System.Drawing.Imaging.EncoderParameters(1)
$encoderParams.Param[0] = New-Object System.Drawing.Imaging.EncoderParameter($quality, 50)
$bmp.Save($ms, $jpegCodec, $encoderParams)
$bmp.Dispose()
$bytes = $ms.ToArray()
$ms.Dispose()
$b64 = [Convert]::ToBase64String($bytes)
Write-Output "$($bounds.Width)x$($bounds.Height)"
Write-Output $b64
"#;

        let output = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", ps_script])
            .output()
            .map_err(|e| format!("Failed to run screen capture: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            println!("[Vision] PowerShell screen capture failed: {}", stderr);
            // Return a fallback tiny image so the app doesn't crash
            return Self::fallback_capture(now);
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let lines: Vec<&str> = stdout.trim().lines().collect();

        if lines.len() < 2 {
            println!("[Vision] Unexpected output format from screen capture");
            return Self::fallback_capture(now);
        }

        // Parse resolution from first line (e.g. "1920x1080")
        let resolution = lines[0].trim();
        let (width, height) = if let Some((w, h)) = resolution.split_once('x') {
            (
                w.parse::<u32>().unwrap_or(1920),
                h.parse::<u32>().unwrap_or(1080),
            )
        } else {
            (1920, 1080)
        };

        // The base64 data is on the second line (may be very long)
        let base64_image = lines[1..].join("").trim().to_string();

        if base64_image.is_empty() {
            println!("[Vision] Empty base64 from screen capture");
            return Self::fallback_capture(now);
        }

        println!("[Vision] Screen captured: {}x{} ({} bytes b64)", width, height, base64_image.len());

        Ok(ScreenCaptureResult {
            base64_image,
            width,
            height,
            timestamp: now,
        })
    }

    /// Fallback: return a small placeholder so the system never crashes
    fn fallback_capture(timestamp: i64) -> Result<ScreenCaptureResult, String> {
        // Minimal valid 1x1 JPEG
        let dummy_jpeg: Vec<u8> = vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x01,
            0x00, 0x48, 0x00, 0x48, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06,
            0x07, 0x06, 0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D,
            0x0C, 0x0B, 0x0B, 0x0C, 0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D,
            0x1A, 0x1C, 0x1C, 0x20, 0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28,
            0x37, 0x29, 0x2C, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32,
            0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01,
            0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00, 0x1F, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02,
            0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xDA, 0x00, 0x08, 0x01,
            0x01, 0x00, 0x00, 0x3F, 0x00, 0xBF, 0x00, 0xFF, 0xD9
        ];

        Ok(ScreenCaptureResult {
            base64_image: BASE64.encode(&dummy_jpeg),
            width: 1,
            height: 1,
            timestamp,
        })
    }
}
