use std::path::Path;

use glad_gl::gl::*;
use glfw::{Action, Context, Key, WindowHint};
use glm::Vec3;
use nalgebra_glm as glm;
use textr::{shader::Shader, Ebo, Vao, Vbo};

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
            "Colored cube",
            glfw::WindowMode::Windowed,
        )
        .expect("failed to create GLFW window");

    window.set_key_polling(true);
    window.make_current();
    // glfw.set_swap_interval(glfw::SwapInterval::None);

    glad_gl::gl::load(|procname| glfw.get_proc_address_raw(procname) as *const _);

    unsafe {
        Enable(DEPTH_TEST);
        DepthFunc(LESS);
        ClearColor(0.1, 0.12, 0.2, 1.0);
    }

    let vao = Vao::new();
    vao.bind();

    let indices = [
        // Front
        0_u16, 1, 2, 2, 3, 0, // Right
        0, 3, 7, 7, 4, 0, // Bottom
        2, 6, 7, 7, 3, 2, // Left
        1, 5, 6, 6, 2, 1, // Back
        4, 7, 6, 6, 5, 4, // Top
        5, 1, 0, 0, 4, 5,
    ];
    let indices_ebo = Ebo::new();
    indices_ebo.bind();
    indices_ebo.buffer_data(&indices, STATIC_DRAW);

    let vertices = [
        // Front face
        0.5_f32, 0.5, 0.5, -0.5, 0.5, 0.5, -0.5, -0.5, 0.5, 0.5, -0.5, 0.5, // Back face
        0.5, 0.5, -0.5, -0.5, 0.5, -0.5, -0.5, -0.5, -0.5, 0.5, -0.5, -0.5,
    ];
    let vbo = Vbo::new(0);
    vbo.bind();
    vbo.buffer_data(&vertices, STATIC_DRAW);

    let colors = [
        1.0_f32, 0.4, 0.6, 1.0, 0.9, 0.2, 0.7, 0.3, 0.8, 0.5, 0.3, 1.0, 0.2, 0.6, 1.0, 0.6, 1.0,
        0.4, 0.6, 0.8, 0.8, 0.4, 0.8, 0.8,
    ];
    let colors_vbo = Vbo::new(1);
    colors_vbo.bind();
    colors_vbo.buffer_data(&colors, STATIC_DRAW);

    let vertex_source = r#"
#version 410

layout(location = 0) in vec3 pos;
layout(location = 1) in vec3 vertex_color;

uniform mat4 transform;

out vec3 color;

void main() {
  gl_Position = transform * vec4(pos, 1.0);
  color = vertex_color;
}
"#;
    let fragment_source = r#"
#version 410

in vec3 color;

out vec4 frag_color;

void main() {
  frag_color = vec4(color, 1.0);
}
"#;
    let shader = Shader::new_from_source(vertex_source, fragment_source);
    shader.use_program();

    let projection_matrix =
        glm::perspective(SCREEN_WIDTH as f32 / SCREEN_HEIGHT as f32, 45.0, 0.1, 100.0);
    let view_matrix = glm::look_at(
        &Vec3::new(2.0, 1.5, 2.0),
        &Vec3::new(0.0, 0.0, 0.0),
        &Vec3::new(0.0, 1.0, 0.0),
    );
    shader.set_mat4("transform", projection_matrix * view_matrix);

    while !window.should_close() {
        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events) {
            handle_window_event(&mut window, event);
        }

        unsafe {
            // ClearDepth(1.0);
            Clear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT);
        }

        vbo.configure(3, 0);
        colors_vbo.configure(3, 0);

        unsafe {
            DrawElements(TRIANGLES, 6 * 2 * 3, UNSIGNED_SHORT, 0 as _);
        }

        unsafe {
            let error_code = GetError();
            if error_code != 0 {
                println!("{}", error_code);
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
