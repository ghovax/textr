// ...
pub mod shader;
pub mod text_atlas;

use std::ffi::c_uint;

use glad_gl::gl::*;

pub struct Vbo {
    id: u32,
    shader_index: u32,
}

impl Vbo {
    pub fn new(index: u32) -> Self {
        let mut vbo = 0;
        unsafe {
            GenBuffers(1, &mut vbo);
        }
        Self {
            id: vbo,
            shader_index: index,
        }
    }

    pub fn bind(&self) {
        unsafe { BindBuffer(ARRAY_BUFFER, self.id) }
    }

    pub fn buffer_data(&self, data: &[f32], usage: c_uint) {
        unsafe {
            BindBuffer(ARRAY_BUFFER, self.id);
            BufferData(
                ARRAY_BUFFER,
                std::mem::size_of_val(data) as isize,
                data.as_ptr() as *const _,
                usage,
            );
        }
    }

    pub fn sub_buffer_data(&self, data: &[f32], offset: isize) {
        unsafe {
            BindBuffer(ARRAY_BUFFER, self.id);
            BufferSubData(ARRAY_BUFFER, offset, std::mem::size_of_val(data) as isize, data.as_ptr() as *const _);
        }
    }

    pub fn configure(&self, size: i32, stride: i32) {
        unsafe {
            EnableVertexAttribArray(self.shader_index);
            BindBuffer(ARRAY_BUFFER, self.id);
            VertexAttribPointer(self.shader_index, size, FLOAT, FALSE, stride, std::ptr::null());
        }
    }

    pub fn unbind(&self) {
        unsafe { DisableVertexAttribArray(self.shader_index) }
    }

    pub fn delete(&self) {
        unsafe {
            DeleteBuffers(1, &self.id);
        }
    }

    pub fn delete_array(&self) {
        unsafe {
            DeleteVertexArrays(1, &self.id);
        }
    }
}

pub struct Vao(u32);

impl Vao {
    pub fn new() -> Self {
        let mut vao = 0;
        unsafe {
            GenVertexArrays(1, &mut vao);
        }
        Self(vao)
    }

    pub fn bind(&self) {
        unsafe { BindVertexArray(self.0) }
    }

    pub fn delete_array(&self) {
        unsafe { DeleteVertexArrays(1, &self.0) }
    }
}

pub struct Ebo(u32);

impl Ebo {
    pub fn new() -> Self {
        let mut ebo = 0;
        unsafe {
            GenBuffers(1, &mut ebo);
        }
        Ebo(ebo)
    }

    pub fn bind(&self) {
        unsafe { BindBuffer(ELEMENT_ARRAY_BUFFER, self.0) }
    }

    pub fn buffer_data(&self, data: &[u16], usage: c_uint) {
        unsafe {
            BufferData(
                ELEMENT_ARRAY_BUFFER,
                std::mem::size_of_val(data) as _,
                data.as_ptr() as _,
                usage,
            );
        }
    }
}
