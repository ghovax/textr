use std::path::Path;

use glad_gl::gl::*;
use glfw::{Action, Context, Key, WindowHint};

use textr::{shader::Shader, Vao, Vbo};

const SCREEN_WIDTH: u32 = 800;
const SCREEN_HEIGHT: u32 = 600;

#[allow(dead_code)]
fn framebuffer_size_callback(_window: &mut glfw::Window, width: u32, height: u32) {
    unsafe {
        Viewport(0, 0, width as i32, height as i32);
    }
}

fn main() {
    // GLFW window stuff
    let mut glfw = glfw::init(glfw::fail_on_errors).unwrap();
    glfw.window_hint(WindowHint::ContextVersionMajor(3));
    glfw.window_hint(WindowHint::ContextVersionMinor(3));
    glfw.window_hint(WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));

    if cfg!(target_os = "macos") {
        glfw.window_hint(WindowHint::OpenGlForwardCompat(true));
    }

    let (mut window, events) = glfw
        .create_window(
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
            "Triangle",
            glfw::WindowMode::Windowed,
        )
        .expect("failed to create GLFW window");

    window.set_key_polling(true);
    window.make_current();

    glad_gl::gl::load(|procname| glfw.get_proc_address_raw(procname) as *const _);

    // Configure the shader
    let vertex_source = r#"
#version 330 core
layout (location = 0) in vec3 aPos;

void main() {
    gl_Position = vec4(aPos.x, aPos.y, aPos.z, 1.0);
}
"#;
    let fragment_source = r#"
#version 330 core
out vec4 FragColor;

void main() {
    FragColor = vec4(1.0f, 0.5f, 0.2f, 1.0f);
} 
"#;
    let shader = Shader::new_from_source(vertex_source, fragment_source);

    let vao = Vao::new();
    vao.bind();

    // Load the vertices data into the VBO
    let vertices = [-0.5_f32, -0.5, 0.0, 0.5, -0.5, 0.0, 0.0, 0.5, 0.0];
    let vbo = Vbo::new(0);
    vbo.bind();
    vbo.buffer_data(&vertices, STATIC_DRAW);

    unsafe {
        // PolygonMode(FRONT_AND_BACK, LINE);
        Enable(BLEND);
        BlendFunc(SRC_ALPHA, ONE_MINUS_SRC_ALPHA);
    }

    while !window.should_close() {
        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events) {
            handle_window_event(&mut window, event);
        }

        unsafe {
            ClearColor(0.2, 0.3, 0.3, 1.0);
            // ClearDepth(1.0);
            Clear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT);
        }

        shader.use_program();
        vbo.configure(3, 0);
        unsafe {
            DrawArrays(TRIANGLES, 0, 3);
        }
        vbo.unbind();

        unsafe {
            let error_code = GetError();
            if error_code != 0 {
                println!("{}", error_code);
            }
        }

        window.swap_buffers();
    }

    shader.delete_program();

    vao.delete_array();
    vbo.delete();
}

fn handle_window_event(window: &mut glfw::Window, event: glfw::WindowEvent) {
    match event {
        glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => window.set_should_close(true),
        _ => {}
    }
}
