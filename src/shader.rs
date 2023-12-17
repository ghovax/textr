#![allow(dead_code)]

use glad_gl::gl::*;
use nalgebra_glm::{Mat4, Vec2, Vec3};
use std::{
    ffi::{c_uint, CString},
    path::Path,
};

pub struct Shader {
    program_id: u32,
}

impl Shader {
    pub fn new(vertex_path: &Path, fragment_path: &Path) -> Shader {
        let vertex_shader_source = std::fs::read_to_string(vertex_path).unwrap();
        let fragment_shader_source = std::fs::read_to_string(fragment_path).unwrap();

        Self::new_from_source(&vertex_shader_source, &fragment_shader_source)
    }

    pub fn new_from_source(vertex_source: &str, fragment_source: &str) -> Shader {
        let vertex_shader = shader_from_source(vertex_source, VERTEX_SHADER);
        let fragment_shader = shader_from_source(fragment_source, FRAGMENT_SHADER);
        let program_id = program_from_shaders(vertex_shader, fragment_shader);
        unsafe {
            DeleteShader(vertex_shader);
            DeleteShader(fragment_shader);
        }

        Shader { program_id }
    }

    pub fn delete_program(&self) {
        unsafe {
            DeleteProgram(self.program_id);
        }
    }

    pub fn use_program(&self) {
        unsafe {
            UseProgram(self.program_id);
        }
    }

    pub fn set_int(&self, name: &str, value: i32) {
        unsafe {
            Uniform1i(
                GetUniformLocation(self.program_id, name.as_ptr() as *const i8),
                value,
            );
        }
    }

    pub fn set_float(&self, name: &str, value: f32) {
        unsafe {
            Uniform1f(
                GetUniformLocation(self.program_id, name.as_ptr() as *const i8),
                value,
            );
        }
    }

    pub fn set_vec3(&self, name: &str, value: Vec3) {
        unsafe {
            Uniform3f(
                GetUniformLocation(self.program_id, name.as_ptr() as *const i8),
                value.x,
                value.y,
                value.z,
            );
        }
    }

    pub fn set_mat4(&self, name: &str, value: Mat4) {
        unsafe {
            UniformMatrix4fv(
                GetUniformLocation(self.program_id, name.as_ptr() as *const i8),
                1,
                FALSE,
                value.as_ptr(),
            );
        }
    }

    pub fn set_vec2(&self, name: &str, value: Vec2) {
        unsafe {
            Uniform2f(
                GetUniformLocation(self.program_id, name.as_ptr() as *const i8),
                value.x,
                value.y,
            );
        }
    }

    pub fn set_bool(&self, name: &str, value: bool) {
        self.set_int(name, value as i32);
    }
}

fn shader_from_source(shader_source: &str, shader_type: c_uint) -> u32 {
    let mut _shader_id = unsafe { CreateShader(shader_type) };

    unsafe {
        // Shader compilation
        let c_str_source = CString::new(shader_source).unwrap();
        ShaderSource(_shader_id, 1, &c_str_source.as_ptr(), std::ptr::null());
        CompileShader(_shader_id);
    }

    let mut success: i32 = 1;
    unsafe {
        GetShaderiv(_shader_id, COMPILE_STATUS, &mut success);
        if success == 0 {
            let error = retrieve_error(_shader_id);
            panic!(
                "error in the compilation of the {} shader:\n{}",
                match shader_type {
                    VERTEX_SHADER => "vertex",
                    FRAGMENT_SHADER => "fragment",
                    _ => "unknown type",
                },
                error
            );
        }
    }

    _shader_id
}

fn program_from_shaders(vertex_shader: u32, fragment_shader: u32) -> u32 {
    let program_id = unsafe { CreateProgram() };

    unsafe {
        // Program linking
        AttachShader(program_id, vertex_shader);
        AttachShader(program_id, fragment_shader);

        LinkProgram(program_id);
    }

    let mut success = 1;
    unsafe {
        GetProgramiv(program_id, LINK_STATUS, &mut success);
        if success == 0 {
            let error = retrieve_error(program_id);
            panic!("error in the linking of the program:\n{}", error);
        }
    }

    program_id
}

fn retrieve_error(program_id: u32) -> String {
    let mut length = 0;
    let mut _error = CString::new("").unwrap();
    unsafe {
        GetProgramiv(program_id, INFO_LOG_LENGTH, &mut length);
        let mut buffer: Vec<u8> = Vec::with_capacity(length as usize + 1);
        buffer.extend([b' '].iter().cycle().take(length as usize));
        _error = CString::from_vec_unchecked(buffer);
        GetProgramInfoLog(
            program_id,
            length,
            std::ptr::null_mut(),
            _error.as_ptr() as *mut _,
        );
    }
    _error.to_string_lossy().into_owned()
}
