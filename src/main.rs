use std::{collections::HashMap, fs::File, time::Instant};

use chrono::Local;
// use clap::Parser;
use glm::{IVec2, Vec2, Vec3};
use glow::*;
use itertools::Itertools;
use log::LevelFilter;
use nalgebra_glm as glm;
use regex::Regex;
use sdl2::{
    event::Event,
    keyboard::{Keycode, Mod},
};
use std::io::Write;
use unicode_normalization::UnicodeNormalization;

mod line;

use crate::line::Margins;

// TODOs
// [ ] 2. Improve performance by caching the textures or something similar
// [ ] 4. Fix the disappearance of the caret at the end of the line, as well as the crashes

fn main() {
    // Log file with the current time and date
    // let log_file_name = format!("logs/log_{}.txt", Local::now().format("%Y-%m-%d_%H-%M-%S"));
    // let log_file = Box::new(File::create(log_file_name).unwrap());
    env_logger::builder()
        // .target(env_logger::Target::Pipe(log_file))
        .filter_level(LevelFilter::Trace)
        .init();

    let document_path = "assets/textTest.txt";
    let font_path = "fonts/cmunrm.ttf";

    log::trace!(
        "The document '{}' will be loaded with the font '{}'",
        document_path,
        font_path
    );

    // Initialize the GLFW window

    let sdl = sdl2::init().unwrap();
    let event_subsystem = sdl.event().unwrap();
    let video = sdl.video().unwrap();

    let gl_attributes = video.gl_attr();
    gl_attributes.set_context_profile(sdl2::video::GLProfile::Core);
    gl_attributes.set_context_version(3, 3);
    gl_attributes.set_context_flags().forward_compatible().set();

    gl_attributes.set_accelerated_visual(true);
    gl_attributes.set_multisample_samples(4);

    let mut window = video
        .window(document_path, 800, 600)
        .opengl()
        .resizable()
        .allow_highdpi()
        .build()
        .unwrap();
    window.set_minimum_size(480, 320).unwrap();

    // Load OpenGL v3.3 from GLFW

    let _gl_context = window.gl_create_context().unwrap();
    let gl = unsafe {
        glow::Context::from_loader_function(|s| video.gl_get_proc_address(s) as *const _)
    };
    let mut event_loop = sdl.event_pump().unwrap();

    // Load the library Freetype for using the font glyphs

    let library: freetype::Library = freetype::Library::init().unwrap();

    // Load the text from the file path given
    let mut text = std::fs::read_to_string(document_path).unwrap();
    log::trace!("Imported the text: {:?}", text);
    let face = library.new_face(font_path, 0).unwrap();

    // Calculate the line length based on the average character advance

    let font_size = 50.0; // Arbitrary unit of measurement
    face.set_pixel_sizes(0, font_size as u32).unwrap(); // TODO: `pixel_width` is 0? Probably it means "take the default one"

    let margins = Margins {
        top: 60.0,
        bottom: 60.0,
        left: 30.0,
        right: 30.0,
    };

    let framebuffer_size = window.drawable_size();
    let (mut window_width, mut window_height) =
        (framebuffer_size.0 as f32, framebuffer_size.1 as f32);

    let mut line_height = window_height - font_size - margins.top;

    let mut character_advances = Vec::new();
    for normalized_utf8_character in
        r#"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890'`~<,.>/?"\|;:]}[{=+"#
            .chars()
            .nfc()
    {
        face.load_char(
            normalized_utf8_character as usize,
            freetype::face::LoadFlag::RENDER,
        )
        .unwrap();
        let glyph = face.glyph();

        character_advances.push((glyph.advance().x as u32) >> 6); // Bitshift by 6 to convert in pixels
    }
    let average_character_advance =
        (character_advances.iter().sum::<u32>() as f32 / character_advances.len() as f32) as u32;

    let mut average_line_length =
        ((window_width - margins.left - margins.right) / average_character_advance as f32) as u32;
    log::trace!("Average line length in characters: {}", average_line_length);

    // Set the culling, clear color and blending options

    unsafe {
        // gl.enable(CULL_FACE);
        gl.clear_color(1.0, 1.0, 1.0, 1.0);

        gl.enable(BLEND);
        gl.blend_func(SRC_ALPHA, ONE_MINUS_SRC_ALPHA);
    }

    // Set up the text program with the shaders

    let vertex_shader_source = r#"
#version 330 core
layout (location = 0) in vec4 vertex; // <vec2 pos, vec2 tex>
out vec2 TexCoords;

uniform mat4 projection;

void main() {
    gl_Position = projection * vec4(vertex.xy, 0.0, 1.0);
    TexCoords = vertex.zw;
}
"#;
    let fragment_shader_source = r#"
#version 330 core
in vec2 TexCoords;
out vec4 color;

uniform sampler2D text;
uniform vec3 textColor;

void main() {    
    vec4 sampled = vec4(1.0, 1.0, 1.0, texture(text, TexCoords).r);
    color = vec4(textColor, 1.0) * sampled;
}
"#;
    let program = unsafe {
        let program = gl.create_program().unwrap();
        let mut shaders = Vec::with_capacity(2);

        // Compile and attach the vertex shader
        {
            let shader = gl.create_shader(VERTEX_SHADER).unwrap();
            gl.shader_source(shader, &vertex_shader_source);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                panic!(
                    "error in the compilation of the vertex shader: {}",
                    gl.get_shader_info_log(shader)
                );
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }

        // Compile and attach the fragment shader
        {
            let shader = gl.create_shader(FRAGMENT_SHADER).unwrap();
            gl.shader_source(shader, &fragment_shader_source);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                panic!(
                    "error in the compilation of the fragment shader: {}",
                    gl.get_shader_info_log(shader)
                );
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }

        // Link the program
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            panic!(
                "error in the linking of the program: {}",
                gl.get_program_info_log(program)
            );
        }

        program
    };
    unsafe {
        gl.use_program(Some(program));
    }

    // Bind the VAO and the VBO

    let _vao = unsafe {
        let vao = gl.create_vertex_array().unwrap();
        gl.bind_vertex_array(Some(vao));

        vao
    };

    let _vbo = unsafe {
        let vbo = gl.create_buffer().unwrap();
        gl.bind_buffer(ARRAY_BUFFER, Some(vbo));

        // TODO(?): An error might lay in the next call
        let empty_buffer_data: &[u8] =
            core::slice::from_raw_parts(&[] as *const _, 6 * 4 * std::mem::size_of::<f32>());
        gl.buffer_data_u8_slice(ARRAY_BUFFER, empty_buffer_data, DYNAMIC_DRAW);

        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 4, FLOAT, false, 4 * std::mem::size_of::<f32>() as i32, 0);

        vbo
    };

    // Set the uniforms to their default values

    let projection_matrix = glm::ortho(0.0, window_width, 0.0, window_height, -1.0, 1.0);
    unsafe {
        let uniform_location = gl.get_uniform_location(program, "projection");
        gl.uniform_matrix_4_f32_slice(
            uniform_location.as_ref(),
            false,
            projection_matrix.as_slice(),
        );
    }

    unsafe {
        let uniform_location = gl.get_uniform_location(program, "text");
        gl.uniform_1_i32(uniform_location.as_ref(), 0);
    }

    let color = Vec3::new(0.0, 0.0, 0.0);
    unsafe {
        let uniform_location = gl.get_uniform_location(program, "textColor");
        gl.uniform_3_f32(uniform_location.as_ref(), color.x, color.y, color.z);
    }

    unsafe {
        // Disable the byte-alignment restriction
        gl.pixel_store_i32(UNPACK_ALIGNMENT, 1);
    }

    // Load the characters in the text from the chosen font

    let mut characters: HashMap<char, Character> = HashMap::new();

    for normalized_utf8_character in text.nfc() {
        // If it hasn't already been loaded...
        if characters.get(&normalized_utf8_character).is_some() {
            continue;
        } else {
            // ...load it
            load_character(&gl, &face, &mut characters, normalized_utf8_character);
        }
    }
    log::trace!(
        "Characters initially loaded from the text: {}",
        characters.len()
    );

    let wrapped_text: Vec<_> = textwrap::wrap(&text, average_line_length as usize)
        .iter()
        .map(|line| line.to_string())
        .collect();

    // The caret is the '|' character, at least for now. Load it.

    load_character(&gl, &face, &mut characters, '|');
    let caret_character = characters.get(&'|').unwrap().clone();
    // Set the caret position at the end of the wrapped text
    let mut caret = Caret {
        character: caret_character,
        position: IVec2::new(
            wrapped_text.last().unwrap().chars().count() as i32,
            wrapped_text.len() as i32 - 1,
        ),
    };

    let mut input_buffer = Vec::new();

    // Start the events loop...
    let (resize_event_transmitter, resize_event_receiver) = std::sync::mpsc::channel();
    let _resize_event_watcher = event_subsystem.add_event_watch(|event| match event {
        sdl2::event::Event::Window {
            win_event: window_event,
            ..
        } => match window_event {
            sdl2::event::WindowEvent::Resized(width, height) => {
                let (window_width, window_height) = (2.0 * width as f32, 2.0 * height as f32);

                unsafe {
                    gl.viewport(0, 0, window_width as i32, window_height as i32);
                    gl.clear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT);
                }
                window.gl_swap_window();
                resize_event_transmitter
                    .send((window_width, window_height))
                    .unwrap();
            }
            _ => (),
        },
        _ => (),
    });

    let mut mouse_position = Vec2::new(0.0, 0.0);

    'events_loop: loop {
        let mut mouse_pressed = false;
        let mut events = Vec::new();
        for event in event_loop.poll_iter() {
            match event {
                Event::Quit { .. } => {
                    break 'events_loop;
                }
                // Register the mouse position
                Event::MouseMotion { x, y, .. } => {
                    mouse_position = Vec2::new(x as f32 * 2.0, y as f32 * 2.0);
                }
                // Check if the mouse button is pressed
                Event::MouseButtonDown { .. } => {
                    mouse_pressed = true;
                }
                // Enable/disable blending when pressing Ctrl + A or Cmd + A
                Event::KeyDown {
                    keycode: Some(Keycode::A),
                    keymod: Mod::LCTRLMOD,
                    ..
                } => unsafe { gl.disable(BLEND) },
                Event::KeyUp {
                    keycode: Some(Keycode::A),
                    keymod: Mod::LCTRLMOD,
                    ..
                } => unsafe { gl.enable(BLEND) },
                // Save the opened document when the user presses Ctrl + S
                Event::KeyDown {
                    keycode: Some(Keycode::S),
                    keymod: Mod::LCTRLMOD,
                    ..
                } => {
                    // If the file creation is unsuccessful, then the user will lose their data
                    let mut file = File::create(document_path).unwrap();
                    file.write_all(text.as_bytes()).unwrap();
                    log::trace!("The document has been successfully saved");
                }
                _ => (),
            }
            // Collect the events one by one, then use them after for further matching
            events.push(event);
        }

        // Match for only one of the resizing events. This is because there's always 2 resizing events
        // emitted by SDL2, which is a repetition and/or bug.
        let mut resize_requested = false;
        while let Ok((width, height)) = resize_event_receiver.try_recv() {
            // Drain the channel
            resize_requested = true;
            (window_width, window_height) = (width, height);
        }
        if resize_requested {
            // TODO(!): The factor 2.0 is only for the sake of testing, it should be removed
            let projection_matrix = glm::ortho(0.0, window_width, 0.0, window_height, -1.0, 1.0);
            unsafe {
                let uniform_location = gl.get_uniform_location(program, "projection");
                gl.uniform_matrix_4_f32_slice(
                    uniform_location.as_ref(),
                    false,
                    projection_matrix.as_slice(),
                );
            }

            average_line_length = ((window_width - margins.left - margins.right)
                / average_character_advance as f32) as u32;
            line_height = window_height - font_size - margins.top;

            unsafe {
                gl.viewport(0, 0, window_width as i32, window_height as i32);
                gl.clear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT);
            }
        }

        // Each iteration of the loop, wrap the text in lines...
        let wrapped_text: Vec<_> = textwrap::wrap(&text, average_line_length as usize)
            .iter()
            .map(|line| line.to_string())
            .collect();

        // ...then calculate their lengths in order to set the caret index, used to find the caret
        // in the text to perform the usual insertions/removals/edits.
        let line_lengths = wrapped_text.iter().map(|line| line.len());
        let mut caret_index = line_lengths
            .clone()
            .take(caret.position.y as usize)
            .sum::<usize>()
            + caret.position.x as usize
            + caret.position.y as usize;

        for event in events {
            match event {
                Event::KeyDown {
                    keycode: Some(key),
                    keymod: Mod::NOMOD,
                    ..
                } => {
                    match key {
                        // Delete the character at the position of the caret
                        Keycode::Backspace => {
                            caret.position.x -= 1;
                            text.remove(caret_index - 1);
                        }
                        // Enter a newline when pressing enter at the position of the caret
                        Keycode::Return => {
                            text.insert(caret_index, '\n');
                            // Move the caret to the beginning of the next line
                            caret.position.x = 0;
                            caret.position.y += 1;
                        }
                        Keycode::Left => {
                            if caret.position.x > 0 {
                                caret.position.x -= 1;
                            }
                        }
                        Keycode::Right => {
                            if caret.position.x
                                < line_lengths.clone().nth(caret.position.y as usize).unwrap()
                                    as i32
                            {
                                caret.position.x += 1;
                            }
                        }
                        Keycode::Up => {
                            if caret.position.y > 0 {
                                caret.position.y -= 1;
                            }
                        }
                        Keycode::Down => {
                            if caret.position.y < wrapped_text.len() as i32 - 1 {
                                caret.position.y += 1;
                            }
                        }
                        _ => (),
                    }
                }
                // Receive text input from the keyboard, then append it to the last line
                Event::TextInput {
                    text: input_text, ..
                } => {
                    // Insert the input character at the position of the caret
                    input_buffer.push(input_text);
                }
                _ => (),
            }
            // Recalculate the caret index at each event as it may have been modified
            caret_index = line_lengths
                .clone()
                .take(caret.position.y as usize)
                .sum::<usize>()
                + caret.position.x as usize
                + caret.position.y as usize;
        }

        // Insert the text in the input buffer at the caret position and load each newly present
        // character in the input.
        for input in input_buffer.drain(..) {
            for input_character in input.chars() {
                text.insert(caret_index, input_character);
                caret.position.x += 1;
                caret_index = line_lengths
                    .clone()
                    .take(caret.position.y as usize)
                    .sum::<usize>()
                    + caret.position.x as usize
                    + caret.position.y as usize;

                let normalized_utf8_character = input_character.nfc().next().unwrap();
                if characters.get(&normalized_utf8_character).is_none() {
                    load_character(&gl, &face, &mut characters, normalized_utf8_character);
                }
            }
        }

        // Re-wrap the text after inserting the input buffer in text at the caret position
        let wrapped_text: Vec<_> = textwrap::wrap(&text, average_line_length as usize)
            .iter()
            .map(|line| line.to_string())
            .collect();

        // Begin the rendering, firstly clear the screen...
        unsafe {
            gl.clear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT);
        }

        // ...then, for each line in the wrapped text...
        for (line_index, line) in wrapped_text.iter().enumerate() {
            let mut horizontal_origin = margins.left;

            // ...draw each character therein present...
            for (character_index, char) in line.chars().enumerate() {
                let character = characters.get(&char).unwrap();

                let x = horizontal_origin + character.bearing.x as f32;
                let y = line_height - (character.size.y - character.bearing.y) as f32;

                let width = character.size.x as f32;
                let height = character.size.y as f32;

                let vertices = [
                    [x, y + height, 0.0, 0.0],
                    [x, y, 0.0, 1.0],
                    [x + width, y, 1.0, 1.0],
                    [x, y + height, 0.0, 0.0],
                    [x + width, y, 1.0, 1.0],
                    [x + width, y + height, 1.0, 0.0],
                ];

                let character_bounding_box = BoundingBox {
                    x,
                    y,
                    width,
                    height,
                };
                if mouse_pressed {
                    // If the mouse is pressed, check if the mouse is over any of the characters
                    // If it is, then move the caret to that position
                    if character_bounding_box.contains_position(mouse_position) {
                        caret.position =
                            IVec2::new(character_index as i32, (20 - line_index) as i32);
                    }
                }

                unsafe {
                    gl.bind_texture(TEXTURE_2D, Some(character.texture));

                    let vertices_slice = std::slice::from_raw_parts(
                        vertices.as_ptr() as *const _,
                        vertices.len() * 4 * std::mem::size_of::<f32>(),
                    );
                    gl.buffer_sub_data_u8_slice(ARRAY_BUFFER, 0, vertices_slice);
                }

                unsafe {
                    gl.draw_arrays(TRIANGLES, 0, 6);
                }

                // ...and then, eventually, draw the caret as well at its position
                let position_in_text = IVec2::new(character_index as i32, line_index as i32);
                // When the position in the text matches the one of the caret
                if position_in_text == caret.position {
                    let x = horizontal_origin - caret.character.bearing.x as f32;
                    let y =
                        line_height - (caret.character.size.y - caret.character.bearing.y) as f32;

                    let width = caret.character.size.x as f32;
                    let height = caret.character.size.y as f32;

                    let vertices = [
                        [x, y + height, 0.0, 0.0],
                        [x, y, 0.0, 1.0],
                        [x + width, y, 1.0, 1.0],
                        [x, y + height, 0.0, 0.0],
                        [x + width, y, 1.0, 1.0],
                        [x + width, y + height, 1.0, 0.0],
                    ];

                    unsafe {
                        gl.bind_texture(TEXTURE_2D, Some(caret.character.texture));

                        let vertices_slice = std::slice::from_raw_parts(
                            vertices.as_ptr() as *const _,
                            vertices.len() * 4 * std::mem::size_of::<f32>(),
                        );
                        gl.buffer_sub_data_u8_slice(ARRAY_BUFFER, 0, vertices_slice);
                    }

                    unsafe {
                        gl.draw_arrays(TRIANGLES, 0, 6);
                    }
                }

                // Move the origin by the character advance in order to draw the characters side-to-side.
                horizontal_origin += (character.advance >> 6) as f32; // Bitshift by 6 to get value in pixels (2^6 = 64)
            }

            // Move the line height below by the font size when each line is finished
            line_height -= font_size;
        }

        // In the end, reset the line height to its original value
        line_height = window_height - margins.top - font_size;

        unsafe {
            let error_code = gl.get_error();
            if error_code != 0 {
                panic!("OpenGL error code: {}", error_code);
            }
        }

        // Swap the windows in order to get rid of the previous frame, which is now obsolete.
        window.gl_swap_window();
    }
}

#[derive(Debug)]
struct BoundingBox {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl BoundingBox {
    fn contains_position(&self, position: Vec2) -> bool {
        position.x >= self.x
            && position.x <= self.x + self.width
            && position.y >= self.y
            && position.y <= self.y + self.height
    }
}

#[derive(Debug, Clone, Copy)]
struct Character {
    texture: NativeTexture, // ID handle of the glyph texture
    size: IVec2,            // Size of glyph
    bearing: IVec2,         // Offset from baseline to left/top of glyph
    advance: u32,           // Offset to advance to the next glyph
}

struct Caret {
    position: IVec2,
    character: Character,
}

unsafe fn texture_from_glyph(gl: &glow::Context, glyph: &freetype::GlyphSlot) -> NativeTexture {
    let texture = gl.create_texture().unwrap();
    gl.bind_texture(TEXTURE_2D, Some(texture));
    gl.tex_image_2d(
        TEXTURE_2D,
        0,
        RED as i32, // TODO: Why `RED`?
        glyph.bitmap().width(),
        glyph.bitmap().rows(),
        0,
        RED,
        UNSIGNED_BYTE,
        Some(glyph.bitmap().buffer()),
    );
    // Set the texture parameters for wrapping on the edges
    gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_WRAP_S, CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_WRAP_T, CLAMP_TO_EDGE as i32);
    // Set the texture min and mag filters to use linear filtering
    gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_MIN_FILTER, LINEAR as i32);
    gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_MAG_FILTER, LINEAR as i32);

    texture
}

fn load_character(
    gl: &glow::Context,
    face: &freetype::Face,
    characters: &mut HashMap<char, Character>,
    normalized_utf8_character: char,
) {
    face.load_char(
        normalized_utf8_character as usize,
        freetype::face::LoadFlag::RENDER,
    )
    .unwrap_or_else(|_| {
        panic!(
            "unable to find the character '{}' in the font",
            normalized_utf8_character
        )
    });
    let glyph = face.glyph();
    let texture = unsafe { texture_from_glyph(&gl, glyph) };

    let character = Character {
        texture,
        size: IVec2::new(glyph.bitmap().width(), glyph.bitmap().rows()),
        bearing: IVec2::new(glyph.bitmap_left(), glyph.bitmap_top()),
        advance: glyph.advance().x as u32,
    };
    characters.insert(normalized_utf8_character, character);
}
