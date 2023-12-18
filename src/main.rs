use std::{collections::HashMap, fs::File, path::Path};

use chrono::Local;
use glad_gl::gl::*;
use glfw::{Action, Context, Key, Modifiers, WindowHint};
use glm::{IVec2, Vec3};
use log::LevelFilter;
use nalgebra_glm as glm;
use shader::Shader;
use buffers::{Texture, Vao, Vbo};
use unicode_normalization::UnicodeNormalization;

use std::io::Write;

mod buffers;
mod shader;

fn main() {
    let target = Box::new(File::create("console.log").expect("can't create log file"));
    env_logger::Builder::new()
        .target(env_logger::Target::Stderr)
        .write_style(env_logger::WriteStyle::Always)
        .format_target(false)
        .format_timestamp_millis()
        .filter(None, LevelFilter::Trace)
        .init();

    // GLFW window stuff
    let mut glfw = glfw::init(glfw::fail_on_errors).unwrap();
    glfw.window_hint(WindowHint::ContextVersion(3, 3));
    glfw.window_hint(WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));

    if cfg!(target_os = "macos") {
        glfw.window_hint(WindowHint::OpenGlForwardCompat(true));
    }

    let (mut window, events) = glfw
        .create_window(800, 600, "Text rendering", glfw::WindowMode::Windowed)
        .expect("failed to create GLFW window");

    let (mut screen_width, mut screen_height) = window.get_framebuffer_size();

    window.make_current();
    window.set_resizable(true);
    window.set_all_polling(true);

    glad_gl::gl::load(|procname| glfw.get_proc_address_raw(procname) as *const _);

    glfw.set_swap_interval(glfw::SwapInterval::Sync(1));

    unsafe {
        Enable(CULL_FACE);
        ClearColor(1.0, 1.0, 1.0, 1.0);
        Enable(BLEND);
        BlendFunc(SRC_ALPHA, ONE_MINUS_SRC_ALPHA);
    }

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
        screen_width as f32,
        0.0,
        screen_height as f32,
        -1.0,
        1.0,
    );
    shader.set_mat4("projection", projection_matrix);
    shader.set_int("text", 0);

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

    // Freetype library stuff
    let library: freetype::Library = freetype::Library::init().unwrap();

    // Load the characters of of the ASCII table
    let mut text = "This is sample text! 1 ≠ 2 of course, but also ã if you may".to_string();
    let mut line_length = 40;
    let mut wrapped_text: Vec<_> = textwrap::wrap(&text, line_length)
        .iter()
        .map(|line| line.to_string())
        .collect();
    let font_path = Path::new("fonts/NewCMMath-Regular.otf");
    let face = library.new_face(font_path, 0).unwrap();
    let font_size = 60;
    face.set_pixel_sizes(0, font_size).unwrap(); // TODO: `pixel_width` is 0?

    unsafe {
        // Disable the byte-alignment restriction
        PixelStorei(UNPACK_ALIGNMENT, 1);
    }

    let mut characters: HashMap<char, Character> = HashMap::new();

    for character_code in text.nfc() {
        if characters.get(&(character_code as char)).is_some() {
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
    log::trace!("Characters loaded: {}", characters.len(),);

    unsafe {
        BindTexture(TEXTURE_2D, 0);
    }

    let color = Vec3::new(0.0, 0.0, 0.0);
    shader.set_vec3("textColor", color);
    let x_position = 10.0;
    let mut y_position: f32 = (screen_height - font_size as i32) as f32;
    let scale = 1.0;

    while !window.should_close() {
        glfw.wait_events();
        for (_, event) in glfw::flush_messages(&events) {
            match event {
                // Disable blending when pressing Alt + A
                glfw::WindowEvent::Key(
                    Key::A,
                    _,
                    action,
                    Modifiers::Super | Modifiers::Control,
                ) => match action {
                    Action::Press => {
                        unsafe {
                            Disable(BLEND);
                        }
                        log::trace!("Blending disabled");
                    }
                    Action::Release => {
                        unsafe {
                            Enable(BLEND);
                        }
                        log::trace!("Blending enabled");
                    }
                    _ => (),
                },
                glfw::WindowEvent::FramebufferSize(width, height) => {
                    // Make sure the viewport matches the new window dimensions; note that width and
                    // height will be significantly larger than specified on retina displays.
                    (screen_width, screen_height) = (width, height);
                    let projection_matrix = glm::ortho(
                        0.0,
                        screen_width as f32,
                        0.0,
                        screen_height as f32,
                        -1.0,
                        1.0,
                    );
                    shader.set_mat4("projection", projection_matrix);
                    y_position = screen_height as f32 - font_size as f32;
                    unsafe {
                        Viewport(0, 0, screen_width, screen_height);
                        Clear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT)
                    }
                    // window.swap_buffers();
                }
                // Receive text input from the keyboard, then append it to the last line
                glfw::WindowEvent::Char(input_character) => {
                    text.push(input_character);
                    let character_code = input_character.nfc().next().unwrap();
                    if characters.get(&character_code).is_none() {
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
                        characters.insert(character_code, character);
                        log::trace!("Loaded on the fly the character '{}'", input_character);
                    }
                    log::trace!("Inserted the character '{}'", input_character);
                }
                // Delete the last character from the last line
                glfw::WindowEvent::Key(Key::Backspace, _, Action::Repeat | Action::Press, _) => {
                    match text.pop() {
                        Some(deleted_character) => {
                            log::trace!("Deleted the character '{}'", deleted_character);
                        }
                        None => {
                            log::trace!("Attempting to delete, but no character left to delete");
                        }
                    }
                }
                // Enter a newline when pressing enter
                glfw::WindowEvent::Key(Key::Enter, _, Action::Repeat | Action::Press, _) => {
                    text.push('\n');
                    log::trace!("Inserted a new line");
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
            // Viewport(100, 0, SCREEN_WIDTH as i32, SCREEN_HEIGHT as i32);
            Clear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT);
        }

        unsafe {
            ActiveTexture(TEXTURE0);
        }
        vao.bind();

        // ------ ALGORITHM FOR SPLITTING THE TEXT INTO MULTIPLE LINES ------

        // Wrap the text in a vector of strings, each string representing a line of text
        // Join them but respect the newlines inserted by the user
        wrapped_text = textwrap::wrap(&text, line_length)
            .iter()
            .map(|line| line.to_string())
            .collect();

        for line in wrapped_text.iter() {
            let mut x = x_position;

            for character in line.chars() {
                let character = characters.get(&character).unwrap();

                let u = x + character.bearing.x as f32 * scale;
                let v = y_position - (character.size.y - character.bearing.y) as f32 * scale;

                let width = character.size.x as f32 * scale;
                let height = character.size.y as f32 * scale;

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

                x += (character.advance >> 6) as f32 * scale; // Bitshift by 6 to get value in pixels (2^6 = 64)
            }

            y_position -= font_size as f32;
        }

        y_position = (screen_height - font_size as i32) as f32;

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
