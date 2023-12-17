use std::{collections::HashMap, path::Path};

use glad_gl::gl::*;
use glfw::{Action, Context, Key, Modifiers, WindowHint};
use glm::{IVec2, Vec3};
use nalgebra_glm as glm;
use textr::{shader::Shader, Texture, Vao, Vbo};
use unicode_normalization::UnicodeNormalization;

const SCREEN_WIDTH: u32 = 800;
const SCREEN_HEIGHT: u32 = 600;

fn main() {
    env_logger::init();

    // GLFW window stuff
    let mut glfw = glfw::init(glfw::fail_on_errors).unwrap();
    glfw.window_hint(WindowHint::ContextVersion(3, 3));
    glfw.window_hint(WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));

    if cfg!(target_os = "macos") {
        glfw.window_hint(WindowHint::OpenGlForwardCompat(true));
    }

    let (mut window, events) = glfw
        .create_window(
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
            "Text rendering",
            glfw::WindowMode::Windowed,
        )
        .expect("failed to create GLFW window");

    let (screen_width, screen_height) = window.get_framebuffer_size();

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
    let mut text: Vec<_> = textwrap::wrap("This is sample text! Welcome to my test document everyone! My name is Giovanni Gravili and I'm a master degree student at UNIBO.", 28).iter().map(|line| line.to_string()).collect();
    let font_path = Path::new("fonts/cmunrm.ttf");
    let face = library.new_face(font_path, 0).unwrap();
    let font_size = 60;
    face.set_pixel_sizes(0, font_size).unwrap(); // TODO: `pixel_width` is 0?

    unsafe {
        // Disable the byte-alignment restriction
        PixelStorei(UNPACK_ALIGNMENT, 1);
    }

    let mut characters: HashMap<char, Character> = HashMap::new();

    for character_code in 0..=128_u8 {
        // Before it was `text.nfc()`
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

    // println!("{:?}", characters);

    unsafe {
        BindTexture(TEXTURE_2D, 0);
    }

    let color = Vec3::new(0.0, 0.0, 0.0);
    let x_position = 10.0;
    let mut y_position: f32 = (screen_height - font_size as i32) as f32;
    let scale = 1.0;

    while !window.should_close() {
        glfw.wait_events();
        for (_, event) in glfw::flush_messages(&events) {
            match event {
                glfw::WindowEvent::FramebufferSize(width, height) => {
                    // Make sure the viewport matches the new window dimensions; note that width and
                    // height will be significantly larger than specified on retina displays.
                    let projection_matrix =
                        glm::ortho(0.0, width as f32, 0.0, height as f32, -1.0, 1.0);
                    shader.set_mat4("projection", projection_matrix);
                    y_position = height as f32 - font_size as f32;
                    unsafe {
                        Viewport(0, 0, width, height);
                        Clear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT)
                    }
                    // window.swap_buffers();
                }
                // Receive text input from the keyboard, then append it to the last line
                glfw::WindowEvent::Char(character) => {
                    // If the character is a newline character, add a new line
                    match character {
                        '\n' => {
                            text.push("".to_string());
                        }
                        _ => {
                            let last_line = text.last_mut().unwrap();
                            last_line.push(character);
                        }
                    }
                }
                // Delete the last character from the last line
                glfw::WindowEvent::Key(Key::Backspace, _, Action::Repeat | Action::Press, _) => {
                    let last_line = text.last_mut().unwrap();
                    last_line.pop();
                }
                glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => {
                    window.set_should_close(true);
                }
                _ => (),
            }
        }

        unsafe {
            // ClearDepth(1.0);
            // Viewport(100, 0, SCREEN_WIDTH as i32, SCREEN_HEIGHT as i32);
            Clear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT);
        }

        shader.set_vec3("textColor", color);
        unsafe {
            ActiveTexture(TEXTURE0);
        }
        vao.bind();

        // Wrap the text in a vector of strings, each string representing a line of text
        // Join them but respect the newlines inserted by the user
        let has_ending_space = text.last().unwrap().ends_with(' ');
        text = textwrap::wrap(&text.join(" "), 28).iter().map(|line| line.to_string()).collect();
        if has_ending_space {
            text.last_mut().unwrap().push(' ');
        }

        for line in text.iter() {
            let mut x = x_position;
            y_position -= font_size as f32;

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

        }

        y_position = (screen_height - font_size as i32) as f32;

        unsafe {
            BindVertexArray(0);
            BindTexture(TEXTURE_2D, 0);
        }

        unsafe {
            let error_code = GetError();
            if error_code != 0 {
                println!("{}", error_code);
            }
            match error_code {
                1282 => panic!("error 1282 encountered"),
                _ => (),
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
