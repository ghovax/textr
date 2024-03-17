// These settings make it so that certain things that the linter highlights
// which are typically non-idiomatic patterns get disabled; these kind of
// patterns are used purposely in order to accommodate further pattern extension
#![allow(clippy::collapsible_match, clippy::single_match, clippy::collapsible_if)]
//
// Forbid at the level of the linter the use of unwraps, which panic the program
// and forbid graceful termination; in this way we are sure that the errors are properly handled
#![deny(clippy::unwrap_used)]

use crate::{
    graphics::{Texture, Vertex},
    text::{CharacterGeometry, Margins},
};
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

// The following structs have only been employed in this file, thus they do
// not need to have a separate module associated, they represent data
// which is exchanged between different threads or either settings which are
// employed when generating the window

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
/// the width, number of rows, pitch and buffer data needed to render the bitmap.
struct BitmapData {
    width: u32,
    rows: u32,
    buffer: Vec<u8>,
    pitch: u32,
}

/// The events which can be sent from the logic events thread to the main loop.
enum LogicThreadEvent {
    /// The request which is made once the vertex buffer needs to be loaded.
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

/// The most common characters found in the English language. They are used in
/// order to calculate the average character advance.
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

/// Load the character from the given font face and request the loading of its
/// texture into the character texture cache.
fn load_character(
    font_face: &freetype::Face,
    character_to_load: char,
    characters_map: &mut HashMap<char, CharacterGeometry>,
    logic_events_sender: &crossbeam_channel::Sender<LogicThreadEvent>,
) -> Result<(), anyhow::Error> {
    // Load the selected character from the font and retrieve the glyph
    font_face.load_char(character_to_load as usize, freetype::face::LoadFlag::RENDER)?;
    let glyph = font_face.glyph();
    glyph.render_glyph(freetype::RenderMode::Mono)?;
    let glyph_bitmap = glyph.bitmap();

    // Associate to character to load the geometry of the associated glyph
    let character = CharacterGeometry {
        size: IVec2::new(glyph_bitmap.width(), glyph_bitmap.rows()),
        bearing: IVec2::new(glyph.bitmap_left(), glyph.bitmap_top()),
        advance: glyph.advance().x as u32,
    };
    characters_map.insert(character_to_load, character);

    if glyph_bitmap.width() == 0 || glyph_bitmap.rows() == 0 {
        return Err(anyhow::Error::new(SkippedCharacter(character_to_load)));
    }

    // Send to the render thread the data needed in order to load the glyph bitmap into a texture
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

/// Estimate the average line length by taking into consideration both the
/// window width and the specified margins.
fn estimate_average_line_length(
    font_face: &freetype::Face,
    window_width: u32,
    margins: Margins,
) -> Result<u32, anyhow::Error> {
    let mut character_advances = Vec::new();

    // For each common character...
    for character in COMMON_CHARACTERS.chars().nfc() {
        // ...retrieve the associated glyph from the font...
        font_face.load_char(character as usize, freetype::face::LoadFlag::RENDER)?;
        let glyph = font_face.glyph();

        // ...then save its horizontal advance in pixels by bitshifting it by 6
        character_advances.push((glyph.advance().x as u32) >> 6);
    }

    // Compute the average character advance as the arithmetic average of all the character advances
    let average_character_advance =
        (character_advances.iter().sum::<u32>() as f32 / character_advances.len() as f32) as u32;

    // Estimate the average line length as the total number of characters which
    // fit in the available horizontal space, if the average character
    // advance is the one previously calculated
    let average_line_length = ((window_width as f32 - margins.left - margins.right)
        / average_character_advance as f32) as u32;
    log::debug!("Average line length as measured in characters: {}", average_line_length);

    Ok(average_line_length)
}

/// Calculate the positions of the vertices for the two triangles making up the
/// glyph by knowing its geometry.
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

/// The configuration setting for the window size which is loaded from the `config.json` file.
#[derive(Serialize, Deserialize, Debug)]
struct WindowSize {
    width: u32,
    height: u32,
}

/// All the parameters needed for configuring the rendering which are loaded
/// from the `config.json` file. The fields are renamed in order to stabilize
/// the interface in case the field names have been changed during development.
#[derive(Serialize, Deserialize, Debug)]
struct Configuration {
    /// The path of the document which is loaded in the editor.
    #[serde(rename = "documentPath")]
    document_path: PathBuf,
    /// The path of the font file which is used in rendering the text.
    #[serde(rename = "fontPath")]
    font_path: PathBuf,
    /// The size of the font in arbitrary units.
    #[serde(rename = "fontSize")]
    font_size: u32,
    /// The margins which the text is subjected to.
    #[serde(rename = "margins")]
    margins: Margins,
    /// The option for setting fullscreen or windowed
    #[serde(rename = "fullscreen")]
    fullscreen: bool,
    /// The window size.
    #[serde(rename = "windowSize")]
    window_size: WindowSize,
}

/// This is the alternative main function which can return an error and
/// propagate it through the stack in order to handle it gracefully.
fn fallible_main() -> Result<(), anyhow::Error> {
    // Initialize the logging processes and read the environment variable
    // `RUST_LOG` in order to set the level for filtering the events
    env_logger::init();

    // Open the configuration file
    let mut configuration_file = File::open("config.json")?;
    // Read the configuration file contents into a string
    let mut contents = String::new();
    configuration_file.read_to_string(&mut contents)?;

    // Parse the JSON in order to extract the configuration parameters
    let configuration: Configuration = serde_json::from_str(&contents)?;
    log::info!("Configuration file `config.json` loaded successfully");

    // Extract the path of the document which is loaded in the editor
    let document_path = configuration.document_path;
    let font_path = configuration.font_path;
    log::info!(
        "The document {:?} will be loaded with the font {:?}",
        document_path.as_path(),
        font_path.as_path()
    );
    let WindowSize { width: window_width, height: window_height } = configuration.window_size;
    let font_size = configuration.font_size;
    let margins = configuration.margins;
    log::info!("Loading the text with the margins {:?} and font size {}.", margins, font_size);

    // Create the event loop which can accept custom events generated by the user
    let event_loop: EventLoop<_> =
        EventLoopBuilder::<RenderThreadEvent>::with_user_event().build()?;

    let mut builder = WindowBuilder::new().with_resizable(false).with_title("TeXtr");

    // If the setting is fullscreen, then configure it to borderless...
    if configuration.fullscreen {
        builder = builder.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
    } else {
        // ...otherwise, construct the window at the center of the screen of the
        // primary monitor with the width and height taken from the configuration file
        let monitor = match event_loop.primary_monitor() {
            Some(monitor) => monitor,
            None => {
                return Err(anyhow::Error::msg("The primary monitor is not available"));
            }
        };
        let monitor_size = monitor.size();
        let position = PhysicalPosition::new(
            (monitor_size.width - window_width) as i32 / 2,
            (monitor_size.height - window_height) as i32 / 2,
        );
        builder = builder
            .with_inner_size(PhysicalSize::new(window_width, window_height))
            .with_position(position);
    }

    let window = builder.build(&event_loop)?;

    // Initialize the render state as pre-configured in the `RenderState::new` function
    let mut render_state = pollster::block_on(graphics::RenderState::new(window))?;

    // Create the channel for sending the events from the logic thread to the main loop
    let (logic_events_sender, logic_events_receiver) = crossbeam_channel::unbounded();
    // Create the channel for sending the events from the main loop to the logic thread
    let (render_events_sender, render_events_receiver) = crossbeam_channel::unbounded();

    // Bootstrap the thread for handling the logic events, which are separate
    // from the rendering which happens in the render thread
    let _logic_events_thread: std::thread::JoinHandle<()> = std::thread::spawn(move || {
        // Collect the outcome of the logic events thread as it's might return an error
        let logic_events_thread_outcome = move || -> Result<(), anyhow::Error> {
            // Initialize the library Freetype in order to use the font glyphs; it is using
            // the FFI (foreign function interface) to call the underlying library
            let library = freetype::Library::init()?;
            // Load the font from the font path
            let font_face = library.new_face(font_path, 0)?;
            // Configure the pixel size of the font in units of pixel
            font_face.set_pixel_sizes(0, font_size)?;

            // Load the text contained in the file at the document path
            let mut text = std::fs::read_to_string(document_path.as_path())?;
            log::debug!("Imported the text: {:?}", text);

            // Set up the starting line height based on the font size, window height and top-most margin
            let mut line_height = window_height as f32 - font_size as f32 - margins.top;

            // Estimate the average line length based on the window width and the margins
            let average_line_length =
                estimate_average_line_length(&font_face, window_width, margins)?;

            let mut characters_map: HashMap<char, CharacterGeometry> = HashMap::new();

            // Load the glyphs associated to the characters in the text from the chosen font
            for character_to_load in text.nfc().unique().filter(|character| *character != ' ') {
                if load_character(
                    &font_face,
                    character_to_load,
                    &mut characters_map,
                    &logic_events_sender,
                )
                .is_err()
                {
                    log::warn!(
                        "Skipped the initial loading of the character {:?}",
                        character_to_load
                    );
                    continue;
                }
            }
            log::debug!("Characters requested to be loaded: {}", characters_map.len());

            // Calculate the advance of the space character separately as it needs to be handled differently
            // from the other characters because it doesn't have a texture associated to it
            font_face.load_char(' ' as usize, freetype::face::LoadFlag::RENDER)?;
            let glyph = font_face.glyph();
            let space_character_advance = (glyph.advance().x as u32) >> 6;

            // Initialize the input buffer, which is used to keep track of the characters
            // pressed on the keyboard by the user
            let mut input_buffer = Vec::new();

            // Assign a value to cap the ticks-per-second in the logic events loop
            let tps_cap: Option<u32> = Some(30);
            let desired_iteration_duration =
                tps_cap.map(|tps| Duration::from_secs_f64(1.0 / tps as f64));
            let mut iteration_count = 0;
            let mut is_first_iteration = true;

            // Each iteration of the logic events loop...
            loop {
                // ...register the start time for the current iteration as a checkpoint
                let iteration_start_time = Instant::now();
                log::trace!(
                    "Logic events loop iteration number {iteration_count} start time: {:?}",
                    iteration_start_time
                );

                // ...fetch the custom events sent by the render thread to the logic events
                // thread, such as the request to save the file or to register a pressed character...
                while let Ok(render_events) = render_events_receiver.try_recv() {
                    match render_events {
                        RenderThreadEvent::SaveFile => {
                            // Save the text to the file path given
                            let mut file = File::create(document_path.as_path())?;
                            file.write_all(text.clone().as_bytes())?;
                            log::info!(
                                "The document has been successfully saved to the path: {:?}",
                                document_path
                            );
                        }
                        RenderThreadEvent::RegisterCharacter(character) => {
                            input_buffer.push(character);
                        }
                    }
                }

                // ...insert the characters present in the input buffer at the end of the text
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
                        if load_character(
                            &font_face,
                            character_to_load,
                            &mut characters_map,
                            &logic_events_sender,
                        )
                        .is_err()
                        {
                            log::warn!(
                                "Skipped the dynamic loading of the character {:?}",
                                character_to_load
                            );
                        }
                    }
                }

                // ...check if any character is have been inserted by the user and if this
                // happened, then begin calculating the new text layout
                if !input_buffer.is_empty() || is_first_iteration {
                    // Each iteration of the loop, wrap the text in lines...
                    let wrapped_text: Vec<_> = textwrap::wrap(&text, average_line_length as usize)
                        .iter()
                        .map(|line| line.to_string())
                        .collect();

                    // ...and then calculate the positions at which the vertices need to be in order
                    // to render the text correctly onto the window by respecting the margin constraints
                    let mut raw_vertices_data = Vec::new();
                    let mut text_characters = Vec::new();

                    // For each line in the wrapped text...
                    for line in wrapped_text.iter() {
                        let mut horizontal_origin = margins.left;

                        // ...draw each character therein present...
                        for character in line.chars() {
                            // ...and skip the space character...
                            if character == ' ' {
                                horizontal_origin += space_character_advance as f32;
                                continue;
                            }
                            // ...otherwise, retrieve the character geometry from the map...
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
                            let y = line_height
                                - (character_data.size.y - character_data.bearing.y) as f32;

                            let width = character_data.size.x as f32;
                            let height = character_data.size.y as f32;

                            // ...calculate the positions of the two triangles making up a single
                            // text glyph by knowing the geometry of the glyph...
                            let raw_vertex_data = calculate_glyph_vertices(x, y, width, height);

                            raw_vertices_data.push(raw_vertex_data);
                            text_characters.push(character);

                            // ...and in the end, move the horizontal origin of the text by the
                            // character advance in order to draw the characters side-to-side...
                            horizontal_origin += (character_data.advance >> 6) as f32;
                        }

                        // ...conclude by moving the line height below by the font size
                        line_height -= font_size as f32;
                    }

                    // ... after the calculations are done, send to the render thread the newly
                    // calculated vertices and the characters present in the text, discarding the spaces...
                    logic_events_sender.send(LogicThreadEvent::UpdateVertexBuffers {
                        vertex_data: raw_vertices_data,
                        text_characters,
                    })?;

                    // ...and in the end, clear the input buffer and request a rendering
                    input_buffer.clear();
                    logic_events_sender.send(LogicThreadEvent::RequestRendering)?;
                }

                // ...before starting the next iteration of the loop, reset the line height to
                // its original value, as computed in the beginning of the loop
                line_height = window_height as f32 - margins.top - font_size as f32;

                // Cap the ticks-per-second in order to not cause a performance hog
                if let Some(desired_iteration_duration) = desired_iteration_duration {
                    let elapsed_time = iteration_start_time.elapsed();
                    if elapsed_time < desired_iteration_duration {
                        // Do not busy wait, but instead request the CPU to idle
                        std::thread::sleep(desired_iteration_duration - elapsed_time);
                    }
                }

                iteration_count += 1;
                is_first_iteration = false;
            }

            #[allow(unreachable_code)]
            Ok(())
        }();

        // Gracefully handle the termination of the loop due to an error
        match logic_events_thread_outcome {
            Ok(_) => (),
            Err(error) => {
                log::error!("The logic events thread has just stopped: {:?}", error);
            }
        }
    });

    // Run the render events loop in the main thread; each iteration of the loop...
    event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::Wait);
        // ...fetch the logic thread events, which can either be a request...
        while let Ok(logic_events) = logic_events_receiver.try_recv() {
            match logic_events {
                // ...to update the vertex buffers
                LogicThreadEvent::UpdateVertexBuffers { vertex_data, text_characters } => {
                    render_state.update_vertex_buffer(vertex_data, text_characters)
                }
                // ...or to load the new textures
                LogicThreadEvent::LoadTextureBindGroup { character_to_load, bitmap_data } => {
                    // Use the received bitmap data to generate the glyph texture
                    let texture = Texture::from_bitmap_data(
                        &render_state,
                        bitmap_data,
                        Some(format!("Glyph Texture {:?}", character_to_load).as_str()),
                    );

                    // Load the texture bind group for the glyph
                    render_state.load_texture(texture, character_to_load);
                    log::debug!("Loaded the texture bind group for the glyph {:?}", character_to_load);
                }
                // ...or to request a redraw of the window frame
                LogicThreadEvent::RequestRendering => {
                    render_state.window().request_redraw();
                }
            }
        }

        // ...check if any modifiers have been pressed before parsing any other events
        let mut ctrl_is_pressed = false;
        match event {
            Event::WindowEvent { ref event, .. } => match event {
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

        // ...parse the possible combinations of keys with modifiers which could have
        // been pressed since the last iteration
        match event {
            Event::WindowEvent { ref event, .. } => {
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

        // ...and then parse all the other possible window events
        match event {
            Event::WindowEvent { ref event, .. } => {
                match event {
                    // Check if the window has been requested to be closed or the user has pressed escape key
                    WindowEvent::CloseRequested
                    | WindowEvent::KeyboardInput {
                        event:
                            KeyEvent {
                                state: ElementState::Pressed, physical_key: PhysicalKey::Code(KeyCode::Escape), ..
                            },
                        ..
                    } => target.exit(),
                    // Check if the window has been resized and update the render state accordingly
                    WindowEvent::Resized(physical_size) => {
                        render_state.resize(*physical_size);
                    }
                    // Check if the user has typed a character and if this happened, then send the pressed 
                    // character to the logic events loop for further processing
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
                    // RedrawRequested will only trigger once, unless we manually request it.
                    WindowEvent::RedrawRequested => {
                        // Redraw the current frame by employing the pre-configured
                        // `RenderState::render` function, this can either succeed or fail...
                        match render_state.render() {
                            // ...if it succeeds, then proceed and return from this function
                            std::result::Result::Ok(_) => (),
                            Err(error) => {
                                match error.downcast_ref::<wgpu::SurfaceError>() {
                                    Some(wgpu_error) => match wgpu_error {
                                        // ...otherwise, reconfigure the surface if it was lost
                                        wgpu::SurfaceError::Lost => render_state.resize(render_state.physical_size),
                                        // ...or if the system is out of memory, we should probably quit
                                        wgpu::SurfaceError::OutOfMemory => target.exit(),
                                        // ..all other errors (Outdated, Timeout) should typically be resolved by the
                                        // next frame, thus we just log it and continue
                                        wgpu_error => {
                                            log::error!("Unhandled error in the render function: {:?}", wgpu_error)
                                        }
                                    },
                                    None => {
                                        log::error!(
                                            "Unhandled error in the render function after downcasting: {:?}",
                                            error
                                        );
                                    }
                                }
                            }
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
    // Run the fallible main which contains all the code, and then gracefully
    // terminate if an error is returned
    match fallible_main() {
        Ok(_) => {}
        Err(error) => {
            log::error!("{}", error);
        }
    }
}
