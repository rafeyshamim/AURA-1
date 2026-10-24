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
            is_roaming: Arc::new(AtomicBool::new(false)),
            step_angle: 0.0,
            app_handle: None,
        }
    }

    pub fn set_app_handle(&mut self, app: AppHandle) {
        self.app_handle = Some(app.clone());
        self.refresh_display_info();
    }

    /// Refresh actual screen bounds and window position from the OS
    pub fn refresh_display_info(&mut self) {
        if let Some(app) = &self.app_handle {
            if let Some(window) = app.get_webview_window("main") {
                if let Ok(Some(monitor)) = window.current_monitor() {
                    let size = monitor.size();
                    self.screen_width = size.width as i32;
                    self.screen_height = size.height as i32;
                }
                if let Ok(pos) = window.outer_position() {
                    self.current_position.x = pos.x;
                    self.current_position.y = pos.y;
                }
            }
        }
    }

    /// Return a human-readable description of where AURA is located
    pub fn get_location_description(&mut self) -> String {
        self.refresh_display_info();

        let x = self.current_position.x;
        let y = self.current_position.y;
        let sw = self.screen_width.max(1);
        let sh = self.screen_height.max(1);

        let horiz = if x < sw / 3 {
            "left"
        } else if x > (sw * 2) / 3 {
            "right"
        } else {
            "center"
        };

        let vert = if y < sh / 3 {
            "top"
        } else if y > (sh * 2) / 3 {
            "bottom"
        } else {
            "middle"
        };

        let region = match (vert, horiz) {
            ("middle", "center") => "center of the screen",
            (v, h) => &format!("{}-{} area", v, h),
        };

        format!(
            "Currently positioned in the {}. Roaming status: {}.",
            region,
            if self.is_roaming.load(Ordering::SeqCst) { "Floating" } else { "Stationary" }
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

        let start_x = self.current_position.x as f32;
        let start_y = self.current_position.y as f32;
        let dest_x = target_x.clamp(0, (self.screen_width - 540).max(0)) as f32;
        let dest_y = target_y.clamp(0, (self.screen_height - 460).max(0)) as f32;

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

    /// Move to named preset positions
    pub async fn move_to_preset(&mut self, preset: &str) -> Result<String, String> {
        self.refresh_display_info();
        let sw = self.screen_width;
        let sh = self.screen_height;

        let (tx, ty) = match preset.to_lowercase().replace('_', "-").as_str() {
            "top-right" | "topright" | "tr" => (sw - 560, 40),
            "bottom-right" | "bottomright" | "br" | "dock" => (sw - 560, sh - 480),
            "top-left" | "topleft" | "tl" => (40, 40),
            "bottom-left" | "bottomleft" | "bl" => (40, sh - 480),
            "center" | "middle" => ((sw - 540) / 2, (sh - 460) / 2),
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

        self.is_roaming.store(true, Ordering::SeqCst);
        self.roam_mode = RoamMode::AutonomousFloat;
        crate::logger::log("MOVEMENT", "INFO", "AURA autonomous floating roam activated.");

        let app_opt = self.app_handle.clone();
        let is_roam_flag = self.is_roaming.clone();
        let sw = self.screen_width;
        let sh = self.screen_height;

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
            let center_x = ((sw - 540) / 2) as f32;
            let center_y = ((sh - 460) / 2) as f32;
            let radius_x = 380.0_f32;
            let radius_y = 220.0_f32;

            while is_roam_flag.load(Ordering::SeqCst) {
                angle += 0.025;
                if angle > 6.283185 {
                    angle = 0.0;
                }

                // Fluid Lissajous figure-8 floating trajectory
                let new_x = (center_x + angle.sin() * radius_x) as i32;
                let new_y = (center_y + (angle * 2.0).cos() * radius_y) as i32;

                let _ = window.set_position(Position::Physical(PhysicalPosition {
                    x: new_x.clamp(40, (sw - 560).max(40)),
                    y: new_y.clamp(40, (sh - 480).max(40)),
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
