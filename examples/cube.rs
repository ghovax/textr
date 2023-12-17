use std::path::Path;

use glad_gl::gl::*;
use glfw::{Action, Context, Key, WindowHint};
use glm::Vec3;
use nalgebra_glm as glm;
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
    glfw.window_hint(WindowHint::ContextVersion(3, 3));
    glfw.window_hint(WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));
    glfw.window_hint(WindowHint::Samples(Some(4)));

    if cfg!(target_os = "macos") {
        glfw.window_hint(WindowHint::OpenGlForwardCompat(true));
    }

    let (mut window, events) = glfw
        .create_window(
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
            "Cube",
            glfw::WindowMode::Windowed,
        )
        .expect("failed to create GLFW window");

    window.set_key_polling(true);
    window.make_current();
    window.set_sticky_keys(true);

    glad_gl::gl::load(|procname| glfw.get_proc_address_raw(procname) as *const _);

    unsafe {
        Enable(DEPTH_TEST);
        DepthFunc(LESS);
    }

    let vertex_source = r#"
#version 330 core

// Input vertex data, different for all executions of this shader.
layout(location = 0) in vec3 vertexPosition_modelspace;
layout(location = 1) in vec3 vertexColor;

// Output data ; will be interpolated for each fragment.
out vec3 fragmentColor;
// Values that stay constant for the whole mesh.
uniform mat4 MVP;

void main(){	

	// Output position of the vertex, in clip space : MVP * position
	gl_Position =  MVP * vec4(vertexPosition_modelspace,1);

	// The color of each vertex will be interpolated
	// to produce the color of each fragment
	fragmentColor = vertexColor;
}
"#;
    let fragment_source = r#"
#version 330 core

// Interpolated values from the vertex shaders
in vec3 fragmentColor;

// Ouput data
out vec3 color;

void main(){

	// Output color = color specified in the vertex shader, 
	// interpolated between all 3 surrounding vertices
	color = fragmentColor;

}
"#;
    let shader = Shader::new_from_source(vertex_source, fragment_source);
    shader.use_program();

    // Configure the uniform MVP matrix
    let projection_matrix =
        glm::perspective(SCREEN_WIDTH as f32 / SCREEN_HEIGHT as f32, 45.0, 0.1, 100.0);
    let view_matrix = glm::look_at(
        &Vec3::new(4.0, 3.0, 2.0),
        &Vec3::new(0.0, 0.0, 0.0),
        &Vec3::new(0.0, 1.0, 0.0),
    );
    shader.set_mat4("MVP", projection_matrix * view_matrix);

    let vao = Vao::new();
    vao.bind();

    unsafe {
        // PolygonMode(FRONT_AND_BACK, LINE);
        ClearColor(0.0, 0.0, 0.4, 0.0);
        // Enable(BLEND);
        // BlendFunc(SRC_ALPHA, ONE_MINUS_SRC_ALPHA);
    }

    let vertices = [
        -1.0_f32, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0,
        -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0,
        -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0,
        1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0,
        1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0,
        1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0,
        1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0,
    ];
    let vbo = Vbo::new(0);
    vbo.bind();
    vbo.buffer_data(&vertices, STATIC_DRAW);

    let colors = [
        0.583_f32, 0.771, 0.014, 0.609, 0.115, 0.436, 0.327, 0.483, 0.844, 0.822, 0.569, 0.201,
        0.435, 0.602, 0.223, 0.310, 0.747, 0.185, 0.597, 0.770, 0.761, 0.559, 0.436, 0.730, 0.359,
        0.583, 0.152, 0.483, 0.596, 0.789, 0.559, 0.861, 0.639, 0.195, 0.548, 0.859, 0.014, 0.184,
        0.576, 0.771, 0.328, 0.970, 0.406, 0.615, 0.116, 0.676, 0.977, 0.133, 0.971, 0.572, 0.833,
        0.140, 0.616, 0.489, 0.997, 0.513, 0.064, 0.945, 0.719, 0.592, 0.543, 0.021, 0.978, 0.279,
        0.317, 0.505, 0.167, 0.620, 0.077, 0.347, 0.857, 0.137, 0.055, 0.953, 0.042, 0.714, 0.505,
        0.345, 0.783, 0.290, 0.734, 0.722, 0.645, 0.174, 0.302, 0.455, 0.848, 0.225, 0.587, 0.040,
        0.517, 0.713, 0.338, 0.053, 0.959, 0.120, 0.393, 0.621, 0.362, 0.673, 0.211, 0.457, 0.820,
        0.883, 0.371, 0.982, 0.099, 0.879,
    ];
    let colors_vbo = Vbo::new(1);
    colors_vbo.bind();
    colors_vbo.buffer_data(&colors, STATIC_DRAW);

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
            // Draw to the screen
            DrawArrays(TRIANGLES, 0, 36);
        }

        // Unbind the data
        vbo.unbind();
        colors_vbo.unbind();

        unsafe {
            let error_code = GetError();
            if error_code != 0 {
                println!("{}", error_code);
            }
        }

        window.swap_buffers();
    }

    // Manually delete the VBO and VAO
    vbo.delete_array();
    colors_vbo.delete();
    vao.delete_array();
    // Manually delete the shader
    shader.delete_program();
}

fn handle_window_event(window: &mut glfw::Window, event: glfw::WindowEvent) {
    match event {
        glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => window.set_should_close(true),
        _ => {}
    }
}
