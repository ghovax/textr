#![warn(clippy::unwrap_used)]

use clap::{Parser, Subcommand};
use custom_error::CustomError;
use glium::glutin::event::{Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use glium::glutin::event_loop::EventLoop;
use itertools::Itertools as _;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use crate::configuration_format::Configuration;
use crate::{graphics::GraphicsHandle, layouting::FontStyles};

mod configuration_format;
mod custom_error;
mod document_format;
mod graphics;
mod layouting;

#[derive(Parser)]
#[command(version, long_about = None)]
struct CliArguments {
    #[command(subcommand)]
    command: Option<Commands>,
    #[arg(long = "config", value_name = "json_file")]
    configuration_file_path: PathBuf,
    #[arg(long = "debug")]
    debug_mode: bool,
}

#[derive(Subcommand)]
enum Commands {
    Test {
        #[command(subcommand)]
        test_flag: Option<TestFlag>,
    },
    Load {
        #[arg(long = "document")]
        document_path: Option<PathBuf>,
    },
}

#[derive(Debug, Copy, Clone, Subcommand)]
enum TestFlag {
    #[command(name = "generate")]
    GenerateReferenceImages,
    #[command(name = "compare")]
    CompareWithReferenceImages,
}

fn main() {
    if let Err(error) = fallible_main() {
        log::error!("{}", error);
    }
}

fn fallible_main() -> Result<(), CustomError> {
    if cfg!(target_os = "linux") && env::var("WINIT_UNIX_BACKEND").is_err() {
        env::set_var("WINIT_UNIX_BACKEND", "x11");
    }

    let CliArguments { command, configuration_file_path, debug_mode } = CliArguments::parse();

    if debug_mode {
        env_logger::builder().filter_level(log::LevelFilter::Debug).init();
    } else {
        env_logger::builder().filter_level(log::LevelFilter::Info).init();
    }

    let mut font_styles_map: HashMap<String, FontStyles> = HashMap::new();
    layouting::load_fonts(&mut font_styles_map)?;
    log::info!(
        "Initialized the program with the languages {:?} supported",
        font_styles_map.keys().collect_vec()
    );

    let configuration_file_contents =
        std::fs::read_to_string(configuration_file_path).map_err(|error| {
            CustomError::with_source("Failed to read the configuration file".into(), error.into())
        })?;
    let configuration: Configuration =
        serde_json::from_str(&configuration_file_contents).map_err(|error| {
            CustomError::with_source("Failed to parse the configuration file".into(), error.into())
        })?;

    let event_loop = EventLoop::new();
    let mut graphics_handle = GraphicsHandle::new(&event_loop, configuration)?;

    if let Some(commands) = command {
        match commands {
            Commands::Test { test_flag } => {
                if let Some(test_flag) = test_flag {
                    graphics_handle.run_tests(test_flag, font_styles_map)?;
                } else {
                    log::error!("No test flag specified, the possible test flags are `generate` and `compare`");
                }
            }
            Commands::Load { document_path } => {
                let (document, document_path) = document_format::load_document(document_path)?;
                graphics_handle.set_window_title(document_path);

                event_loop.run(move |event, _, control_flow| {
                    control_flow.set_wait();

                    match event {
                        Event::WindowEvent { event, .. } => match event {
                            WindowEvent::KeyboardInput {
                                input:
                                    KeyboardInput {
                                        virtual_keycode: Some(VirtualKeyCode::Escape), ..
                                    },
                                ..
                            }
                            | WindowEvent::CloseRequested => control_flow.set_exit(),

                            _ => (),
                        },
                        Event::RedrawRequested(_) => {
                            if let Err(error) =
                                graphics_handle.draw_glyphs(&document, &font_styles_map)
                            {
                                log::error!("{}", error);
                                control_flow.set_exit_with_code(1);
                            }
                        }
                        _ => (),
                    }
                });
            }
        }
    } else {
        log::error!("No command specified, run instead with `--help` to see the possible commands");
    }

    Ok(())
}
