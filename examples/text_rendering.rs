use std::path::Path;

use glad_gl::gl::*;
use glfw::{Action, Context, Key, WindowHint};
use glm::Vec3;
use nalgebra_glm as glm;
use textr::{shader::Shader, text_atlas::TextAtlas};

const SCREEN_WIDTH: u32 = 800;
const SCREEN_HEIGHT: u32 = 600;

fn main() {
    // GLFW window stuff
    let mut glfw = glfw::init(glfw::fail_on_errors).unwrap();
    glfw.window_hint(WindowHint::ContextVersion(4, 1));
    glfw.window_hint(WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));

    if cfg!(target_os = "macos") {
        glfw.window_hint(WindowHint::OpenGlForwardCompat(true));
    }

    let (mut window, events) = glfw
        .create_window(
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
            "textr",
            glfw::WindowMode::Windowed,
        )
        .expect("failed to create GLFW window");

    window.set_key_polling(true);
    window.make_current();

    glad_gl::gl::load(|procname| glfw.get_proc_address_raw(procname) as *const _);

    unsafe {
        // Enable(CULL_FACE);
        ClearColor(0.2, 0.3, 0.3, 1.0);
        Enable(BLEND);
        BlendFunc(SRC_ALPHA, ONE_MINUS_SRC_ALPHA);
    }

    let vertex_path = Path::new("shaders/main/vertex.glsl");
    let fragment_path = Path::new("shaders/main/fragment.glsl");
    let shader = Shader::new(vertex_path, fragment_path);
    let projection = glm::ortho(
        0.0,
        SCREEN_WIDTH as f32,
        0.0,
        SCREEN_WIDTH as f32,
        -1.0,
        1.0,
    );

    // Freetype library stuff
    let library: freetype::Library = freetype::Library::init().unwrap();

    // Load the characters of of the ASCII table
    let text = "This is sample text";
    let font_path = Path::new("fonts/SourceCodePro-VariableFont_wght.ttf");
    let mut atlas = TextAtlas::new(&library, font_path);
    atlas.load_characters(text);

    while !window.should_close() {
        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events) {
            handle_window_event(&mut window, event);
        }

        unsafe {
            // ClearDepth(1.0);
            Clear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT);
        }

        shader.use_program();
        shader.set_mat4("projection", projection);

        atlas.configure();
        atlas.render_text(&shader, text, 10.0, 10.0, 1.0, Vec3::new(0.5, 0.8, 0.2));

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

fn handle_window_event(window: &mut glfw::Window, event: glfw::WindowEvent) {
    match event {
        glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => window.set_should_close(true),
        _ => {}
    }
}
