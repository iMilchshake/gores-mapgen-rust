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

use macroquad::{color::*, miniquad, window::*};
use miniquad::conf::{Conf, Platform};

const DISABLE_VSYNC: bool = true;
const STEPS_PER_FRAME: usize = 50;

fn window_conf() -> Conf {
    Conf {
        window_title: "egui with macroquad".to_owned(),
        platform: Platform {
            swap_interval: match DISABLE_VSYNC {
                true => Some(0), // set swap_interval to 0 to disable vsync
                false => None,
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut editor = Editor::new(EditorPlayback::Paused);
    let mut fps_ctrl = FPSControl::new().with_max_fps(60);

    let spawn = Position::new(50, 50);
    let mut map = Map::new(300, 300, BlockType::Hookable, spawn.clone());
    let mut rnd = Random::new("iMilchshake".to_string(), vec![8, 6, 6, 5]);
    let config = GenerationConfig::new(3, 5, 0.5, 0.2);
    let kernel_table = ValidKernelTable::new(config.max_outer_size);

    let waypoints: Vec<Position> = vec![
        Position::new(250, 50),
        Position::new(250, 250),
        Position::new(50, 250),
        Position::new(50, 50),
    ];

    let init_inner_kernel = Kernel::new(
        config.max_inner_size,
        *kernel_table
            .get_valid_radii(&config.max_inner_size)
            .last()
            .unwrap(),
    );
    let init_outer_kernel = kernel_table.get_min_valid_outer_kernel(&init_inner_kernel);
    let mut walker = CuteWalker::new(spawn, waypoints, init_inner_kernel, init_outer_kernel);

    loop {
        fps_ctrl.on_frame_start();
        editor.on_frame_start();

        // walker logic
        if editor.playback.is_not_paused() {
            for _ in 0..STEPS_PER_FRAME {
                // check if walker has reached goal position
                if walker.is_goal_reached() == Some(true) {
                    walker.next_waypoint().unwrap_or_else(|_| {
                        println!("pause due to reaching last checkpoint");
                        editor.playback.pause();
                    });
                }

                // randomly mutate kernel
                walker.mutate_kernel(&config, &mut rnd, &kernel_table);

                // perform one greedy step
                if let Err(err) = walker.probabilistic_step(&mut map, &mut rnd) {
                    println!("greedy step failed: '{:}' - pausing...", err);
                    editor.playback.pause();
                }

                // walker did a step using SingleStep -> now pause
                if editor.playback == EditorPlayback::SingleStep {
                    editor.playback.pause();
                    break; // skip following steps for this frame!
                }
            }
        }

        editor.define_egui(&walker);
        editor.set_cam(editor.get_display_factor(&map));
        editor.handle_user_inputs(&map);

        clear_background(WHITE);
        draw_grid_blocks(&map.grid);
        draw_waypoints(&walker.waypoints);
        draw_walker(&walker);
        draw_walker_kernel(&walker, KernelType::Outer);
        draw_walker_kernel(&walker, KernelType::Inner);

        egui_macroquad::draw();

        fps_ctrl.wait_for_next_frame().await;
    }
}
