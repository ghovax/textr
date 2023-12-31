use std::{collections::HashMap, fs::File};

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

    // --------- INITIALIZE THE GLFW WINDOW ---------

    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();

    let gl_attributes = video.gl_attr();
    gl_attributes.set_context_profile(sdl2::video::GLProfile::Core);
    gl_attributes.set_context_version(3, 3);
    gl_attributes.set_context_flags().forward_compatible().set();

    gl_attributes.set_accelerated_visual(true);
    gl_attributes.set_multisample_samples(4);

    let mut window = video
        .window(document_path, 1024, 769)
        .opengl()
        // .resizable()
        .allow_highdpi()
        .build()
        .unwrap();
    window.set_minimum_size(480, 320).unwrap();

    // --------- LOAD OPENGL V3.3 FROM GLFW ---------

    let _gl_context = window.gl_create_context().unwrap();
    let gl = unsafe {
        glow::Context::from_loader_function(|s| video.gl_get_proc_address(s) as *const _)
    };
    let mut event_loop = sdl.event_pump().unwrap();

    // --------- LOAD THE LIBRARY FREETYPE FOR THE GLYPHS ---------

    let library: freetype::Library = freetype::Library::init().unwrap();

    // Load the text from the file path given
    let mut text = std::fs::read_to_string(document_path).unwrap();
    log::trace!("Imported the text: {:?}", text);
    let face = library.new_face(font_path, 0).unwrap();

    // --------- CALCULATE THE LINE LENGTH BASED ON AVERAGE CHARACTER ADVANCE ---------

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

    let mut character_advances = Vec::new();
    for character_nfc in
        r#"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890'`~<,.>/?"\|;:]}[{=+"#
            .chars()
            .nfc()
    {
        face.load_char(character_nfc as usize, freetype::face::LoadFlag::RENDER)
            .unwrap();
        let glyph = face.glyph();

        character_advances.push((glyph.advance().x as u32) >> 6);
    }
    let average_character_advance =
        (character_advances.iter().sum::<u32>() as f32 / character_advances.len() as f32) as u32; // Bitshift by 6 to convert in pixels

    let mut line_length_in_characters =
        ((window_width - margins.left - margins.right) / average_character_advance as f32) as u32;
    log::trace!("Line length in characters: {}", line_length_in_characters);

    // --------- CULLING, CLEAR COLOR AND BLENDING SET ---------

    unsafe {
        gl.enable(CULL_FACE);
        gl.clear_color(1.0, 1.0, 1.0, 1.0);
        gl.enable(BLEND);
        gl.blend_func(SRC_ALPHA, ONE_MINUS_SRC_ALPHA);
    }

    // --------- SET UP THE TEXT SHADER ---------

    let vertex_source = r#"
#version 330 core
layout (location = 0) in vec4 vertex; // <vec2 pos, vec2 tex>
out vec2 TexCoords;

uniform mat4 projection;

void main() {
    gl_Position = projection * vec4(vertex.xy, 0.0, 1.0);
    TexCoords = vertex.zw;
}
"#;
    let fragment_source = r#"
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
        for (shader_type, shader_source) in [
            (VERTEX_SHADER, vertex_source),
            (FRAGMENT_SHADER, fragment_source),
        ] {
            let shader = gl.create_shader(shader_type).unwrap();
            gl.shader_source(shader, &shader_source);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                panic!("{}", gl.get_shader_info_log(shader));
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }

        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            panic!("{}", gl.get_program_info_log(program));
        }

        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }

        program
    };
    unsafe {
        gl.use_program(Some(program));
    }

    // --------- BIND THE VAO AND VBO FOR RENDERING THE TEXT ---------

    let vao = unsafe {
        let vao = gl.create_vertex_array().unwrap();
        gl.bind_vertex_array(Some(vao));

        vao
    };

    let vbo = unsafe {
        let vbo = gl.create_buffer().unwrap();
        gl.bind_buffer(ARRAY_BUFFER, Some(vbo));
        // TODO: An error might lay in the next call
        let empty_buffer_data: &[u8] =
            core::slice::from_raw_parts(&[] as *const _, 6 * 4 * std::mem::size_of::<f32>());
        gl.buffer_data_u8_slice(ARRAY_BUFFER, empty_buffer_data, DYNAMIC_DRAW);
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 4, FLOAT, false, 4 * std::mem::size_of::<f32>() as i32, 0);

        vbo
    };

    // TODO: What does this do?
    unsafe {
        gl.bind_buffer(ARRAY_BUFFER, None);
        gl.bind_vertex_array(None);
    }

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

    let mut line_text_position = window_height - font_size - margins.top;

    unsafe {
        // Disable the byte-alignment restriction
        gl.pixel_store_i32(UNPACK_ALIGNMENT, 1);
    }

    let mut characters: HashMap<char, Character> = HashMap::new();

    // --------- REQUEST TO LOAD THE CHARACTERS IN THE TEXT FROM THE CHOSEN FONT ---------

    for character_nfc in text.nfc() {
        if characters.get(&character_nfc).is_some() {
            continue;
        } else {
            insert_character(&gl, &face, &mut characters, character_nfc)
        }
    }
    log::trace!("Characters initially loaded: {}", characters.len());

    unsafe {
        gl.bind_texture(TEXTURE_2D, None);
    }

    // --------- START THE RENDER/EVENTS LOOP ---------

    let color = Vec3::new(0.0, 0.0, 0.0);
    unsafe {
        let uniform_location = gl.get_uniform_location(program, "textColor");
        gl.uniform_3_f32(uniform_location.as_ref(), color.x, color.y, color.z);
    }

    let mut mouse_position = Vec2::zeros();

    let mut wrapped_text: Vec<_> = textwrap::wrap(&text, line_length_in_characters as usize)
        .iter()
        .map(|line| line.to_string())
        .collect();

    // ------- LOAD THE Caret --------

    insert_character(&gl, &face, &mut characters, '|');
    let caret_character = characters.get(&'|').unwrap().clone();
    let mut caret = Caret::new(caret_character, wrapped_text);

    // Set the caret position at the end of the wrapped text
    let mut render_requested = true;
    let mut input_buffer = Vec::new();

    'render_loop: loop {
        let mut events = Vec::new();
        for event in event_loop.wait_timeout_iter(16) {
            match event {
                Event::Quit { .. } => {
                    break 'render_loop;
                }
                // Enable/disable blending when pressing Ctrl + A or Cmd + A
                Event::KeyDown {
                    keycode: Some(Keycode::A),
                    keymod: Mod::LCTRLMOD,
                    ..
                } => {
                    render_requested = true;
                    unsafe { gl.disable(BLEND) }
                }
                Event::KeyUp {
                    keycode: Some(Keycode::A),
                    keymod: Mod::LCTRLMOD,
                    ..
                } => {
                    render_requested = true;
                    unsafe { gl.enable(BLEND) }
                }
                // Save the opened document when the user presses Ctrl + S
                Event::KeyDown {
                    keycode: Some(Keycode::S),
                    keymod: Mod::LCTRLMOD,
                    ..
                } => {
                    log::trace!("Saving the document");
                    let mut file = File::create(document_path).unwrap();
                    file.write_all(text.as_bytes()).unwrap();
                }
                _ => (),
            }
            events.push(event);
        }

        match events.iter().find(|event| match event {
            sdl2::event::Event::Window { .. } => true,
            _ => false,
        }) {
            Some(sdl2::event::Event::Window {
                win_event: window_event,
                ..
            }) => match window_event {
                sdl2::event::WindowEvent::Resized(width, height) => {
                    render_requested = true;
                    // TODO: The factor 2.0 is only for the sake of testing, it should be removed
                    (window_width, window_height) = (2.0 * *width as f32, 2.0 * *height as f32);

                    let projection_matrix =
                        glm::ortho(0.0, window_width, 0.0, window_height, -1.0, 1.0);
                    unsafe {
                        let uniform_location = gl.get_uniform_location(program, "projection");
                        gl.uniform_matrix_4_f32_slice(
                            uniform_location.as_ref(),
                            false,
                            projection_matrix.as_slice(),
                        );
                    }

                    line_length_in_characters = ((window_width - margins.left - margins.right)
                        / average_character_advance as f32)
                        as u32;
                    line_text_position = window_height - font_size - margins.top;

                    unsafe {
                        gl.viewport(0, 0, window_width as i32, window_height as i32);
                        gl.clear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT);
                    }
                    window.gl_swap_window();

                    log::trace!(
                        "Window resized to pixel size: {:?}",
                        IVec2::new(window_width as i32, window_height as i32).as_slice()
                    );
                }
                _ => (),
            },
            _ => (),
        }

        // ------ ALGORITHM FOR SPLITTING THE TEXT INTO MULTIPLE LINES ------

        // Wrap the text in a vector of strings, each string representing a line of text
        // Join them but respect the newlines inserted by the user
        wrapped_text = textwrap::wrap(&text, line_length_in_characters as usize)
            .iter()
            .map(|line| line.to_string())
            .collect();

        let mut line_lengths = wrapped_text.iter().map(|line| line.len());
        let mut caret_index_position = line_lengths
            .clone()
            .take(caret.position.y as usize)
            .sum::<usize>()
            + caret.position.x as usize
            + caret.position.y as usize;

        for event in events {
            match event {
                Event::KeyDown {
                    keycode: Some(key), ..
                } => {
                    render_requested = true;
                    match key {
                        // Delete the character at the position of the caret
                        Keycode::Backspace => {
                            caret.position.x -= 1;
                            text.remove(caret_index_position - 1);
                        }
                        // Enter a newline when pressing enter at the position of the caret
                        Keycode::Return => {
                            text.insert(caret_index_position, '\n');
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
                    render_requested = true;
                    // Insert the input character at the position of the caret
                    input_buffer.push(input_text);
                }
                _ => (),
            }
            caret_index_position = line_lengths
                .clone()
                .take(caret.position.y as usize)
                .sum::<usize>()
                + caret.position.x as usize
                + caret.position.y as usize;
        }

        // Basic replacement logic for inserting characters not available on the keyboard.
        // Search for '\\u{XXXX}' in the text and replace it with the equivalent UTF8 symbol
        let regex = Regex::new(r"\\u\{([0-9A-Fa-f]{4})\}").unwrap();
        let mut text_with_unicode_symbols = text.clone();
        for capture in regex.captures_iter(&text) {
            // Obtain the equivalent UTF8 symbol from the 4 digits
            let utf8_code_str = capture.get(1).unwrap().as_str();
            let unicode_symbol =
                char::from_u32(u32::from_str_radix(utf8_code_str, 16).unwrap()).unwrap();
            text_with_unicode_symbols = text.replace(
                capture.get(0).unwrap().as_str(),
                unicode_symbol.to_string().as_str(),
            );
        }
        text = text_with_unicode_symbols;

        // ------------- RENDER ON REQUEST -------------

        if !render_requested {
            continue;
        }
        render_requested = false;

        for input in input_buffer.drain(..) {
            for input_character in input.chars() {
                text.insert(caret_index_position, input_character);
                caret.position.x += 1;
                caret_index_position = line_lengths
                    .clone()
                    .take(caret.position.y as usize)
                    .sum::<usize>()
                    + caret.position.x as usize
                    + caret.position.y as usize;

                let character_nfc = input_character.nfc().next().unwrap();
                if characters.get(&character_nfc).is_none() {
                    insert_character(&gl, &face, &mut characters, character_nfc);
                }
            }
        }

        for character_nfc in text.nfc() {
            if characters.get(&character_nfc).is_some() {
                continue;
            } else {
                insert_character(&gl, &face, &mut characters, character_nfc);

                log::trace!(
                    "Dynamically loaded the character '{}' from its UTF8 code: {}",
                    character_nfc,
                    format!("{:04X}", character_nfc as u32)
                );
            }
        }

        unsafe {
            gl.clear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT);
        }

        unsafe {
            gl.active_texture(TEXTURE0);
            gl.bind_vertex_array(Some(vao));
        }

        wrapped_text = textwrap::wrap(&text, line_length_in_characters as usize)
            .iter()
            .map(|line| line.to_string())
            .collect();

        for (line_index, line) in wrapped_text.iter().enumerate() {
            let mut character_text_position = margins.left;

            for (character_index, character) in line.chars().enumerate() {
                let character = characters.get(&character).unwrap();

                let u = character_text_position + character.bearing.x as f32;
                let v = line_text_position - (character.size.y - character.bearing.y) as f32;

                let width = character.size.x as f32;
                let height = character.size.y as f32;

                draw_character(&gl, u, v, width, height, character.texture, vbo);

                // ------ ALGORITHM FOR FINDING THE CARET POSITION AND DRAWING IT ------

                caret.draw(
                    &gl,
                    character_index,
                    line_index,
                    character_text_position,
                    line_text_position,
                    vbo,
                );

                character_text_position += (character.advance >> 6) as f32; // Bitshift by 6 to get value in pixels (2^6 = 64)
            }

            line_text_position -= font_size;
        }

        line_text_position = window_height - margins.top - font_size;

        unsafe {
            gl.bind_vertex_array(None);
            gl.bind_texture(TEXTURE_2D, None);
        }

        unsafe {
            let error_code = gl.get_error();
            if error_code != 0 {
                log::error!("OpenGL error code: {}", error_code);
            }
        }

        window.gl_swap_window();
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
    blink: bool,
    character: Character,
}

impl Caret {
    pub fn new(character: Character, wrapped_text: Vec<String>) -> Self {
        Caret {
            character,
            position: IVec2::new(
                wrapped_text.last().unwrap().chars().count() as i32,
                wrapped_text.len() as i32 - 1,
            ),
            blink: false,
        }
    }

    fn draw(
        &mut self,
        gl: &glow::Context,
        character_index: usize,
        line_index: usize,
        character_text_position: f32,
        line_text_position: f32,
        vbo: NativeBuffer,
    ) {
        let position_in_text = IVec2::new(character_index as i32, line_index as i32);
        if position_in_text == self.position {
            // When the position in the text matches the one of the caret

            let u = character_text_position - self.character.bearing.x as f32;
            let v = line_text_position - (self.character.size.y - self.character.bearing.y) as f32;

            let width = self.character.size.x as f32;
            let height = self.character.size.y as f32;

            draw_character(&gl, u, v, width, height, self.character.texture, vbo)
        }
    }
}

trait NativeTextureExt {
    unsafe fn from_glyph(gl: &glow::Context, glyph: &freetype::GlyphSlot) -> Self;
}

impl NativeTextureExt for NativeTexture {
    unsafe fn from_glyph(gl: &glow::Context, glyph: &freetype::GlyphSlot) -> Self {
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
        // Set the texture parameters
        gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_WRAP_S, CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_WRAP_T, CLAMP_TO_EDGE as i32);
        // Set the texture min and mag filters
        gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_MIN_FILTER, NEAREST as i32);
        gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_MAG_FILTER, NEAREST as i32);

        texture
    }
}

fn insert_character(
    gl: &glow::Context,
    face: &freetype::Face,
    characters: &mut HashMap<char, Character>,
    character_nfc: char,
) {
    face.load_char(character_nfc as usize, freetype::face::LoadFlag::RENDER)
        .unwrap_or_else(|_| {
            panic!(
                "unable to find the character '{}' in the font",
                character_nfc
            )
        });
    let glyph = face.glyph();

    let texture = unsafe { NativeTexture::from_glyph(&gl, glyph) };

    let character = Character {
        texture,
        size: IVec2::new(glyph.bitmap().width(), glyph.bitmap().rows()),
        bearing: IVec2::new(glyph.bitmap_left(), glyph.bitmap_top()),
        advance: glyph.advance().x as u32,
    };
    characters.insert(character_nfc, character);
}

fn draw_character(
    gl: &glow::Context,
    u: f32,
    v: f32,
    width: f32,
    height: f32,
    texture: NativeTexture,
    vbo: NativeBuffer,
) {
    let vertices = {
        [
            [u, v + height, 0.0, 0.0],
            [u, v, 0.0, 1.0],
            [u + width, v, 1.0, 1.0],
            [u, v + height, 0.0, 0.0],
            [u + width, v, 1.0, 1.0],
            [u + width, v + height, 1.0, 0.0],
        ]
    };

    unsafe {
        gl.bind_texture(TEXTURE_2D, Some(texture));

        gl.bind_buffer(ARRAY_BUFFER, Some(vbo));
        let vertices_slice = std::slice::from_raw_parts(
            vertices.as_ptr() as *const _,
            vertices.len() * 4 * std::mem::size_of::<f32>(),
        );
        gl.buffer_sub_data_u8_slice(ARRAY_BUFFER, 0, vertices_slice);
    }

    unsafe {
        gl.bind_buffer(ARRAY_BUFFER, None);

        gl.draw_arrays(TRIANGLES, 0, 6);
    }
}
