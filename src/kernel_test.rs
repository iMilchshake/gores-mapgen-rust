mod editor;
mod fps_control;
mod grid_render;
mod kernel;
mod map;
mod position;
mod random;
mod walker;

use crate::{
    editor::*,
    fps_control::*,
    grid_render::*,
    kernel::{Kernel, ValidKernelTable},
    map::*,
    position::*,
    random::*,
    walker::*,
};

use egui::Label;
use macroquad::color::*;
use macroquad::shapes::*;
use macroquad::window::clear_background;

fn state_to_kernels(
    state: &mut State,
    kernel_table: &ValidKernelTable,
) -> (Kernel, Kernel, usize, usize) {
    let outer_size = state.outer_size_index * 2 + 1;
    let outer_radii = kernel_table.get_valid_radii(&outer_size);
    let outer_radius = outer_radii.get(state.outer_radius_index).unwrap();

    let max_valid_inner_radius = kernel_table.get_max_valid_inner_radius(outer_radius);

    let inner_size = state.inner_size_index * 2 + 1;
    let mut inner_radii = kernel_table.get_valid_radii(&inner_size).clone();
    dbg!("before", &inner_radii);
    inner_radii.retain(|&x| x <= max_valid_inner_radius);

    dbg!(&inner_size, &outer_size, &outer_radius);
    dbg!("after", &inner_radii);

    let inner_radius = if state.inner_radius_index < inner_radii.len().saturating_sub(1) {
        inner_radii.get(state.inner_radius_index).unwrap()
    } else {
        state.inner_radius_index = inner_radii.len().saturating_sub(1);
        inner_radii.get(state.inner_radius_index).unwrap_or(&0)
    };

    dbg!(&state);

    dbg!(&max_valid_inner_radius);

    (
        Kernel::new(inner_size, *inner_radius),
        Kernel::new(outer_size, *outer_radius),
        inner_radii.len().saturating_sub(1),
        outer_radii.len().saturating_sub(1),
    )
}

pub fn define_egui(editor: &mut Editor, state: &mut State, kernel_table: &ValidKernelTable) {
    let (inner_kernel, outer_kernel, inner_radius_max_index, outer_radius_max_index) =
        state_to_kernels(state, kernel_table);

    // define egui
    egui_macroquad::ui(|egui_ctx| {
        egui::Window::new("DEBUG")
            .frame(window_frame())
            .show(egui_ctx, |ui| {
                ui.add(Label::new("TEST".to_string()));
                ui.horizontal(|ui| {
                    ui.label(format!("inner size: {}", inner_kernel.size));
                    if ui.button("-").clicked() {
                        state.inner_size_index = state.inner_size_index.saturating_sub(1);
                        state.inner_radius_index = 0;
                    }
                    if ui.button("+").clicked() {
                        state.inner_size_index = state.inner_size_index.saturating_add(1);
                        state.inner_radius_index = 0;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(format!("outer size: {}", outer_kernel.size));
                    if ui.button("-").clicked() {
                        state.outer_size_index = state.outer_size_index.saturating_sub(1);
                        state.outer_radius_index = 0;
                    }
                    if ui.button("+").clicked() {
                        state.outer_size_index = state.outer_size_index.saturating_add(1);
                        state.outer_radius_index = 0;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(format!("inner radius: {}", inner_kernel.radius));
                    if ui.button("-").clicked() {
                        state.inner_radius_index = state
                            .inner_radius_index
                            .saturating_sub(1)
                            .min(inner_radius_max_index);
                    }
                    if ui.button("+").clicked() {
                        state.inner_radius_index = state
                            .inner_radius_index
                            .saturating_add(1)
                            .min(outer_radius_max_index);
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(format!("outer radius: {}", outer_kernel.radius));
                    if ui.button("-").clicked() {
                        state.outer_radius_index = state.outer_radius_index.saturating_sub(1);
                    }
                    if ui.button("+").clicked() {
                        state.outer_radius_index = state.outer_radius_index.saturating_add(1);
                    }
                });
            });

        // store remaining space for macroquad drawing
        editor.canvas = Some(egui_ctx.available_rect());
        editor.egui_wants_mouse = Some(egui_ctx.wants_pointer_input());
    });
}

#[derive(Debug)]
struct State {
    inner_radius_index: usize,
    outer_radius_index: usize,
    inner_size_index: usize,
    outer_size_index: usize,
}

fn draw_thingy(walker: &CuteWalker, flag: bool) {
    let offset: usize = walker.inner_kernel.size / 2; // offset of kernel wrt. position (top/left)
    let root_pos = Position::new(walker.pos.x - offset, walker.pos.y - offset);
    for ((x, y), kernel_active) in walker.inner_kernel.vector.indexed_iter() {
        if *kernel_active {
            draw_rectangle(
                (root_pos.x + x) as f32,
                (root_pos.y + y) as f32,
                1.0,
                1.0,
                match flag {
                    true => Color::new(0.0, 1.0, 0.0, 0.5),
                    false => Color::new(1.0, 0.0, 0.0, 0.5),
                },
            );
        }
    }

    let size = walker.inner_kernel.size;
    let radius = walker.inner_kernel.radius;

    // very crappy hotfix to deal with different center whether size is even or not
    let offset = match size % 2 == 0 {
        true => 0.0,
        false => 0.5,
    };

    draw_circle_lines(
        (walker.pos.x) as f32 + offset,
        (walker.pos.y) as f32 + offset,
        (radius as f32).sqrt(),
        0.05,
        match flag {
            true => GREEN,
            false => RED,
        },
    );

    draw_circle_lines(
        (walker.pos.x) as f32 + offset,
        (walker.pos.y) as f32 + offset,
        radius as f32,
        0.025,
        match flag {
            true => GREEN,
            false => RED,
        },
    );
}

#[macroquad::main("kernel_test")]
async fn main() {
    let mut editor = Editor::new(EditorPlayback::Paused);
    let map = Map::new(20, 20, BlockType::Hookable, Position::new(0, 0));

    let init_kernel = Kernel::new(1, 0);
    let mut walker = CuteWalker::new(
        Position::new(10, 10),
        vec![Position::new(15, 15)],
        init_kernel.clone(),
        init_kernel.clone(),
    );
    let mut fps_ctrl = FPSControl::new().with_max_fps(60);

    let mut state = State {
        inner_size_index: 0,
        inner_radius_index: 0,
        outer_size_index: 0,
        outer_radius_index: 0,
    };

    let kernel_table = ValidKernelTable::new(19);

    loop {
        fps_ctrl.on_frame_start();
        editor.on_frame_start();
        define_egui(&mut editor, &mut state, &kernel_table);
        editor.set_cam();
        editor.handle_user_inputs(&map);
        clear_background(GRAY);
        draw_walker(&walker);

        let (inner_kernel, outer_kernel, _, _) = state_to_kernels(&mut state, &kernel_table);

        walker.inner_kernel = outer_kernel.clone();
        draw_thingy(&walker, false);

        walker.inner_kernel = inner_kernel.clone();
        draw_thingy(&walker, true);

        egui_macroquad::draw();
        fps_ctrl.wait_for_next_frame().await;
    }
}
