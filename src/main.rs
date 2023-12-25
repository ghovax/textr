use std::{
    collections::HashMap,
    fs::{File, Permissions},
    path::Path,
    time::Instant,
};

use buffers::{Texture, Vao, Vbo};
use chrono::Local;
// use clap::Parser;
use gl::*;
use glfw::{Action, Context, Key, Modifiers, WindowHint};
use glm::{IVec2, Vec2, Vec3};
use itertools::Itertools;
use log::LevelFilter;
use nalgebra_glm as glm;
use shader::Shader;
use unicode_normalization::UnicodeNormalization;

use std::io::Write;

mod buffers;
mod cursor;
mod line;
mod shader;

use crate::line::Margins;

fn framebuffer_size_callback(window: &mut glfw::Window, width: i32, height: i32) {
    unsafe {
        // gl::Viewport(0, 0, width, height);
        Clear(COLOR_BUFFER_BIT)
    }
    window.swap_buffers()
}

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

    let mut glfw = glfw::init(glfw::log_errors).unwrap();

    glfw.window_hint(WindowHint::ContextVersion(3, 3));
    glfw.window_hint(WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));
    glfw.window_hint(WindowHint::RefreshRate(Some(60)));
    glfw.window_hint(WindowHint::Samples(Some(4)));

    if cfg!(target_os = "macos") {
        glfw.window_hint(WindowHint::CocoaRetinaFramebuffer(true));
        glfw.window_hint(WindowHint::OpenGlForwardCompat(true));
    }

    let (mut window, events) = glfw
        .create_window(800, 600, &document_path, glfw::WindowMode::Windowed)
        .expect("failed to create GLFW window");

    window.make_current();
    window.set_resizable(true);
    window.set_all_polling(true);
    window.set_framebuffer_size_callback(framebuffer_size_callback);
    window.set_size_limits(Some(480), Some(320), None, None);

    // --------- LOAD THE LIBRARY FREETYPE FOR THE GLYPHS ---------

    let library: freetype::Library = freetype::Library::init().unwrap();

    // Load the text from the file path given
    let mut text = std::fs::read_to_string(&document_path).unwrap();
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

    let framebuffer_size = window.get_framebuffer_size();
    let (mut window_width, mut window_height) =
        (framebuffer_size.0 as f32, framebuffer_size.1 as f32);

    let mut character_advances = Vec::new();
    for character_code in
        r#"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890'`~<,.>/?"\|;:]}[{=+"#
            .chars()
            .nfc()
    {
        face.load_char(character_code as usize, freetype::face::LoadFlag::RENDER)
            .unwrap();
        let glyph = face.glyph();

        character_advances.push((glyph.advance().x as u32) >> 6);
    }
    let average_character_advance =
        (character_advances.iter().sum::<u32>() as f32 / character_advances.len() as f32) as u32; // Bitshift by 6 to convert in pixels

    let mut line_length_in_characters =
        ((window_width - margins.left - margins.right) / average_character_advance as f32) as u32;
    log::trace!("Line length in characters: {}", line_length_in_characters);

    // --------- LOAD OPENGL V3.3 FROM GLFW ---------

    gl::load_with(|procname| glfw.get_proc_address_raw(procname) as *const _);

    glfw.set_swap_interval(glfw::SwapInterval::Sync(1));

    // --------- CULLING, CLEAR COLOR AND BLENDING SET ---------

    unsafe {
        Enable(CULL_FACE);
        ClearColor(1.0, 1.0, 1.0, 1.0);
        Enable(BLEND);
        BlendFunc(SRC_ALPHA, ONE_MINUS_SRC_ALPHA);
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
    let shader = Shader::new_from_source(vertex_source, fragment_source);
    shader.use_program();

    let projection_matrix = glm::ortho(
        0.0,
        window_width as f32,
        0.0,
        window_height as f32,
        -1.0,
        1.0,
    );
    shader.set_mat4("projection", projection_matrix);
    shader.set_int("text", 0);

    // --------- BIND THE VAO AND VBO FOR RENDERING THE TEXT ---------

    // Configure VAO/VBO for texture quads
    let vao = Vao::new();
    vao.bind();

    let vbo = Vbo::new(0);
    vbo.bind();
    unsafe {
        BufferData(
            ARRAY_BUFFER,
            (6 * 4 * std::mem::size_of::<f32>()) as isize,
            std::ptr::null(),
            DYNAMIC_DRAW,
        );
        EnableVertexAttribArray(0);
        VertexAttribPointer(
            0,
            4,
            FLOAT,
            FALSE,
            (4 * std::mem::size_of::<f32>()) as i32,
            std::ptr::null(),
        );
    }

    unsafe {
        BindBuffer(ARRAY_BUFFER, 0);
        BindVertexArray(0);
    }

    let mut y_text_position = window_height - font_size - margins.top;

    unsafe {
        // Disable the byte-alignment restriction
        PixelStorei(UNPACK_ALIGNMENT, 1);
    }

    // ------ LOAD THE CURSOR CHARACTER ------

    let mut characters: HashMap<char, Character> = HashMap::new();

    face.load_char('|' as usize, freetype::face::LoadFlag::RENDER)
        .unwrap();
    let glyph = face.glyph();

    let texture = Texture::new();
    texture.bind();
    texture.image_2d(
        glyph.bitmap().width(),
        glyph.bitmap().rows(),
        glyph.bitmap().buffer(),
    );
    texture.set_parameters(CLAMP_TO_EDGE, CLAMP_TO_EDGE, NEAREST, NEAREST);

    let character = Character {
        texture,
        size: IVec2::new(glyph.bitmap().width(), glyph.bitmap().rows()),
        bearing: IVec2::new(glyph.bitmap_left(), glyph.bitmap_top()),
        advance: glyph.advance().x as u32,
    };
    characters.insert('|' as char, character);
    let cursor_character = characters.get(&'|').unwrap().clone();

    // --------- REQUEST TO LOAD THE CHARACTERS IN THE TEXT FROM THE CHOSEN FONT ---------

    for character_code in text.nfc() {
        if characters.get(&character_code).is_some() {
            continue;
        } else {
            face.load_char(character_code as usize, freetype::face::LoadFlag::RENDER)
                .unwrap();
            let glyph = face.glyph();

            let texture = Texture::new();
            texture.bind();
            texture.image_2d(
                glyph.bitmap().width(),
                glyph.bitmap().rows(),
                glyph.bitmap().buffer(),
            );
            texture.set_parameters(CLAMP_TO_EDGE, CLAMP_TO_EDGE, NEAREST, NEAREST);

            let character = Character {
                texture,
                size: IVec2::new(glyph.bitmap().width(), glyph.bitmap().rows()),
                bearing: IVec2::new(glyph.bitmap_left(), glyph.bitmap_top()),
                advance: glyph.advance().x as u32,
            };
            characters.insert(character_code as char, character);
        }
    }
    log::trace!("Characters initially loaded: {}", characters.len());

    unsafe {
        BindTexture(TEXTURE_2D, 0);
    }

    // --------- START THE RENDER/EVENTS LOOP ---------

    let color = Vec3::new(0.0, 0.0, 0.0);
    shader.set_vec3("textColor", color);

    let mut mouse_position = Vec2::zeros();

    let mut wrapped_text: Vec<_> = textwrap::wrap(&text, line_length_in_characters as usize)
        .iter()
        .map(|line| line.to_string())
        .collect();

    // Set the cursor position at the end of the wrapped text
    let mut cursor_position = IVec2::new(
        wrapped_text.last().unwrap().chars().count() as i32,
        wrapped_text.len() as i32 - 1,
    );

    while !window.should_close() {
        glfw.poll_events();

        // Filter out all the `WindowEvent::FramebufferSize` except the last one
        let events = glfw::flush_messages(&events).collect_vec();
        let mut last_resize_event_index = None;

        for (index, (_, event)) in events.iter().enumerate() {
            match event {
                glfw::WindowEvent::FramebufferSize(_, _) => last_resize_event_index = Some(index),
                _ => (),
            }
        }

        if let Some(index) = last_resize_event_index {
            match events.get(index).unwrap() {
                (_, glfw::WindowEvent::FramebufferSize(width, height)) => {
                    (window_width, window_height) = (*width as f32, *height as f32);
                    log::trace!(
                        "Window resized to pixel size: {:?}",
                        IVec2::new(*width, *height).as_slice()
                    );

                    let projection_matrix =
                        glm::ortho(0.0, window_width, 0.0, window_height, -1.0, 1.0);
                    shader.set_mat4("projection", projection_matrix);

                    line_length_in_characters = ((window_width - margins.left - margins.right)
                        / average_character_advance as f32)
                        as u32;
                    y_text_position = window_height - font_size - margins.top;

                    unsafe {
                        Viewport(0, 0, *width, *height);
                        Clear(COLOR_BUFFER_BIT);
                    }
                    window.swap_buffers()
                }
                _ => unreachable!(),
            }
        }

        for (_, event) in events.iter() {
            match event {
                // Disable blending when pressing Ctrl + A
                glfw::WindowEvent::Key(
                    Key::A,
                    _,
                    action,
                    Modifiers::Super | Modifiers::Control,
                ) => match action {
                    Action::Press | Action::Repeat => unsafe {
                        Disable(BLEND);
                    },
                    Action::Release => unsafe {
                        Enable(BLEND);
                    },
                },
                glfw::WindowEvent::CursorPos(x, y) => {
                    mouse_position = Vec2::new(*x as f32, *y as f32);
                    // log::trace!("Cursor position: {:?}", cursor_position);
                }
                // Save the opened document when the user presses Ctrl + S
                glfw::WindowEvent::Key(
                    Key::S,
                    _,
                    Action::Press,
                    Modifiers::Control | Modifiers::Super,
                ) => {
                    log::trace!("Saving the document");
                    let mut file = File::create(&document_path).unwrap();
                    file.write_all(text.as_bytes()).unwrap();
                }
                // Enter a newline when pressing enter
                glfw::WindowEvent::Key(Key::Enter, _, Action::Repeat | Action::Press, _) => {
                    text.push('\n');
                }
                glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _)
                | glfw::WindowEvent::Close => {
                    window.set_should_close(true);
                    log::trace!("Requesting to close the window");
                }
                _ => (),
            }
        }

        unsafe {
            // ClearDepth(1.0);
            // Viewport(0, 0, window_width as i32, window_height as i32);
            Clear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT);
        }
        // window.swap_buffers();

        unsafe {
            ActiveTexture(TEXTURE0);
        }
        vao.bind();

        // ------ ALGORITHM FOR SPLITTING THE TEXT INTO MULTIPLE LINES ------

        // Wrap the text in a vector of strings, each string representing a line of text
        // Join them but respect the newlines inserted by the user
        wrapped_text = textwrap::wrap(&text, line_length_in_characters as usize)
            .iter()
            .map(|line| line.to_string())
            .collect();

        let mut line_lengths = wrapped_text.iter().map(|line| line.len());
        let cursor_index_position = line_lengths
            .clone()
            .take(cursor_position.y as usize)
            .sum::<usize>()
            + cursor_position.x as usize
            + cursor_position.y as usize;

        for (_, event) in events.iter() {
            match event {
                // Delete the character at the position of the cursor
                glfw::WindowEvent::Key(Key::Backspace, _, Action::Repeat | Action::Press, _) => {
                    // We have multiple lines of text, each with their own length. The index is calculated by going
                    // through the lengths of the lines and summing them.

                    // TODO: This works, but when the line is rearranged, then the cursor isn't brought to the word that eventually
                    // had to move because of this rearrangement.

                    cursor_position.x -= 1;

                    text.remove(cursor_index_position - 1);
                }
                glfw::WindowEvent::Key(Key::Left, _, Action::Repeat | Action::Press, _) => {
                    if cursor_position.x > 0 {
                        cursor_position.x -= 1;
                    }
                }
                glfw::WindowEvent::Key(Key::Right, _, Action::Repeat | Action::Press, _) => {
                    if cursor_position.x
                        < line_lengths.nth(cursor_position.y as usize).unwrap() as i32
                    {
                        cursor_position.x += 1;
                    }
                }
                glfw::WindowEvent::Key(Key::Up, _, Action::Repeat | Action::Press, _) => {
                    if cursor_position.y > 0 {
                        cursor_position.y -= 1;
                    }
                }
                glfw::WindowEvent::Key(Key::Down, _, Action::Repeat | Action::Press, _) => {
                    if cursor_position.y < wrapped_text.len() as i32 - 1 {
                        cursor_position.y += 1;
                    }
                }
                // Receive text input from the keyboard, then append it to the last line
                glfw::WindowEvent::Char(input_character) => {
                    // Insert the input character at the position of the cursor
                    text.insert(cursor_index_position, *input_character);
                    cursor_position.x += 1;

                    let character_code = input_character.nfc().next().unwrap();
                    if characters.get(&character_code).is_none() {
                        face.load_char(character_code as usize, freetype::face::LoadFlag::RENDER)
                            .expect(
                                format!(
                                    "unable to find the character '{}' in the font",
                                    character_code
                                )
                                .as_str(),
                            );
                        let glyph = face.glyph();

                        let texture = Texture::new();
                        texture.bind();
                        texture.image_2d(
                            glyph.bitmap().width(),
                            glyph.bitmap().rows(),
                            glyph.bitmap().buffer(),
                        );
                        texture.set_parameters(CLAMP_TO_EDGE, CLAMP_TO_EDGE, NEAREST, NEAREST);

                        let character = Character {
                            texture,
                            size: IVec2::new(glyph.bitmap().width(), glyph.bitmap().rows()),
                            bearing: IVec2::new(glyph.bitmap_left(), glyph.bitmap_top()),
                            advance: glyph.advance().x as u32,
                        };
                        characters.insert(character_code, character);
                    }
                }
                _ => (),
            }
        }

        wrapped_text = textwrap::wrap(&text, line_length_in_characters as usize)
            .iter()
            .map(|line| line.to_string())
            .collect();

        for (line_index, line) in wrapped_text.iter().enumerate() {
            let mut x_text_position = margins.left as f32;

            for (character_index, character) in line.chars().enumerate() {
                let character = characters.get(&character).unwrap();

                let u = x_text_position + character.bearing.x as f32;
                let v = y_text_position - (character.size.y - character.bearing.y) as f32;

                let width = character.size.x as f32;
                let height = character.size.y as f32;

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

                character.texture.bind();
                vbo.bind();
                unsafe {
                    BufferSubData(
                        ARRAY_BUFFER,
                        0,
                        (vertices.len() * 4 * std::mem::size_of::<f32>()) as isize,
                        vertices.as_ptr() as *const _,
                    );
                }

                unsafe {
                    BindBuffer(ARRAY_BUFFER, 0);
                }

                unsafe {
                    DrawArrays(TRIANGLES, 0, 6);
                }

                // ------ ALGORITHM FOR FINDING THE CURSOR POSITION AND DRAWING IT ------

                let position_in_text = IVec2::new(character_index as i32, line_index as i32);
                if position_in_text == cursor_position {
                    // When the position in the text matches the one of the cursor

                    let u = x_text_position - character.bearing.x as f32;
                    let v = y_text_position
                        - (cursor_character.size.y - cursor_character.bearing.y) as f32;

                    let width = cursor_character.size.x as f32;
                    let height = cursor_character.size.y as f32;

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

                    cursor_character.texture.bind();
                    vbo.bind();
                    unsafe {
                        BufferSubData(
                            ARRAY_BUFFER,
                            0,
                            (vertices.len() * 4 * std::mem::size_of::<f32>()) as isize,
                            vertices.as_ptr() as *const _,
                        );
                    }

                    unsafe {
                        BindBuffer(ARRAY_BUFFER, 0);
                    }

                    unsafe {
                        DrawArrays(TRIANGLES, 0, 6);
                    }
                }

                x_text_position += (character.advance >> 6) as f32; // Bitshift by 6 to get value in pixels (2^6 = 64)
            }

            y_text_position -= font_size;
        }

        y_text_position = window_height - margins.top - font_size;

        unsafe {
            BindVertexArray(0);
            BindTexture(TEXTURE_2D, 0);
        }

        unsafe {
            let error_code = GetError();
            if error_code != 0 {
                log::error!("OpenGL error code: {}", error_code);
            }
        }

        window.swap_buffers();
    }
}

#[derive(Debug, Clone, Copy)]
struct Character {
    texture: Texture, // ID handle of the glyph texture
    size: IVec2,      // Size of glyph
    bearing: IVec2,   // Offset from baseline to left/top of glyph
    advance: u32,     // Offset to advance to the next glyph
}
