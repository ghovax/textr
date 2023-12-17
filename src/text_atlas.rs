use freetype::{Face, Library};
use glad_gl::gl::*;
use glm::{IVec2, Vec3};
use nalgebra_glm as glm;
use std::{collections::HashMap, path::Path};
use unicode_normalization::UnicodeNormalization;

use crate::{shader::Shader, Vao, Vbo};

#[derive(Debug, Clone, Copy)]
struct Character {
    texture_id: u32, // ID handle of the glyph texture
    size: IVec2,     // Size of glyph
    bearing: IVec2,  // Offset from baseline to left/top of glyph
    advance: u32,    // Offset to advance to the next glyph
}

pub struct TextAtlas {
    characters: HashMap<char, Character>,
    face: Face,
    vao: Vao,
    vbo: Vbo,
}

impl TextAtlas {
    pub fn new(library: &Library, font_path: &Path) -> Self {
        let face = library.new_face(font_path, 0).unwrap();
        face.set_pixel_sizes(0, 48).unwrap(); // TODO: `pixel_width` is 0?

        unsafe {
            // Disable byte-alignment restriction
            PixelStorei(UNPACK_ALIGNMENT, 1);
            BindTexture(TEXTURE_2D, 0);
        }

        let vao = Vao::new();
        vao.bind();

        let vbo = Vbo::new(0);
        vbo.bind();
        unsafe {
            BufferData(
                ARRAY_BUFFER,
                (std::mem::size_of::<f32>() * 6 * 4) as isize, // sizeof(float) * 6 * 4
                std::ptr::null(),
                DYNAMIC_DRAW,
            );
        }

        Self {
            characters: HashMap::new(),
            face,
            vao,
            vbo,
        }
    }

    pub fn load_characters(&mut self, text: &str) {
        for character_code in text.nfc() {
            if self.characters.get(&character_code).is_some() {
                continue;
            } else {
                println!("{}: {}", character_code, character_code as usize);
                self.face
                    .load_char(character_code as usize, freetype::face::LoadFlag::RENDER)
                    .unwrap();
                let glyph = self.face.glyph();

                let mut texture: u32 = 0;
                unsafe {
                    GenTextures(1, &mut texture);
                    BindTexture(TEXTURE_2D, texture);

                    // Wrap settings
                    TexParameteri(TEXTURE_2D, TEXTURE_WRAP_S, CLAMP_TO_EDGE as i32);
                    TexParameteri(TEXTURE_2D, TEXTURE_WRAP_T, CLAMP_TO_EDGE as i32);
                    // View filters
                    TexParameteri(TEXTURE_2D, TEXTURE_MIN_FILTER, NEAREST as i32);
                    TexParameteri(TEXTURE_2D, TEXTURE_MAG_FILTER, NEAREST as i32);

                    TexImage2D(
                        TEXTURE_2D,
                        0,
                        RED as i32,
                        glyph.bitmap().width(),
                        glyph.bitmap().rows(),
                        1,
                        RED,
                        UNSIGNED_BYTE,
                        glyph.bitmap().buffer().as_ptr() as *const _,
                    );
                }

                let character = Character {
                    texture_id: texture,
                    size: IVec2::new(glyph.bitmap().width(), glyph.bitmap().rows()),
                    bearing: IVec2::new(glyph.bitmap_left(), glyph.bitmap_top()),
                    advance: glyph.advance().x as u32,
                };
                self.characters.insert(character_code, character);
            }
        }
    }

    pub fn configure(&self) {
        self.vao.bind();
        self.vbo.configure(4, 4 * 4);
    }

    pub fn render_text(
        &self,
        shader: &Shader,
        text: &str,
        x: f32,
        y: f32,
        scale: f32,
        color: Vec3,
    ) {
        shader.use_program();
        shader.set_vec3("textColor", color);

        unsafe {
            ActiveTexture(TEXTURE0);
        }

        self.vao.bind();

        let mut x = x;
        for character in text.chars() {
            let character = self.characters.get(&character).unwrap();

            let u = x + character.bearing.x as f32 * scale;
            let v = y - (character.size.y - character.bearing.y) as f32 * scale;

            let width = character.size.x as f32 * scale;
            let height = character.size.y as f32 * scale;

            let vertices: [[f32; 4]; 6] = {
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
                BindTexture(TEXTURE_2D, character.texture_id);
            }

            unsafe {
                BindBuffer(ARRAY_BUFFER, 0);
                BufferSubData(
                    ARRAY_BUFFER,
                    0,
                    (6 * 4 * std::mem::size_of::<f32>()) as isize,
                    vertices.as_ptr() as *const _,
                );
            }

            unsafe {
                DrawArrays(TRIANGLES, 0, 6);
            }

            x += (character.advance >> 6) as f32 * scale; // Bitshift by 6 to get value in pixels (2^6 = 64)
        }

        unsafe {
            BindVertexArray(0);
            BindTexture(TEXTURE_2D, 0);
        }
    }
}
