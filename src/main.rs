#![deny(clippy::unwrap_used, clippy::expect_used)]

use clap::Parser;
use custom_error::CustomError;
use glium::glutin::event::{Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use glium::glutin::event_loop::EventLoop;
use itertools::Itertools as _;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

#[cfg(debug_assertions)]
use clap::ValueEnum;

use crate::configuration_format::Configuration;
use crate::{graphics::GraphicSystem, layouting::FontStyles};

mod configuration_format;
mod custom_error;
mod document_format;
mod graphics;
mod layouting;

#[derive(Parser, Debug)]
#[command(version, long_about = None)]
struct CliArguments {
    #[arg(long = "document", value_name = "json_file")]
    document_path: Option<PathBuf>,
    #[arg(long = "config", value_name = "json_config_file")]
    configuration_file_path: Option<PathBuf>,
    #[arg(long = "debug", value_name = "bool", action = clap::ArgAction::SetTrue, default_value_t = false)]
    debug_mode: bool,
    #[cfg(debug_assertions)]
    #[arg(long = "test", value_enum, value_name = "test_flag")]
    test_flag: Option<TestFlag>,
}

#[cfg(debug_assertions)]
#[derive(Debug, Copy, Clone, ValueEnum)]
enum TestFlag {
    GenerateReferenceImages,
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

    let arguments = CliArguments::parse();
    if arguments.debug_mode {
        env_logger::builder().filter_level(log::LevelFilter::Debug).init();
    } else {
        env_logger::builder().filter_level(log::LevelFilter::Info).init();
    }

    log::debug!("The program has been initialized with the parameters: {:?}", arguments);

    let mut font_styles_map: HashMap<String, FontStyles> = HashMap::new();
    layouting::load_fonts(&mut font_styles_map)?;
    log::debug!("Only the languages {:?} are supported", font_styles_map.keys().collect_vec());

    if arguments.configuration_file_path.is_none() {
        return Err(CustomError::with_context(
            "The configuration file path is missing, you need to provide one via the `config` flag"
                .into(),
        ));
    }
    #[allow(clippy::unwrap_used)]
    let configuration_file_path = arguments.configuration_file_path.unwrap();
    let configuration_file_contents =
        std::fs::read_to_string(configuration_file_path).map_err(|error| {
            CustomError::with_source("Failed to read the configuration file".into(), error.into())
        })?;
    let configuration: Configuration =
        serde_json::from_str(&configuration_file_contents).map_err(|error| {
            CustomError::with_source("Failed to parse the configuration file".into(), error.into())
        })?;
    log::debug!("The loaded configuration is: {:?}", configuration);

    let event_loop = EventLoop::new();
    let mut graphic_system = GraphicSystem::new(&event_loop, configuration)?;
    log::debug!("The graphic system has been successfully initialized");

    #[cfg(debug_assertions)]
    if let Some(test_flag) = arguments.test_flag {
        log::debug!(
            "The program has been initialized in test-mode with the test flag: {:?}",
            test_flag
        );
        if arguments.document_path.is_some() {
            log::warn!(
                "The document path provided via the `document` flag is ignored in test-mode"
            );
        }
        graphic_system.run_tests(test_flag, font_styles_map)?;
        return Ok(());
    }

    let (document, document_path) = document_format::load_document(arguments.document_path)?;
    log::debug!("The loaded document is: {:?}", document);
    graphic_system.set_window_title(document_path);

    event_loop.run(move |event, _, control_flow| {
        control_flow.set_wait();

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::KeyboardInput {
                    input: KeyboardInput { virtual_keycode: Some(VirtualKeyCode::Escape), .. },
                    ..
                }
                | WindowEvent::CloseRequested => {
                    control_flow.set_exit();
                    log::debug!("The program has been requested to be closed");
                }
                _ => (),
            },
            Event::RedrawRequested(_) => {
                if let Err(error) = graphic_system.draw_glyphs(&document, &font_styles_map) {
                    log::error!("{}", error);
                    control_flow.set_exit_with_code(1);
                }
            }
            _ => (),
        }
    });
}
