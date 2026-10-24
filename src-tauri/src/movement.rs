use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Manager, PhysicalPosition, Position};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopPosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RoamMode {
    Stationary,
    AutonomousFloat,
    DockBottomRight,
    DockTopRight,
    Center,
    DockTopLeft,
    DockBottomLeft,
}

pub struct MovementController {
    pub current_position: DesktopPosition,
    pub roam_mode: RoamMode,
    pub screen_width: i32,
    pub screen_height: i32,
    pub monitor_x: i32,
    pub monitor_y: i32,
    pub window_width: i32,
    pub window_height: i32,
    pub is_roaming: Arc<AtomicBool>,
    step_angle: f32,
    app_handle: Option<AppHandle>,
}

impl MovementController {
    pub fn new() -> Self {
        Self {
            current_position: DesktopPosition { x: 1200, y: 600 },
            roam_mode: RoamMode::Stationary,
            screen_width: 1920,
            screen_height: 1080,
            monitor_x: 0,
            monitor_y: 0,
            window_width: 540,
            window_height: 460,
            is_roaming: Arc::new(AtomicBool::new(false)),
            step_angle: 0.0,
            app_handle: None,
        }
    }

    pub fn set_app_handle(&mut self, app: AppHandle) {
        self.app_handle = Some(app.clone());
        self.refresh_display_info();
    }

    /// Refresh actual screen bounds and window position dynamically from the OS at runtime
    pub fn refresh_display_info(&mut self) {
        if let Some(app) = &self.app_handle {
            if let Some(window) = app.get_webview_window("main") {
                if let Ok(Some(monitor)) = window.current_monitor() {
                    let size = monitor.size();
                    let mpos = monitor.position();
                    self.screen_width = size.width as i32;
                    self.screen_height = size.height as i32;
                    self.monitor_x = mpos.x;
                    self.monitor_y = mpos.y;
                }
                if let Ok(pos) = window.outer_position() {
                    self.current_position.x = pos.x;
                    self.current_position.y = pos.y;
                }
                if let Ok(sz) = window.outer_size() {
                    self.window_width = sz.width as i32;
                    self.window_height = sz.height as i32;
                }
            }
        }
    }

    /// Return an accurate human-readable description of where AURA is located dynamically at runtime
    pub fn get_location_description(&mut self) -> String {
        self.refresh_display_info();

        let mon_w = self.screen_width.max(1);
        let mon_h = self.screen_height.max(1);
        let mon_x = self.monitor_x;
        let mon_y = self.monitor_y;
        let win_w = self.window_width.max(1);
        let win_h = self.window_height.max(1);

        // Calculate center of the visual orb relative to the active display monitor
        let orb_center_x = (self.current_position.x + win_w / 2) - mon_x;
        let orb_center_y = (self.current_position.y + win_h / 2) - mon_y;

        let horiz = if orb_center_x < (mon_w * 38) / 100 {
            "left"
        } else if orb_center_x > (mon_w * 62) / 100 {
            "right"
        } else {
            "center"
        };

        let vert = if orb_center_y < (mon_h * 38) / 100 {
            "top"
        } else if orb_center_y > (mon_h * 62) / 100 {
            "bottom"
        } else {
            "middle"
        };

        let region = match (vert, horiz) {
            ("middle", "center") => "center of your screen".to_string(),
            ("top", "center") => "top-center of your screen".to_string(),
            ("bottom", "center") => "bottom-center of your screen".to_string(),
            ("middle", "left") => "middle-left edge of your screen".to_string(),
            ("middle", "right") => "middle-right edge of your screen".to_string(),
            ("top", "left") => "top-left corner of your screen".to_string(),
            ("top", "right") => "top-right corner of your screen".to_string(),
            ("bottom", "left") => "bottom-left corner of your screen".to_string(),
            ("bottom", "right") => "bottom-right corner of your screen".to_string(),
            (v, h) => format!("{}-{} area of your screen", v, h),
        };

        let roam_status = if self.is_roaming.load(Ordering::SeqCst) {
            "Floating autonomously"
        } else {
            "Stationary"
        };

        format!(
            "Currently positioned at the {} (X: {}, Y: {} on display {}x{}). Status: {}.",
            region, self.current_position.x, self.current_position.y, mon_w, mon_h, roam_status
        )
    }

    /// Move the window smoothly to an absolute (X, Y) coordinate with cosine interpolation
    pub async fn glide_to(&mut self, target_x: i32, target_y: i32) -> Result<(), String> {
        self.stop_roaming();
        self.refresh_display_info();

        let app = match &self.app_handle {
            Some(a) => a.clone(),
            None => return Err("No app handle available".to_string()),
        };

        let window = match app.get_webview_window("main") {
            Some(w) => w,
            None => return Err("Main window not found".to_string()),
        };

        let win_w = self.window_width;
        let win_h = self.window_height;
        let min_x = self.monitor_x;
        let max_x = self.monitor_x + (self.screen_width - win_w).max(0);
        let min_y = self.monitor_y;
        let max_y = self.monitor_y + (self.screen_height - win_h).max(0);

        let start_x = self.current_position.x as f32;
        let start_y = self.current_position.y as f32;
        let dest_x = target_x.clamp(min_x, max_x) as f32;
        let dest_y = target_y.clamp(min_y, max_y) as f32;

        let steps = 30;
        for i in 1..=steps {
            let progress = i as f32 / steps as f32;
            // Cosine easing for ultra-smooth gliding
            let eased = (1.0 - (progress * std::f32::consts::PI).cos()) / 2.0;
            let cur_x = (start_x + (dest_x - start_x) * eased) as i32;
            let cur_y = (start_y + (dest_y - start_y) * eased) as i32;

            let _ = window.set_position(Position::Physical(PhysicalPosition { x: cur_x, y: cur_y }));
            tokio::time::sleep(tokio::time::Duration::from_millis(14)).await;
        }

        self.current_position.x = dest_x as i32;
        self.current_position.y = dest_y as i32;
        Ok(())
    }

    /// Move to named preset positions dynamically calculated at runtime
    pub async fn move_to_preset(&mut self, preset: &str) -> Result<String, String> {
        self.refresh_display_info();
        let mon_w = self.screen_width;
        let mon_h = self.screen_height;
        let mon_x = self.monitor_x;
        let mon_y = self.monitor_y;
        let win_w = self.window_width;
        let win_h = self.window_height;
        let pad_x = 30;
        let pad_y = 30;

        let (tx, ty) = match preset.to_lowercase().replace('_', "-").as_str() {
            "top-right" | "topright" | "tr" => (mon_x + mon_w - win_w - pad_x, mon_y + pad_y),
            "bottom-right" | "bottomright" | "br" | "dock" => (mon_x + mon_w - win_w - pad_x, mon_y + mon_h - win_h - pad_y),
            "top-left" | "topleft" | "tl" => (mon_x + pad_x, mon_y + pad_y),
            "bottom-left" | "bottomleft" | "bl" => (mon_x + pad_x, mon_y + mon_h - win_h - pad_y),
            "center" | "middle" => (mon_x + (mon_w - win_w) / 2, mon_y + (mon_h - win_h) / 2),
            "top-center" | "top" => (mon_x + (mon_w - win_w) / 2, mon_y + pad_y),
            "bottom-center" | "bottom" => (mon_x + (mon_w - win_w) / 2, mon_y + mon_h - win_h - pad_y),
            "middle-left" | "left" => (mon_x + pad_x, mon_y + (mon_h - win_h) / 2),
            "middle-right" | "right" => (mon_x + mon_w - win_w - pad_x, mon_y + (mon_h - win_h) / 2),
            other => return Err(format!("Unknown position preset: {}", other)),
        };

        crate::logger::log("MOVEMENT", "INFO", &format!("Gliding AURA to '{}' (x: {}, y: {})", preset, tx, ty));
        self.glide_to(tx, ty).await?;
        Ok(format!("Moved to {}", preset))
    }

    /// Start autonomous gentle floating / roaming loop
    pub fn start_autonomous_roam(&mut self) {
        if self.is_roaming.load(Ordering::SeqCst) {
            return;
        }

        self.refresh_display_info();
        self.is_roaming.store(true, Ordering::SeqCst);
        self.roam_mode = RoamMode::AutonomousFloat;
        crate::logger::log("MOVEMENT", "INFO", "AURA autonomous floating roam activated.");

        let app_opt = self.app_handle.clone();
        let is_roam_flag = self.is_roaming.clone();
        let mon_w = self.screen_width;
        let mon_h = self.screen_height;
        let mon_x = self.monitor_x;
        let mon_y = self.monitor_y;
        let win_w = self.window_width;
        let win_h = self.window_height;

        tauri::async_runtime::spawn(async move {
            let app = match app_opt {
                Some(a) => a,
                None => return,
            };
            let window = match app.get_webview_window("main") {
                Some(w) => w,
                None => return,
            };

            let mut angle: f32 = 0.0;
            let center_x = (mon_x + (mon_w - win_w) / 2) as f32;
            let center_y = (mon_y + (mon_h - win_h) / 2) as f32;
            let radius_x = ((mon_w - win_w) / 3).max(100) as f32;
            let radius_y = ((mon_h - win_h) / 4).max(80) as f32;

            while is_roam_flag.load(Ordering::SeqCst) {
                angle += 0.025;
                if angle > 6.283185 {
                    angle = 0.0;
                }

                // Fluid Lissajous figure-8 floating trajectory
                let new_x = (center_x + angle.sin() * radius_x) as i32;
                let new_y = (center_y + (angle * 2.0).cos() * radius_y) as i32;

                let min_x = mon_x + 30;
                let max_x = mon_x + mon_w - win_w - 30;
                let min_y = mon_y + 30;
                let max_y = mon_y + mon_h - win_h - 30;

                let _ = window.set_position(Position::Physical(PhysicalPosition {
                    x: new_x.clamp(min_x, max_x),
                    y: new_y.clamp(min_y, max_y),
                }));

                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
        });
    }

    /// Stop autonomous roaming and freeze in place
    pub fn stop_roaming(&mut self) {
        if self.is_roaming.load(Ordering::SeqCst) {
            self.is_roaming.store(false, Ordering::SeqCst);
            self.roam_mode = RoamMode::Stationary;
            crate::logger::log("MOVEMENT", "INFO", "AURA autonomous floating stopped. Window stationary.");
        }
    }
}
