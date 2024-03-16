// These settings make it so that certain linter highlights which show
// non-idiomatic patterns get disable as they are made on purpose in order to
// accommodate further patterns yet to be implemented.
#![allow(clippy::collapsible_match, clippy::single_match, clippy::collapsible_if)]
// Forbid at the level of the linter the use of unwraps, which panic the program
// and forbid graceful termination
#![deny(clippy::unwrap_used)]

use crate::{
    graphics::{Texture, Vertex},
    text::{CharacterGeometry, Margins},
};
use clap::Parser;
use itertools::Itertools;
use nalgebra_glm::IVec2;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::File,
    io::Write as _,
    time::{Duration, Instant},
};
use std::{io::Read as _, path::PathBuf};
use unicode_normalization::UnicodeNormalization;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder},
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::WindowBuilder,
};
mod graphics;
mod text;

// The following structs have only been employed in this file, thus they they do
// not need to have a separate module associated to them. They represent data
// which is exchanged between different threads or either settings which are
// employed when generating the window.

/// The window modes: either fullscreen or windowed.
#[derive(Clone, Copy)]
pub enum WindowMode {
    Windowed(u32, u32),
    Fullscreen,
}

/// The events which can be sent from the main loop to the logic events thread.
enum RenderThreadEvent {
    SaveFile,
    RegisterCharacter(char),
}

/// The data which is sent through the texture bind group requests. It contains
/// the width, rows and buffer data needed to render the bitmap.
struct BitmapData {
    width: u32,
    rows: u32,
    buffer: Vec<u8>,
    pitch: u32,
}

/// The events which can be sent from the logic events thread to the main loop.
enum LogicThreadEvent {
    /// The request which is made once the vertex buffers need to be loaded.
    UpdateVertexBuffers {
        vertex_data: Vec<[Vertex; 6]>,
        text_characters: Vec<char>,
    },
    /// The request which is made once a character bitmap needs to be loaded.
    LoadTextureBindGroup {
        character_to_load: char,
        bitmap_data: BitmapData,
    },
    RequestRendering,
}

#[derive(Parser)]
#[command(version, about)]
/// The command line interface arguments which are automatically parsed by the
/// `clap` library.
struct CliArguments {
    /// The path of the document which needs to be loaded in order to be
    /// rendered as text.
    #[arg(long)]
    document: PathBuf,
    /// The path of the font file which is used in rendering the text.
    #[arg(long)]
    font: PathBuf,
}

/// The most common character is found in the english language. They are used to
/// calculate the average character advance.
const COMMON_CHARACTERS: &str =
    r#" abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890'`~<,.>/?"\|;:]}[{=+"#;

/// The event which is triggered when a character is not found in the font.
#[derive(Debug)]
struct SkippedCharacter(char);

impl std::fmt::Display for SkippedCharacter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Skipping the character: {}", self.0)
    }
}

impl std::error::Error for SkippedCharacter {}

/// Load the character from the font face and request the loading of its
/// texture.
fn load_character(
    font_face: &freetype::Face,
    character_to_load: char,
    characters_map: &mut HashMap<char, CharacterGeometry>,
    logic_events_sender: &crossbeam_channel::Sender<LogicThreadEvent>,
) -> Result<(), anyhow::Error> {
    // Load the selected character
    font_face.load_char(character_to_load as usize, freetype::face::LoadFlag::RENDER)?;
    let glyph = font_face.glyph();
    glyph.render_glyph(freetype::RenderMode::Mono)?;
    let glyph_bitmap = glyph.bitmap();

    // Associate to each character the geometry
    let character = CharacterGeometry {
        size: IVec2::new(glyph_bitmap.width(), glyph_bitmap.rows()),
        bearing: IVec2::new(glyph.bitmap_left(), glyph.bitmap_top()),
        advance: glyph.advance().x as u32,
    };
    characters_map.insert(character_to_load, character);

    if glyph_bitmap.width() == 0 || glyph_bitmap.rows() == 0 {
        return Err(anyhow::Error::new(SkippedCharacter(character_to_load)));
    }

    // Send to the render thread the data needed to load the texture of the glyph
    logic_events_sender.send(LogicThreadEvent::LoadTextureBindGroup {
        character_to_load,
        bitmap_data: BitmapData {
            width: glyph_bitmap.width() as u32,
            rows: glyph_bitmap.rows() as u32,
            buffer: glyph_bitmap.buffer().to_vec(),
            pitch: glyph_bitmap.pitch() as u32,
        },
    })?;

    Ok(())
}

/// By iterating on a wide range of possible characters we are able to estimate
/// the average line length in order to render properly on to the window.
fn estimate_average_line_length(
    font_face: &freetype::Face,
    window_width: u32,
    margins: Margins,
) -> Result<u32, anyhow::Error> {
    let mut character_advances = Vec::new();

    for character in COMMON_CHARACTERS.chars().nfc() {
        // Each character is loaded according to its character code which is obtained
        // from the `nfc` function
        font_face.load_char(character as usize, freetype::face::LoadFlag::RENDER)?;
        let glyph = font_face.glyph();

        character_advances.push((glyph.advance().x as u32) >> 6); // Bitshift by
        // 6 to convert
        // in pixels
    }
    let average_character_advance =
        (character_advances.iter().sum::<u32>() as f32 / character_advances.len() as f32) as u32;

    let average_line_length =
        ((window_width as f32 - margins.left - margins.right) / average_character_advance as f32) as u32;
    log::debug!("Average line length in characters: {}", average_line_length);

    Ok(average_line_length)
}

/// Calculate the positions of the vertices for the given glyph.
#[inline(always)]
fn calculate_glyph_vertices(x: f32, y: f32, width: f32, height: f32) -> [Vertex; 6] {
    [
        // Upper triangle
        Vertex { position: [x, y + height], texture_coordinates: [0.0, 0.0] },
        Vertex { position: [x, y], texture_coordinates: [0.0, 1.0] },
        Vertex { position: [x + width, y], texture_coordinates: [1.0, 1.0] },
        // Lower triangle
        Vertex { position: [x, y + height], texture_coordinates: [0.0, 0.0] },
        Vertex { position: [x + width, y], texture_coordinates: [1.0, 1.0] },
        Vertex { position: [x + width, y + height], texture_coordinates: [1.0, 0.0] },
    ]
}

#[derive(Serialize, Deserialize, Debug)]
struct WindowSize {
    width: u32,
    height: u32,
}

#[derive(Serialize, Deserialize, Debug)]
struct Configuration {
    #[serde(rename = "windowSize")]
    window_size: WindowSize,
    #[serde(rename = "fontSize")]
    font_size: u32,
    margins: Margins,
}

fn fallible_main() -> Result<(), anyhow::Error> {
    // Initialize the logging processes and read the environment variable `RUST_LOG`
    // in order to set the level for filtering the events
    env_logger::init();

    // Parse the command line arguments which were specified
    let CliArguments { document: document_path, font: font_path } = CliArguments::parse();
    log::info!("The document {:?} will be loaded with the font {:?}", document_path.as_path(), font_path.as_path());

    // Open the configuration file
    let mut configuration_file = File::open("config.json")?;
    // Read the file contents into a string
    let mut contents = String::new();
    configuration_file.read_to_string(&mut contents)?;

    // Parse the JSON in order to extract the configuration parameters
    let configuration: Configuration = serde_json::from_str(&contents)?;
    let WindowSize { width: window_width, height: window_height } = configuration.window_size;
    let font_size = configuration.font_size;
    let margins = configuration.margins;

    log::info!("Loading the text with the margins {:?} and font size {}.", margins, font_size);

    // Create the event loop which can accept custom events generated by the user
    let event_loop: EventLoop<_> = EventLoopBuilder::<RenderThreadEvent>::with_user_event().build()?;

    // Create the window in the windowed setting and set the custom resolution
    let window_mode = WindowMode::Windowed(window_width, window_height);

    let mut builder = WindowBuilder::new().with_resizable(false).with_title("TeXtr");

    match window_mode {
        WindowMode::Windowed(width, height) => {
            // Construct the window at the center of the screen
            let monitor = match event_loop.primary_monitor() {
                Some(monitor) => monitor,
                None => {
                    return Err(anyhow::Error::msg("The primary monitor is not available"));
                }
            };
            let size = monitor.size();
            let position = PhysicalPosition::new((size.width - width) as i32 / 2, (size.height - height) as i32 / 2);
            builder = builder.with_inner_size(PhysicalSize::new(width, height)).with_position(position);
        }
        WindowMode::Fullscreen => {
            // If the setting is fullscreen, then configure it to borderless
            builder = builder.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }
    };

    let window = builder.build(&event_loop)?;

    // Initialize the render state as pre-configured
    let mut render_state = pollster::block_on(graphics::RenderState::new(window))?;

    // Create the channel for sending the events from the logic thread to the main
    // loop
    let (logic_events_sender, logic_events_receiver) = crossbeam_channel::unbounded();
    // Create the channel for sending the events from the main loop to the logic
    // thread
    let (render_events_sender, render_events_receiver) = crossbeam_channel::unbounded();

    // Bootstrap the thread for handling the logic events, which are separate from
    // the rendering, which happens in the main loop
    let _logic_events_thread: std::thread::JoinHandle<()> = std::thread::spawn(move || {
        let logic_events_thread_outcome = move || -> Result<(), anyhow::Error> {
            // Load the library Freetype for using the font glyphs as it is using FFI
            let library: freetype::Library = freetype::Library::init()?;

            // Load the text from the given document path
            let mut text = std::fs::read_to_string(document_path.as_path())?;
            log::debug!("Imported the text: {:?}", text);
            // Load the font from the font path
            let font_face = library.new_face(font_path, 0)?;

            // Configure the pixel size of the font (in arbitrary units of measurement)
            font_face.set_pixel_sizes(0, font_size)?;

            // Calculate the line length based on the average character advance and the size
            // of the window, respecting the margins
            let mut line_height = window_height as f32 - font_size as f32 - margins.top;

            // Calculate the advance of the space character separately as it is handled
            // differently because it doesn't have a texture
            font_face.load_char(' ' as usize, freetype::face::LoadFlag::RENDER)?;
            let glyph = font_face.glyph();
            let space_character_advance = (glyph.advance().x as u32) >> 6;

            // Estimate the average line length based on the window width and the margins
            let average_line_length = estimate_average_line_length(&font_face, window_width, margins)?;

            // Load the characters in the text from the chosen font...
            let mut characters_map: HashMap<char, CharacterGeometry> = HashMap::new();

            // ...but skip the space character
            for character_to_load in text.nfc().unique().filter(|character| *character != ' ') {
                if load_character(&font_face, character_to_load, &mut characters_map, &logic_events_sender).is_err() {
                    log::warn!("Skipped the initial loading of the character {:?}", character_to_load);
                    continue;
                }
            }

            log::debug!("Characters requested to be loaded: {}", characters_map.len());

            // Create the input buffer which is needed for keeping track of the characters
            // pressed
            let mut input_buffer = Vec::new();

            // Assign a value to cap the ticks-per-second in the logic events loop
            let tps_cap: Option<u32> = Some(30);
            let desired_frame_time = tps_cap.map(|tps| Duration::from_secs_f64(1.0 / tps as f64));
            let mut frame_count = 0;
            let mut is_first_frame = true;

            // Start the logic events loop
            loop {
                // Register the start time for the current frame
                let frame_start_time = Instant::now();
                log::trace!("Logic events loop frame number {frame_count} start time: {:?}", frame_start_time);

                // Fetch the custom events sent by the render thread to the logic events thread
                while let Ok(render_events) = render_events_receiver.try_recv() {
                    match render_events {
                        RenderThreadEvent::SaveFile => {
                            // Save the text to the file path given, but if it fails, the file
                            // is overwritten and all content is lost. It can fail if the path is
                            // invalid.
                            let mut file = File::create(document_path.as_path())?;
                            file.write_all(text.clone().as_bytes())?;
                            log::info!("The document has been successfully saved to the path: {:?}", document_path);
                        }
                        RenderThreadEvent::RegisterCharacter(character) => {
                            input_buffer.push(character);
                        }
                    }
                }

                // Insert the text in the input buffer at the caret position and load each newly
                // present character in the input.
                for input_character in input_buffer.iter().copied() {
                    text.push(input_character);

                    // Convert the character to unicode normalized form C (NFC)...
                    let character_to_load = match input_character.nfc().next() {
                        Some(character) => character,
                        None => {
                            return Err(anyhow::Error::msg(format!(
                                "Unable to normalize the character {:?} to unicode normalized form C",
                                input_character
                            )));
                        }
                    };
                    // ...and then see if the character is already present in the map, if it isn't
                    // then attempt to load it from the specified font
                    if !characters_map.contains_key(&character_to_load) {
                        if load_character(&font_face, character_to_load, &mut characters_map, &logic_events_sender)
                            .is_err()
                        {
                            log::warn!("Skipped the dynamic loading of the character {:?}", character_to_load);
                        }
                    }
                }

                // If the input buffer is not empty or it is the first frame, then we need to
                // calculate the vertices for the text positioning in the window
                if !input_buffer.is_empty() || is_first_frame {
                    // Each iteration of the loop, wrap the text in lines...
                    let wrapped_text: Vec<_> = textwrap::wrap(&text, average_line_length as usize)
                        .iter()
                        .map(|line| line.to_string())
                        .collect();

                    // Calculate the positions at which the vertices need to be in order to render
                    // the text correctly
                    let mut raw_vertices_data = Vec::new();
                    let mut text_characters = Vec::new();

                    // For each line in the wrapped text...
                    for line in wrapped_text.iter() {
                        let mut horizontal_origin = margins.left;

                        // ...draw each character therein present...
                        for character in line.chars() {
                            // ...and skip the space character
                            if character == ' ' {
                                horizontal_origin += space_character_advance as f32;

                                continue;
                            }
                            let character_data = match characters_map.get(&character) {
                                Some(character) => character,
                                None => {
                                    return Err(anyhow::Error::msg(format!(
                                        "Unable to retrieve the character {:?} from the map, it is not (at least yet) loaded",
                                        character
                                    )));
                                }
                            };

                            let x = horizontal_origin + character_data.bearing.x as f32;
                            let y = line_height - (character_data.size.y - character_data.bearing.y) as f32;

                            let width = character_data.size.x as f32;
                            let height = character_data.size.y as f32;

                            // The positions of the two triangles making up a single text glyph
                            let raw_vertex_data = calculate_glyph_vertices(x, y, width, height);

                            raw_vertices_data.push(raw_vertex_data);
                            text_characters.push(character);

                            // Move the origin by the character advance in order to draw the
                            // characters side-to-side.
                            horizontal_origin += (character_data.advance >> 6) as f32;
                            // Bitshift by 6 to get value in pixels (2^6 = 64)
                        }

                        // Move the line height below by the font size when each line is finished
                        line_height -= font_size as f32;
                    }

                    logic_events_sender.send(LogicThreadEvent::UpdateVertexBuffers {
                        vertex_data: raw_vertices_data,
                        text_characters,
                    })?;

                    // Clear the input buffer and request a rendering
                    input_buffer.clear();
                    logic_events_sender.send(LogicThreadEvent::RequestRendering)?;
                }

                // In the end, reset the line height to its original value
                line_height = window_height as f32 - margins.top - font_size as f32;

                // Cap the ticks-per-second
                if let Some(desired_frame_time) = desired_frame_time {
                    let elapsed_time = frame_start_time.elapsed();
                    if elapsed_time < desired_frame_time {
                        std::thread::sleep(desired_frame_time - elapsed_time);
                    }
                }

                frame_count += 1;
                is_first_frame = false;
            }

            #[allow(unreachable_code)]
            Ok(())
        }();

        match logic_events_thread_outcome {
            Ok(_) => (),
            Err(error) => {
                log::error!("The logic events thread has just stopped: {:?}", error);
            }
        }
    });

    // Run the event loop in the main thread
    event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::Wait);
        // Each iteration of the loop fetch the logic thread events
        while let Ok(logic_events) = logic_events_receiver.try_recv() {
            match logic_events {
                // It can either be  update the vertex buffers...
                LogicThreadEvent::UpdateVertexBuffers { vertex_data, text_characters } => {
                    render_state.update_vertex_buffer(vertex_data, text_characters)
                }
                // ...or to load the new textures
                LogicThreadEvent::LoadTextureBindGroup { character_to_load, bitmap_data } => {
                    // Use the received bitmap data to generate the glyph texture
                    let texture = match Texture::from_bitmap_data(
                        &render_state,
                        bitmap_data,
                        Some(format!("Glyph Texture {:?}", character_to_load).as_str()),
                    ) {
                        Ok(texture) => texture,
                        Err(error) => {
                            log::error!(
                                "Unable to create the texture for the character {:?}: {:?}",
                                character_to_load,
                                error
                            );
                            return;
                        }
                    };

                    // Load the texture bind group for the glyph
                    render_state.load_texture(texture, character_to_load);
                    log::debug!("Loaded the texture bind group for the glyph {:?}", character_to_load);
                }
                LogicThreadEvent::RequestRendering => {
                    // RedrawRequested will only trigger once, unless we manually request it.
                    render_state.window().request_redraw();
                }
            }
        }

        // Check if any modifiers have been pressed before parsing any other events
        let mut ctrl_is_pressed = false;
        match event {
            Event::WindowEvent { ref event, window_id } if window_id == render_state.window().id() => match event {
                WindowEvent::ModifiersChanged(modifiers) => match modifiers.state() {
                    ModifiersState::CONTROL => {
                        ctrl_is_pressed = true;
                    }
                    _ => (),
                },
                _ => (),
            },
            _ => (),
        };

        match event {
            Event::WindowEvent { ref event, window_id } if window_id == render_state.window().id() => {
                match event {
                    // If the user presses Ctrl + S key, save the document
                    WindowEvent::KeyboardInput {
                        event:
                            KeyEvent {
                                state: ElementState::Pressed, physical_key: PhysicalKey::Code(KeyCode::Escape), ..
                            },
                        ..
                    } => {
                        if ctrl_is_pressed {
                            match render_events_sender.send(RenderThreadEvent::SaveFile) {
                                Ok(_) => {
                                    log::debug!("Ctrl + S pressed, the document has been requested to be saved");
                                }
                                Err(error) => {
                                    log::error!("Unable to send the event to the render thread: {:?}", error);
                                    return;
                                }
                            };
                        }
                    }
                    _ => (),
                }
            }
            _ => (),
        }

        match event {
            Event::WindowEvent { ref event, window_id } if window_id == render_state.window().id() => {
                match event {
                    // Check if the window has been requested to be closed
                    WindowEvent::CloseRequested
                    | WindowEvent::KeyboardInput {
                        event:
                            KeyEvent {
                                state: ElementState::Pressed, physical_key: PhysicalKey::Code(KeyCode::Escape), ..
                            },
                        ..
                    } => target.exit(),
                    WindowEvent::Resized(physical_size) => {
                        render_state.resize(*physical_size);
                    }

                    WindowEvent::KeyboardInput {
                        event: KeyEvent { text: Some(text_input), state: ElementState::Pressed, .. },
                        ..
                    } => {
                        for character in text_input.chars() {
                            match render_events_sender.send(RenderThreadEvent::RegisterCharacter(character)) {
                                Ok(_) => {
                                    log::debug!("The character {:?} has been registered", character);
                                }
                                Err(error) => {
                                    log::error!("Unable to send the event to the render thread: {:?}", error);
                                    return;
                                }
                            };
                        }
                    }

                    WindowEvent::RedrawRequested => {
                        match render_state.render() {
                            std::result::Result::Ok(_) => {}
                            // Reconfigure the surface if lost
                            Err(wgpu::SurfaceError::Lost) => render_state.resize(render_state.physical_size),
                            // The system is out of memory, we should probably quit
                            Err(wgpu::SurfaceError::OutOfMemory) => target.exit(),
                            // All other errors (Outdated, Timeout) should be resolved by the next frame
                            Err(error) => log::error!("{:?}", error),
                        }
                    }
                    _ => (),
                }
            }
            _ => (),
        }
    })?;

    Ok(())
}

fn main() {
    match fallible_main() {
        Ok(_) => {}
        Err(error) => {
            log::error!("The program has unexpectedly been terminated: {}", error);
        }
    }
}
