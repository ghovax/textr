use clap::Parser;
use glium::{
    glutin::{
        self, dpi::PhysicalSize, event_loop::EventLoop, window::WindowBuilder, ContextBuilder,
    },
    implement_vertex,
    uniforms::UniformsStorage,
    Program, Surface as _,
};
use glutin::event::{Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use itertools::Itertools as _;
use rusttype::{gpu_cache::Cache, Point};
use rusttype::{point, vector, Font, PositionedGlyph, Rect, Scale};
use std::collections::HashMap;
use std::env;
use std::{borrow::Cow, path::PathBuf};
use unicode_normalization::UnicodeNormalization as _;

const BORDER_MARGIN: f32 = 20.0;
const HEADING_SEPARATION: f32 = 57.0;

fn layout_heading<'a>(
    font_styles_map: &HashMap<String, FontStyles<'a>>,
    text_element: &TextElement,
    scale_factor: f32,
    caret: &mut Point<f32>,
) -> Vec<PositionedGlyph<'a>> {
    layout_paragraph(font_styles_map, &vec![text_element.clone()], scale_factor, caret)
}

fn layout_paragraph<'a>(
    font_styles_map: &HashMap<String, FontStyles<'a>>,
    text_elements: &Vec<TextElement>,
    scale_factor: f32,
    caret: &mut Point<f32>,
) -> Vec<PositionedGlyph<'a>> {
    let mut positioned_glyphs = Vec::new();

    let max_vertical_ascent = *text_elements
        .iter()
        .map(|text_element| {
            let font_style = font_styles_map.get(&text_element.language).unwrap();
            let font = match text_element.style.font_style.as_str() {
                "bold" => font_style.bold_font.as_ref().unwrap(),
                "italic" => font_style.italic_font.as_ref().unwrap(),
                "normal" => &font_style.normal_font,
                _ => panic!("unable to determine the font style"),
            };
            let scale = Scale::uniform(text_element.style.font_size as f32 * scale_factor);

            let vertical_metrics = font.v_metrics(scale);
            vertical_metrics.ascent
        })
        .collect_vec()
        .iter()
        .max_by(|a, b| a.total_cmp(b))
        .unwrap();
    caret.y += max_vertical_ascent;

    for text_element in text_elements {
        let font_style = font_styles_map.get(&text_element.language).unwrap();
        let font = match text_element.style.font_style.as_str() {
            "bold" => font_style.bold_font.as_ref().unwrap(),
            "italic" => font_style.italic_font.as_ref().unwrap(),
            "normal" => &font_style.normal_font,
            _ => panic!("unable to determine the font style"),
        };
        let scale = Scale::uniform(text_element.style.font_size as f32 * scale_factor);

        let vertical_metrics = font.v_metrics(scale);
        let advance_height =
            vertical_metrics.ascent - vertical_metrics.descent + vertical_metrics.line_gap;

        let mut last_glyph_id = None;

        for character in text_element.text.chars().nfc() {
            if character.is_control() {
                match character {
                    '\r' | '\n' => {
                        *caret = point(BORDER_MARGIN, caret.y + advance_height);
                    }
                    _ => (),
                }
                continue;
            }
            let base_glyph = font.glyph(character);
            if let Some(id) = last_glyph_id.take() {
                caret.x += font.pair_kerning(scale, id, base_glyph.id());
            }
            last_glyph_id = Some(base_glyph.id());
            let glyph = base_glyph.scaled(scale).positioned(*caret);

            caret.x += glyph.unpositioned().h_metrics().advance_width;
            positioned_glyphs.push(glyph);
        }
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

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct Document {
    charset: String,
    root: Vec<Content>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum Content {
    Paragraph { contents: Vec<TextElement> },
    Heading { content: TextElement },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct TextElement {
    pub style: Style,
    #[serde(rename = "lang")]
    pub language: String,
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Style {
    pub color: String,
    pub font_style: String,
    pub font_size: u32,
}

struct FontStyles<'a> {
    normal_font: Font<'a>,
    italic_font: Option<Font<'a>>,
    bold_font: Option<Font<'a>>,
}

#[derive(Parser)]
#[command(version, long_about = None)]
struct CliArguments {
    #[arg(short = 'd', long = "document-path", help = "")]
    document_path: Option<PathBuf>,
    #[arg(long = "test-flag", help = "")]
    test_flag: Option<String>,
}

fn main() {
    if cfg!(target_os = "linux") && env::var("WINIT_UNIX_BACKEND").is_err() {
        env::set_var("WINIT_UNIX_BACKEND", "x11");
    }
    env_logger::init();

    let mut font_styles_map: HashMap<String, FontStyles> = HashMap::new();

    let english_font = FontStyles {
        normal_font: Font::try_from_bytes(include_bytes!(
            "../fonts/Noto_Sans/NotoSans-Regular.ttf"
        ))
        .unwrap(),
        italic_font: Some(
            Font::try_from_bytes(include_bytes!("../fonts/Noto_Sans/NotoSans-Italic.ttf")).unwrap(),
        ),
        bold_font: Some(
            Font::try_from_bytes(include_bytes!("../fonts/Noto_Sans/NotoSans-Bold.ttf")).unwrap(),
        ),
    };
    font_styles_map.insert("en-US".to_string(), english_font);

    let japanese_font = FontStyles {
        normal_font: Font::try_from_bytes(include_bytes!(
            "../fonts/Noto_Sans_JP/NotoSansJP-Regular.ttf"
        ))
        .unwrap(),
        italic_font: None,
        bold_font: Some(
            Font::try_from_bytes(include_bytes!("../fonts/Noto_Sans_JP/NotoSansJP-Bold.ttf"))
                .unwrap(),
        ),
    };
    font_styles_map.insert("ja-JP".to_string(), japanese_font);

    let simplified_chinese_font = FontStyles {
        normal_font: Font::try_from_bytes(include_bytes!(
            "../fonts/Noto_Sans_SC/NotoSansSC-Regular.ttf"
        ))
        .unwrap(),
        italic_font: None,
        bold_font: Some(
            Font::try_from_bytes(include_bytes!("../fonts/Noto_Sans_SC/NotoSansSC-Bold.ttf"))
                .unwrap(),
        ),
    };
    font_styles_map.insert("zh-CN".to_string(), simplified_chinese_font);

    let window = WindowBuilder::new().with_inner_size(PhysicalSize::new(1600, 900));
    let context = ContextBuilder::new().with_vsync(true);
    let event_loop = EventLoop::new();
    let display = glium::Display::new(window, context, &event_loop).unwrap();

    let program = Program::from_source(
        &display,
        include_str!("glyphVertexShader.glsl"),
        include_str!("glyphFragmentShader.glsl"),
        None,
    )
    .unwrap();

    let scale_factor = display.gl_window().window().scale_factor() as f32;
    let (cache_width, cache_height) =
        ((1600.0 * scale_factor) as u32, (900.0 * scale_factor) as u32);
    let mut cache: Cache<'static> = Cache::builder().dimensions(cache_width, cache_height).build();

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

    let CliArguments { document_path, test_flag } = CliArguments::parse();

    if let Some(test_flag) = test_flag {
        let documents: Vec<_> = if let Ok(asset_files) = std::fs::read_dir("assets") {
            asset_files
                .filter_map(|entry| {
                    entry.ok().and_then(|asset_file| {
                        let asset_file_path = asset_file.path();
                        if asset_file_path.is_file()
                            && asset_file_path
                                .extension()
                                .map_or(false, |extension| extension == "json")
                        {
                            let document_content =
                                std::fs::read_to_string(&asset_file_path).unwrap();
                            let document: Document =
                                serde_json::from_str(&document_content).unwrap();
                            Some((document, asset_file_path))
                        } else {
                            None
                        }
                    })
                })
                .collect()
        } else {
            panic!("failed to read assets directory");
        };

        for (document, document_path) in documents.iter() {
            let scale_factor = display.gl_window().window().scale_factor() as f32;
            let mut glyphs = Vec::new();

            let mut caret = point(BORDER_MARGIN, BORDER_MARGIN);
            for content in document.root.iter() {
                let positioned_glyphs = match content {
                    Content::Heading { content: text_element } => {
                        let glyphs = layout_heading(
                            &font_styles_map,
                            text_element,
                            scale_factor,
                            &mut caret,
                        );
                        caret.y += HEADING_SEPARATION;
                        glyphs
                    }
                    Content::Paragraph { contents: text_elements } => {
                        layout_paragraph(&font_styles_map, text_elements, scale_factor, &mut caret)
                    }
                };
                caret.x = BORDER_MARGIN;

                for glyph in &positioned_glyphs {
                    cache.queue_glyph(0, glyph.clone());
                }
                glyphs.extend(positioned_glyphs);
            }
            cache
                .cache_queued(|rectangle, data| {
                    cache_texture.main_level().write(
                        glium::Rect {
                            left: rectangle.min.x,
                            bottom: rectangle.min.y,
                            width: rectangle.width(),
                            height: rectangle.height(),
                        },
                        glium::texture::RawImage2d {
                            data: Cow::Borrowed(data),
                            width: rectangle.width(),
                            height: rectangle.height(),
                            format: glium::texture::ClientFormat::U8,
                        },
                    );
                })
                .unwrap();

            let uniforms = UniformsStorage::new(
                "texture_sampler",
                cache_texture
                    .sampled()
                    .magnify_filter(glium::uniforms::MagnifySamplerFilter::Nearest),
            );

            let color = [0.0, 0.0, 0.0, 1.0];
            let (screen_width, screen_height) = {
                let (width, height) = display.get_framebuffer_dimensions();
                (width as f32, height as f32)
            };
            let origin = point(0.0, 0.0);
            let vertices: Vec<Vertex> = glyphs
                .iter()
                .filter_map(|glyph| cache.rect_for(0, glyph).ok().flatten())
                .flat_map(|(texture, screen)| {
                    let glyph_rectangle = Rect {
                        min: origin
                            + (vector(
                                screen.min.x as f32 / screen_width - 0.5,
                                1.0 - screen.min.y as f32 / screen_height - 0.5,
                            )) * 2.0,
                        max: origin
                            + (vector(
                                screen.max.x as f32 / screen_width - 0.5,
                                1.0 - screen.max.y as f32 / screen_height - 0.5,
                            )) * 2.0,
                    };
                    vec![
                        Vertex {
                            position: [glyph_rectangle.min.x, glyph_rectangle.max.y],
                            texture_coordinates: [texture.min.x, texture.max.y],
                            color,
                        },
                        Vertex {
                            position: [glyph_rectangle.min.x, glyph_rectangle.min.y],
                            texture_coordinates: [texture.min.x, texture.min.y],
                            color,
                        },
                        Vertex {
                            position: [glyph_rectangle.max.x, glyph_rectangle.min.y],
                            texture_coordinates: [texture.max.x, texture.min.y],
                            color,
                        },
                        Vertex {
                            position: [glyph_rectangle.max.x, glyph_rectangle.min.y],
                            texture_coordinates: [texture.max.x, texture.min.y],
                            color,
                        },
                        Vertex {
                            position: [glyph_rectangle.max.x, glyph_rectangle.max.y],
                            texture_coordinates: [texture.max.x, texture.max.y],
                            color,
                        },
                        Vertex {
                            position: [glyph_rectangle.min.x, glyph_rectangle.max.y],
                            texture_coordinates: [texture.min.x, texture.max.y],
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
                        backface_culling: glium::BackfaceCullingMode::CullCounterClockwise,
                        ..Default::default()
                    },
                )
                .unwrap();

            target.finish().unwrap();

            let front_buffer: glium::texture::RawImage2d<'_, u8> =
                display.read_front_buffer().unwrap();
            let test_image_buffer = image::ImageBuffer::from_raw(
                front_buffer.width,
                front_buffer.height,
                front_buffer.data.into_owned(),
            )
            .unwrap();
            let test_image =
                image::DynamicImage::ImageRgba8(test_image_buffer).flipv().into_rgba8();

            match test_flag.as_str() {
                "generate" => {
                    let test_image_path = format!(
                        "reference_images/{}.png",
                        document_path.file_stem().unwrap().to_str().unwrap()
                    );
                    test_image.save(test_image_path).unwrap();
                }
                "test" => {
                    let reference_image = image::open(format!(
                        "reference_images/{}.png",
                        document_path.file_stem().unwrap().to_str().unwrap()
                    ))
                    .unwrap()
                    .into_rgba8();
                    let comparison_results =
                        image_compare::rgba_hybrid_compare(&test_image, &reference_image).unwrap();
                    assert!(
                        comparison_results.score > 0.99999,
                        "comparison failed for the document {:?} with the similarity score of {}%",
                        document_path.file_stem().unwrap(),
                        comparison_results.score * 100.0
                    );
                }
                _ => panic!("unknown test flag"),
            }
        }
    } else {
        let document_content = std::fs::read_to_string(document_path.as_ref().unwrap()).unwrap();
        let document: Document = serde_json::from_str(&document_content).unwrap();

        let window_title = format!("{}", document_path.unwrap().display());

        event_loop.run(move |event, _, control_flow| {
            control_flow.set_wait();

            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::KeyboardInput {
                        input: KeyboardInput { virtual_keycode: Some(VirtualKeyCode::Escape), .. },
                        ..
                    }
                    | WindowEvent::CloseRequested => control_flow.set_exit(),
                    WindowEvent::ReceivedCharacter(character) => {
                        match character {
                            '\u{8}' => {
                                // TODO
                            }
                            _ if character != '\u{7f}' => {
                                // TODO
                            }
                            _ => (),
                        }
                        display.gl_window().window().request_redraw();
                    }
                    _ => (),
                },
                Event::RedrawRequested(_) => {
                    let scale_factor = display.gl_window().window().scale_factor() as f32;
                    let mut glyphs = Vec::new();

                    let mut caret = point(BORDER_MARGIN, BORDER_MARGIN);
                    for content in document.root.iter() {
                        let positioned_glyphs = match content {
                            Content::Heading { content: text_element } => {
                                let glyphs = layout_heading(
                                    &font_styles_map,
                                    text_element,
                                    scale_factor,
                                    &mut caret,
                                );
                                caret.y += HEADING_SEPARATION;
                                glyphs
                            }
                            Content::Paragraph { contents: text_elements } => layout_paragraph(
                                &font_styles_map,
                                text_elements,
                                scale_factor,
                                &mut caret,
                            ),
                        };
                        caret.x = BORDER_MARGIN;

                        for glyph in &positioned_glyphs {
                            cache.queue_glyph(0, glyph.clone());
                        }
                        glyphs.extend(positioned_glyphs);
                    }
                    cache
                        .cache_queued(|rectangle, data| {
                            cache_texture.main_level().write(
                                glium::Rect {
                                    left: rectangle.min.x,
                                    bottom: rectangle.min.y,
                                    width: rectangle.width(),
                                    height: rectangle.height(),
                                },
                                glium::texture::RawImage2d {
                                    data: Cow::Borrowed(data),
                                    width: rectangle.width(),
                                    height: rectangle.height(),
                                    format: glium::texture::ClientFormat::U8,
                                },
                            );
                        })
                        .unwrap();

                    let uniforms = UniformsStorage::new(
                        "texture_sampler",
                        cache_texture
                            .sampled()
                            .magnify_filter(glium::uniforms::MagnifySamplerFilter::Nearest),
                    );

                    let color = [0.0, 0.0, 0.0, 1.0];
                    let (screen_width, screen_height) = {
                        let (width, height) = display.get_framebuffer_dimensions();
                        (width as f32, height as f32)
                    };
                    let origin = point(0.0, 0.0);
                    let vertices: Vec<Vertex> = glyphs
                        .iter()
                        .filter_map(|glyph| cache.rect_for(0, glyph).ok().flatten())
                        .flat_map(|(texture, screen)| {
                            let glyph_rectangle = Rect {
                                min: origin
                                    + (vector(
                                        screen.min.x as f32 / screen_width - 0.5,
                                        1.0 - screen.min.y as f32 / screen_height - 0.5,
                                    )) * 2.0,
                                max: origin
                                    + (vector(
                                        screen.max.x as f32 / screen_width - 0.5,
                                        1.0 - screen.max.y as f32 / screen_height - 0.5,
                                    )) * 2.0,
                            };
                            vec![
                                Vertex {
                                    position: [glyph_rectangle.min.x, glyph_rectangle.max.y],
                                    texture_coordinates: [texture.min.x, texture.max.y],
                                    color,
                                },
                                Vertex {
                                    position: [glyph_rectangle.min.x, glyph_rectangle.min.y],
                                    texture_coordinates: [texture.min.x, texture.min.y],
                                    color,
                                },
                                Vertex {
                                    position: [glyph_rectangle.max.x, glyph_rectangle.min.y],
                                    texture_coordinates: [texture.max.x, texture.min.y],
                                    color,
                                },
                                Vertex {
                                    position: [glyph_rectangle.max.x, glyph_rectangle.min.y],
                                    texture_coordinates: [texture.max.x, texture.min.y],
                                    color,
                                },
                                Vertex {
                                    position: [glyph_rectangle.max.x, glyph_rectangle.max.y],
                                    texture_coordinates: [texture.max.x, texture.max.y],
                                    color,
                                },
                                Vertex {
                                    position: [glyph_rectangle.min.x, glyph_rectangle.max.y],
                                    texture_coordinates: [texture.min.x, texture.max.y],
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
                                backface_culling: glium::BackfaceCullingMode::CullCounterClockwise,
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
}
