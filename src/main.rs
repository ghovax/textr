use glium::{
    glutin::{
        self, dpi::PhysicalSize, event_loop::EventLoop, window::WindowBuilder, ContextBuilder,
    },
    implement_vertex, program, uniform, Surface as _,
};
use glutin::{
    event::{Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::ControlFlow,
};
use rusttype::gpu_cache::Cache;
use rusttype::{point, vector, Font, PositionedGlyph, Rect, Scale};
use std::borrow::Cow;
use std::env;
use std::error::Error;

fn layout_paragraph<'a>(
    font: &Font<'a>,
    scale: Scale,
    width: u32,
    text: &str,
) -> Vec<PositionedGlyph<'a>> {
    let mut positioned_glyphs = Vec::new();
    let vertical_metrics = font.v_metrics(scale);
    let advance_height =
        vertical_metrics.ascent - vertical_metrics.descent + vertical_metrics.line_gap;
    let mut caret = point(0.0, vertical_metrics.ascent);
    let mut last_glyph_id = None;
    for character in text.chars() {
        if character.is_control() {
            match character {
                '\r' => {
                    caret = point(0.0, caret.y + advance_height);
                }
                '\n' => {}
                _ => {}
            }
            continue;
        }
        let base_glyph = font.glyph(character);
        if let Some(id) = last_glyph_id.take() {
            caret.x += font.pair_kerning(scale, id, base_glyph.id());
        }
        last_glyph_id = Some(base_glyph.id());
        let mut glyph = base_glyph.scaled(scale).positioned(caret);
        if let Some(bounding_box) = glyph.pixel_bounding_box() {
            if bounding_box.max.x > width as i32 {
                caret = point(0.0, caret.y + advance_height);
                glyph.set_position(caret);
                last_glyph_id = None;
            }
        }
        caret.x += glyph.unpositioned().h_metrics().advance_width;
        positioned_glyphs.push(glyph);
    }
    positioned_glyphs
}

#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 2],
    texture_coordinates: [f32; 2],
    color: [f32; 4],
}

implement_vertex!(Vertex, position, texture_coordinates, color);

fn main() {
    if cfg!(target_os = "linux") && env::var("WINIT_UNIX_BACKEND").is_err() {
        env::set_var("WINIT_UNIX_BACKEND", "x11");
    }

    let font_data = std::fs::read("fonts/WenQuanYiMicroHei.ttf").unwrap();
    let font = Font::try_from_vec(font_data).unwrap();

    let window = WindowBuilder::new()
        .with_inner_size(PhysicalSize::new(512, 512))
        .with_title("RustType GPU cache example");
    let context = ContextBuilder::new().with_vsync(true);
    let event_loop = EventLoop::new();
    let display = glium::Display::new(window, context, &event_loop).unwrap();

    let scale = display.gl_window().window().scale_factor();

    let (cache_width, cache_height) = ((512.0 * scale) as u32, (512.0 * scale) as u32);
    let mut cache: Cache<'static> = Cache::builder()
        .dimensions(cache_width, cache_height)
        .build();

    let program = program!(
    &display,
    140 => {
            vertex: r#"
#version 140

in vec2 position;
in vec2 texture_coordinates;
in vec4 color;

out vec2 vertex_texture_coordinates;
out vec4 vertex_color;

void main() {
    gl_Position = vec4(position, 0.0, 1.0);
    vertex_texture_coordinates = texture_coordinates;
    vertex_color = color;
}
            "#,

            fragment: r#"
#version 140

uniform sampler2D texture_sampler;

in vec2 vertex_texture_coordinates;
in vec4 vertex_color;

out vec4 fragment_color;

void main() {
    fragment_color = vertex_color * vec4(1.0, 1.0, 1.0, texture(texture_sampler, vertex_texture_coordinates).r);
}
            "#
    }).unwrap();

    let cache_texture = glium::texture::Texture2d::with_format(
        &display,
        glium::texture::RawImage2d {
            data: Cow::Owned(vec![128u8; cache_width as usize * cache_height as usize]),
            width: cache_width,
            height: cache_height,
            format: glium::texture::ClientFormat::U8,
        },
        glium::texture::UncompressedFloatFormat::U8,
        glium::texture::MipmapsOption::NoMipmap,
    )
    .unwrap();
    let mut text: String = "A japanese poem:\r
\r
色は匂へど散りぬるを我が世誰ぞ常ならむ有為の奥山今日越えて浅き夢見じ酔ひもせず\r
\r
Feel free to type out some text, and delete it with Backspace. \
You can also try resizing this window."
        .into();

    event_loop.run(move |event, _, control_flow| {
        control_flow.set_wait();

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            virtual_keycode: Some(VirtualKeyCode::Escape),
                            ..
                        },
                    ..
                }
                | WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::ReceivedCharacter(character) => {
                    match character {
                        '\u{8}' => {
                            text.pop();
                        }
                        _ if character != '\u{7f}' => text.push(character),
                        _ => {}
                    }
                    display.gl_window().window().request_redraw();
                }
                _ => (),
            },
            Event::RedrawRequested(_) => {
                let scale = display.gl_window().window().scale_factor();
                let (width, _): (u32, _) = display.gl_window().window().inner_size().into();
                let scale = scale as f32;

                let glyphs = layout_paragraph(&font, Scale::uniform(24.0 * scale), width, &text);
                for glyph in &glyphs {
                    cache.queue_glyph(0, glyph.clone());
                }
                cache
                    .cache_queued(|rect, data| {
                        cache_texture.main_level().write(
                            glium::Rect {
                                left: rect.min.x,
                                bottom: rect.min.y,
                                width: rect.width(),
                                height: rect.height(),
                            },
                            glium::texture::RawImage2d {
                                data: Cow::Borrowed(data),
                                width: rect.width(),
                                height: rect.height(),
                                format: glium::texture::ClientFormat::U8,
                            },
                        );
                    })
                    .unwrap();

                let uniforms = uniform! {
                    texture_sampler: cache_texture.sampled().magnify_filter(glium::uniforms::MagnifySamplerFilter::Nearest)
                };

                let color = [0.0, 0.0, 0.0, 1.0];
                let (screen_width, screen_height) = {
                    let (w, h) = display.get_framebuffer_dimensions();
                    (w as f32, h as f32)
                };
                let origin = point(0.0, 0.0);
                let vertices: Vec<Vertex> = glyphs
                    .iter()
                    .filter_map(|glyph| cache.rect_for(0, glyph).ok().flatten())
                    .flat_map(|(texture_rectangle, screen_rectangle)| {
                        let glyph_rectangle = Rect {
                            min: origin
                                + (vector(
                                    screen_rectangle.min.x as f32 / screen_width - 0.5,
                                    1.0 - screen_rectangle.min.y as f32 / screen_height - 0.5,
                                )) * 2.0,
                            max: origin
                                + (vector(
                                    screen_rectangle.max.x as f32 / screen_width - 0.5,
                                    1.0 - screen_rectangle.max.y as f32 / screen_height - 0.5,
                                )) * 2.0,
                        };
                        vec![
                            Vertex {
                                position: [glyph_rectangle.min.x, glyph_rectangle.max.y],
                                texture_coordinates: [
                                    texture_rectangle.min.x,
                                    texture_rectangle.max.y,
                                ],
                                color,
                            },
                            Vertex {
                                position: [glyph_rectangle.min.x, glyph_rectangle.min.y],
                                texture_coordinates: [
                                    texture_rectangle.min.x,
                                    texture_rectangle.min.y,
                                ],
                                color,
                            },
                            Vertex {
                                position: [glyph_rectangle.max.x, glyph_rectangle.min.y],
                                texture_coordinates: [
                                    texture_rectangle.max.x,
                                    texture_rectangle.min.y,
                                ],
                                color,
                            },
                            Vertex {
                                position: [glyph_rectangle.max.x, glyph_rectangle.min.y],
                                texture_coordinates: [
                                    texture_rectangle.max.x,
                                    texture_rectangle.min.y,
                                ],
                                color,
                            },
                            Vertex {
                                position: [glyph_rectangle.max.x, glyph_rectangle.max.y],
                                texture_coordinates: [
                                    texture_rectangle.max.x,
                                    texture_rectangle.max.y,
                                ],
                                color,
                            },
                            Vertex {
                                position: [glyph_rectangle.min.x, glyph_rectangle.max.y],
                                texture_coordinates: [
                                    texture_rectangle.min.x,
                                    texture_rectangle.max.y,
                                ],
                                color,
                            },
                        ]
                    })
                    .collect();

                let vertex_buffer = glium::VertexBuffer::new(&display, &vertices).unwrap();

                let mut target = display.draw();
                target.clear_color(1.0, 1.0, 1.0, 0.0);
                target
                    .draw(
                        &vertex_buffer,
                        glium::index::NoIndices(glium::index::PrimitiveType::TrianglesList),
                        &program,
                        &uniforms,
                        &glium::DrawParameters {
                            blend: glium::Blend::alpha_blending(),
                            ..Default::default()
                        },
                    )
                    .unwrap();

                target.finish().unwrap();
            }
            _ => (),
        }
    });
}
