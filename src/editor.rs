use std::time::Instant;

use crate::{CuteWalker, Map};
use egui::Pos2;
use egui::{epaint::Shadow, Color32, Frame, Label, Margin};
use macroquad::camera::{set_camera, Camera2D};
use macroquad::input::{
    is_key_pressed, is_mouse_button_down, is_mouse_button_released, mouse_position, mouse_wheel,
    KeyCode, MouseButton,
};
use macroquad::math::{Rect, Vec2};
use macroquad::time::get_fps;
use macroquad::window::{screen_height, screen_width};
use rand_distr::num_traits::Zero;

const ZOOM_FACTOR: f32 = 0.9;
const AVG_FPS_FACTOR: f32 = 0.25; // how much current fps is weighted into the rolling average

pub fn window_frame() -> Frame {
    Frame {
        fill: Color32::from_gray(0),
        inner_margin: Margin::same(5.0),
        shadow: Shadow::NONE,
        ..Default::default()
    }
}

#[derive(PartialEq, Debug)]
pub enum EditorPlayback {
    Paused,
    SingleStep,
    Playing,
}

impl EditorPlayback {
    pub fn is_not_paused(&self) -> bool {
        match self {
            EditorPlayback::Paused => false,
            EditorPlayback::Playing | EditorPlayback::SingleStep => true,
        }
    }

    pub fn toggle(&mut self) {
        *self = match self {
            EditorPlayback::Paused => EditorPlayback::Playing,
            EditorPlayback::Playing | EditorPlayback::SingleStep => EditorPlayback::Paused,
        };
    }

    pub fn pause(&mut self) {
        *self = EditorPlayback::Paused;
    }
}
pub struct Editor {
    pub playback: EditorPlayback,
    pub canvas: Option<egui::Rect>,
    pub egui_wants_mouse: Option<bool>,
    pub average_fps: f32,
    zoom: f32,
    offset: Vec2,
    cam: Option<Camera2D>,
    last_mouse: Option<Vec2>,
}

impl Editor {
    pub fn new(initial_playback: EditorPlayback) -> Editor {
        Editor {
            playback: initial_playback,
            canvas: None,
            egui_wants_mouse: None,
            average_fps: 0.0,
            zoom: 1.0,
            offset: Vec2::ZERO,
            cam: None,
            last_mouse: None,
        }
    }

    pub fn on_frame_start(&mut self) {
        // framerate control
        self.average_fps =
            (self.average_fps * (1. - AVG_FPS_FACTOR)) + (get_fps() as f32 * AVG_FPS_FACTOR);

        // this value is only valid for each frame after calling define_egui()
        self.canvas = None;
    }

    pub fn get_display_factor(&self, map: &Map) -> f32 {
        let canvas = self
            .canvas
            .expect("expect define_egui() to be called before");

        f32::min(
            canvas.width() / map.width as f32,
            canvas.height() / map.height as f32,
        )
    }

    pub fn define_egui(&mut self, walker: &CuteWalker) {
        // define egui
        egui_macroquad::ui(|egui_ctx| {
            egui::SidePanel::right("right_panel").show(egui_ctx, |ui| {
                ui.label("hello world");
                // toggle pause
                if ui.button("toggle").clicked() {
                    self.playback.toggle();
                }

                // pause, allow single step
                if ui.button("single").clicked() {
                    self.playback = EditorPlayback::SingleStep;
                }
                ui.separator();
            });

            egui::Window::new("DEBUG")
                .frame(window_frame())
                .default_open(false)
                .show(egui_ctx, |ui| {
                    ui.add(Label::new(format!("fps: {:}", get_fps())));
                    ui.add(Label::new(format!(
                        "avg: {:}",
                        self.average_fps.round() as usize
                    )));
                    ui.add(Label::new(format!("{:?}", walker)));
                    ui.add(Label::new(format!("{:?}", self.playback)));
                });

            // store remaining space for macroquad drawing
            self.canvas = Some(egui_ctx.available_rect());
            self.egui_wants_mouse = Some(egui_ctx.wants_pointer_input());
        });
    }

    fn mouse_in_viewport(cam: &Camera2D) -> bool {
        let (mouse_x, mouse_y) = mouse_position();
        0.0 <= mouse_x
            && mouse_x <= cam.viewport.unwrap().2 as f32
            && 0.0 <= mouse_y
            && mouse_y <= cam.viewport.unwrap().3 as f32
    }

    /// this should result in the exact same behaviour as if not using a camera at all
    pub fn reset_camera() {
        // no idea why i dont have to use negative values here???
        set_camera(&Camera2D::from_display_rect(Rect::new(
            0.0,
            0.0,
            screen_width(),
            screen_height(),
        )));
    }

    pub fn set_cam(&mut self, display_factor: f32) {
        let canvas_width = self.canvas.unwrap().width();
        let canvas_height = self.canvas.unwrap().height();    

        let mut cam = Camera2D::from_display_rect(Rect::new(0.0, 0.0, canvas_width, canvas_height));
        cam.viewport = Some((0, 0, canvas_width as i32, canvas_height as i32));

        let zoomed_offset = self.offset * cam.zoom;
        cam.target = -self.offset * display_factor - zoomed_offset / (self.zoom);
        cam.zoom *= self.zoom;

        set_camera(&cam);
        self.cam = Some(cam);
    }

    pub fn handle_user_inputs(&mut self, map: &Map) {
        if is_key_pressed(KeyCode::R) {
            self.zoom = 1.0;
            self.offset = Vec2::ZERO;
        }

        if is_key_pressed(KeyCode::E) {
            let t0 = Instant::now();
            map.export();
            let time = Instant::now().duration_since(t0);
            dbg!(time);
        }

        // handle mouse inputs
        let mouse_wheel_y = mouse_wheel().1;
        if !mouse_wheel_y.is_zero() {
            if mouse_wheel_y.is_sign_positive() {
                self.zoom /= ZOOM_FACTOR;
            } else {
                self.zoom *= ZOOM_FACTOR;
            }
        }

        let egui_wants_mouse = self
            .egui_wants_mouse
            .expect("expect to be set after define_gui()");

        if !egui_wants_mouse
            && is_mouse_button_down(MouseButton::Left)
            && Editor::mouse_in_viewport(&self.cam.unwrap())
        {
            let mouse = mouse_position();

            if let Some(last_mouse) = self.last_mouse {
                let display_factor = self.get_display_factor(map);
                let local_delta = Vec2::new(mouse.0, mouse.1) - last_mouse;
                self.offset += local_delta / (self.zoom * display_factor);
            }

            self.last_mouse = Some(mouse.into());

        // mouse pressed for first frame, reset last position
        } else if is_mouse_button_released(MouseButton::Left) {
            self.last_mouse = None;
        }
    }
}
