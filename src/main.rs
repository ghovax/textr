#![warn(clippy::unwrap_used)]

use clap::Parser;
use glium::{
    glutin::{self, dpi::PhysicalSize, event_loop::EventLoop, window::WindowBuilder, ContextBuilder},
    implement_vertex,
    uniforms::UniformsStorage,
    Program, Surface as _,
};
use glutin::event::{Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use itertools::Itertools as _;
use rusttype::{gpu_cache::Cache, Point};
use rusttype::{point, vector, Font, PositionedGlyph, Rect, Scale};
use std::env;
use std::{borrow::Cow, path::PathBuf};
use std::{collections::HashMap, sync::mpsc};
use unicode_normalization::UnicodeNormalization as _;

const BORDER_MARGIN: f32 = 20.0;
const HEADING_SEPARATION: f32 = 57.0;

fn layout_heading<'a>(
    font_styles_map: &HashMap<String, FontStyles<'a>>,
    text_element: &TextElement,
    scale_factor: f32,
    caret: &mut Point<f32>,
) -> Result<Vec<PositionedGlyph<'a>>> {
    layout_paragraph(font_styles_map, &vec![text_element.clone()], scale_factor, caret)
}

fn layout_paragraph<'a>(
    font_styles_map: &HashMap<String, FontStyles<'a>>,
    text_elements: &Vec<TextElement>,
    scale_factor: f32,
    caret: &mut Point<f32>,
) -> Result<Vec<PositionedGlyph<'a>>> {
    let mut positioned_glyphs = Vec::new();

    let max_vertical_ascent = *text_elements
        .iter()
        .filter_map(|text_element| {
            let font_style = match font_styles_map.get(&text_element.language) {
                Some(font_style) => font_style,
                None => {
                    log::error!(
                        "Unable to find the font style for the language {}",
                        text_element.language
                    );
                    return None;
                }
            };
            let font = match text_element.style.font_style.as_str() {
                "bold" => match font_style.bold_font.as_ref() {
                    Some(bold_font) => bold_font,
                    None => {
                        log::error!(
                            "Unable to find the bold font for the language {}",
                            text_element.language
                        );
                        return None;
                    }
                },
                "italic" => match font_style.italic_font.as_ref() {
                    Some(italic_font) => italic_font,
                    None => {
                        log::error!(
                            "Unable to find the italic font for the language {}",
                            text_element.language
                        );
                        return None;
                    }
                },
                "normal" => &font_style.normal_font,
                font_style => {
                    log::error!("Unable to find the font style: {}", font_style);
                    return None;
                }
            };
            let scale = Scale::uniform(text_element.style.font_size as f32 * scale_factor);

            let vertical_metrics = font.v_metrics(scale);
            Some(vertical_metrics.ascent)
        })
        .collect_vec()
        .iter()
        .max_by(|a, b| a.total_cmp(b))
        .ok_or(anyhow::anyhow!("Unable to determine the maximum ascent"))?;
    caret.y += max_vertical_ascent;

    for text_element in text_elements {
        let font_style = match font_styles_map.get(&text_element.language) {
            Some(font_style) => font_style,
            None => {
                return Err(anyhow::anyhow!(
                    "Unable to find the font style for the language {}",
                    text_element.language
                ));
            }
        };
        let font = match text_element.style.font_style.as_str() {
            "bold" => match font_style.bold_font.as_ref() {
                Some(bold_font) => bold_font,
                None => {
                    return Err(anyhow::anyhow!(
                        "Unable to find the bold font for the language {}",
                        text_element.language
                    ));
                }
            },
            "italic" => match font_style.italic_font.as_ref() {
                Some(italic_font) => italic_font,
                None => {
                    return Err(anyhow::anyhow!(
                        "Unable to find the italic font for the language {}",
                        text_element.language
                    ));
                }
            },
            "normal" => &font_style.normal_font,
            font_style => {
                return Err(anyhow::anyhow!("Unable to find the font style: {}", font_style));
            }
        };
        let scale = Scale::uniform(text_element.style.font_size as f32 * scale_factor);

        let vertical_metrics = font.v_metrics(scale);
        let advance_height = vertical_metrics.ascent - vertical_metrics.descent + vertical_metrics.line_gap;

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

    Ok(positioned_glyphs)
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
    #[arg(long = "document-path", help = "Path to the document file in the JSON format")]
    document_path: Option<PathBuf>,
    #[arg(
        long = "test-flag",
        help = "Test flag used to select the test type, either generating the reference images or comparing with them",
        value_enum
    )]
    test_flag: Option<TestFlag>,
    #[arg(long = "window-width", help = "Width of the window")]
    window_width: u32,
    #[arg(long = "window-height", help = "Height of the window")]
    window_height: u32,
}

#[derive(Debug, Copy, Clone, clap::ValueEnum)]
enum TestFlag {
    GenerateReferenceImages,
    CompareWithReferenceImages,
}

const SIMILARITY_THRESHOLD: f64 = 1.0 - 1.0e-6;

fn main() {
    if let Err(error) = fallible_main() {
        log::error!("{}", error);
    }
}

use anyhow::Result;

fn fallible_main() -> Result<()> {
    if cfg!(target_os = "linux") && env::var("WINIT_UNIX_BACKEND").is_err() {
        env::set_var("WINIT_UNIX_BACKEND", "x11");
    }
    env_logger::init();

    let mut font_styles_map: HashMap<String, FontStyles> = HashMap::new();

    let english_font = FontStyles {
        normal_font: Font::try_from_bytes(include_bytes!("../fonts/Noto_Sans/NotoSans-Regular.ttf"))
            .ok_or(anyhow::anyhow!("Unable to load the normal english font"))?,
        italic_font: Some(
            Font::try_from_bytes(include_bytes!("../fonts/Noto_Sans/NotoSans-Italic.ttf"))
                .ok_or(anyhow::anyhow!("Unable to load the italic english font"))?,
        ),
        bold_font: Some(
            Font::try_from_bytes(include_bytes!("../fonts/Noto_Sans/NotoSans-Bold.ttf"))
                .ok_or(anyhow::anyhow!("Unable to load the bold english font"))?,
        ),
    };
    font_styles_map.insert("en-US".to_string(), english_font);

    let japanese_font = FontStyles {
        normal_font: Font::try_from_bytes(include_bytes!("../fonts/Noto_Sans_JP/NotoSansJP-Regular.ttf"))
            .ok_or(anyhow::anyhow!("Unable to load the normal japanese font"))?,

        italic_font: None,
        bold_font: Some(
            Font::try_from_bytes(include_bytes!("../fonts/Noto_Sans_JP/NotoSansJP-Bold.ttf"))
                .ok_or(anyhow::anyhow!("Unable to load the bold japanese font"))?,
        ),
    };
    font_styles_map.insert("ja-JP".to_string(), japanese_font);

    let simplified_chinese_font = FontStyles {
        normal_font: Font::try_from_bytes(include_bytes!("../fonts/Noto_Sans_SC/NotoSansSC-Regular.ttf"))
            .ok_or(anyhow::anyhow!("Unable to load the normal simplified chinese font"))?,
        italic_font: None,
        bold_font: Some(
            Font::try_from_bytes(include_bytes!("../fonts/Noto_Sans_SC/NotoSansSC-Bold.ttf"))
                .ok_or(anyhow::anyhow!("Unable to load the bold simplified chinese font"))?,
        ),
    };
    font_styles_map.insert("zh-CN".to_string(), simplified_chinese_font);

    log::info!(
        "Initialized the program with the languages {:?} supported",
        font_styles_map.keys().collect_vec()
    );

    let CliArguments {
        document_path,
        test_flag,
        window_width,
        window_height,
    } = CliArguments::parse();

    let window = WindowBuilder::new().with_inner_size(PhysicalSize::new(window_width, window_height));
    let context = ContextBuilder::new().with_vsync(true);
    let event_loop = EventLoop::new();
    let display = glium::Display::new(window, context, &event_loop)
        .map_err(|error| anyhow::anyhow!("Unable to create the display: {}", error))?;

    let program = Program::from_source(
        &display,
        include_str!("glyphVertexShader.glsl"),
        include_str!("glyphFragmentShader.glsl"),
        None,
    )
    .map_err(|error| anyhow::anyhow!("Unable to create the program: {}", error))?;

    let scale_factor = display.gl_window().window().scale_factor() as f32;
    let (cache_width, cache_height) = (
        (window_width as f32 * scale_factor) as u32,
        (window_height as f32 * scale_factor) as u32,
    );
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
    .map_err(|error| anyhow::anyhow!("Unable to create the cache texture: {}", error))?;

    if let Some(test_flag) = test_flag {
        test_main(
            test_flag,
            &display,
            &font_styles_map,
            &mut cache,
            &cache_texture,
            &program,
        )?;
        return Ok(());
    }

    if document_path.is_none() {
        return Err(anyhow::anyhow!(
            "No document path provided, you need to provide a path to a document"
        ));
    }
    #[allow(clippy::unwrap_used)]
    let document_path = document_path.unwrap();
    let document_content = std::fs::read_to_string(&document_path)
        .map_err(|error| anyhow::anyhow!("Unable to read the document into a string: {}", error))?;
    let document: Document = serde_json::from_str(&document_content)
        .map_err(|error| anyhow::anyhow!("Unable to deserialize the document: {}", error))?;

    let window_title = format!("{}", document_path.display());
    {
        let window = display.gl_window();
        window.window().set_title(&window_title);
    }

    let (errors_sender, error_receiver) = mpsc::channel();

    event_loop.run(move |event, _, control_flow| {
        control_flow.set_wait();

        while let Ok(error) = error_receiver.try_recv() {
            match error {
                EventLoopError::DrawGlyphsError(error) => {
                    log::error!("{}", error);
                    control_flow.set_exit_with_code(1);
                }
            }
        }

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
                if let Err(error) = draw_glyphs(
                    &display,
                    &document,
                    &font_styles_map,
                    &mut cache,
                    &cache_texture,
                    &program,
                ) {
                    #[allow(clippy::unwrap_used)]
                    errors_sender.send(EventLoopError::DrawGlyphsError(error)).unwrap();
                }
            }

            _ => (),
        }
    });
}

enum EventLoopError {
    DrawGlyphsError(anyhow::Error),
}

fn test_main(
    test_flag: TestFlag,
    display: &glium::Display,
    font_styles_map: &HashMap<String, FontStyles<'static>>,
    cache: &mut Cache<'_>,
    cache_texture: &glium::Texture2d,
    program: &Program,
) -> Result<()> {
    log::info!("Executing with the test flag: {:?}", test_flag);
    let documents = if let Ok(document_files) = std::fs::read_dir("assets") {
        document_files
            .filter_map(|document_entry| {
                document_entry.ok().and_then(|document_file| {
                    let document_file_path = document_file.path();

                    if document_file_path.is_file()
                        && document_file_path
                            .extension()
                            .map_or(false, |extension| extension == "json")
                    {
                        let document_content = match std::fs::read_to_string(&document_file_path) {
                            Ok(content) => content,
                            Err(error) => {
                                log::error!(
                                    "Unable to read the document {}: {}",
                                    document_file_path.display(),
                                    error
                                );
                                return None;
                            }
                        };
                        let document: Document = match serde_json::from_str(&document_content) {
                            Ok(document) => document,
                            Err(error) => {
                                log::error!(
                                    "Unable to parse the document {:?}: {}",
                                    document_file_path.display(),
                                    error
                                );
                                return None;
                            }
                        };

                        Some((document, document_file_path))
                    } else {
                        None
                    }
                })
            })
            .collect_vec()
    } else {
        return Err(anyhow::anyhow!("Unable to read the documents directory"));
    };
    log::info!("Found {} documents to test", documents.len(),);

    let mut similarity_scores = Vec::new();
    for (document, document_path) in documents.iter() {
        draw_glyphs(display, document, font_styles_map, cache, cache_texture, program)?;

        let front_buffer: glium::texture::RawImage2d<'_, u8> = display
            .read_front_buffer()
            .map_err(|error| anyhow::anyhow!("Unable to read the front buffer: {}", error))?;
        let test_image_buffer =
            image::ImageBuffer::from_raw(front_buffer.width, front_buffer.height, front_buffer.data.into_owned())
                .ok_or(anyhow::anyhow!("Unable to create the test image buffer"))?;
        let test_image = image::DynamicImage::ImageRgba8(test_image_buffer).flipv().into_rgba8();

        let document_file_name = document_path
            .file_stem()
            .ok_or(anyhow::anyhow!(
                "Unable to get the file name for the document {:?}",
                document_path.display()
            ))?
            .to_str()
            .ok_or(anyhow::anyhow!(
                "Unable to convert the file name of the document {:?} to a string compatible with UTF-8",
                document_path.display()
            ))?;
        let image_path = format!("reference_images/{}.png", document_file_name);

        match test_flag {
            TestFlag::GenerateReferenceImages => {
                test_image
                    .save(image_path)
                    .map_err(|error| anyhow::anyhow!("Unable to save the reference image: {}", error))?;
            }
            TestFlag::CompareWithReferenceImages => {
                let reference_image = image::open(image_path)
                    .map_err(|error| {
                        anyhow::anyhow!(
                            "Unable to open the reference image for the document {:?}: {}",
                            document_path.display(),
                            error
                        )
                    })?
                    .into_rgba8();

                let comparison_results = image_compare::rgba_hybrid_compare(&test_image, &reference_image)
                    .map_err(|error| anyhow::anyhow!("Unable to compare the reference image: {}", error))?;
                similarity_scores.push((document_file_name, comparison_results.score));
            }
        }
    }

    let document_file_names = documents
        .iter()
        .filter_map(|(_, document_path)| match document_path.file_stem() {
            Some(file_stem) => match file_stem.to_str() {
                Some(file_stem) => Some(file_stem.to_string()),
                None => {
                    log::error!(
                        "Unable to convert the file name of the document {:?} to a string compatible with UTF-8",
                        document_path.display()
                    );
                    None
                }
            },
            None => {
                log::error!(
                    "Unable to get the file stem for the document {:?}",
                    document_path.display()
                );
                None
            }
        })
        .collect_vec();

    match test_flag {
        TestFlag::GenerateReferenceImages => {
            log::info!("Generated reference images for the documents {:?}", document_file_names);
        }
        TestFlag::CompareWithReferenceImages => {
            let failed_tests = similarity_scores
                .iter()
                .filter(|(_, similarity_score)| *similarity_score < SIMILARITY_THRESHOLD)
                .collect_vec();
            if failed_tests.is_empty() {
                log::info!(
                    "Successfully compared the documents {:?} with the reference images",
                    document_file_names
                );
            } else {
                return Err(anyhow::anyhow!(
                    "Failed to compare the documents {:?} with the respective reference images",
                    failed_tests
                ));
            }
        }
    }

    Ok(())
}

fn draw_glyphs(
    display: &glium::Display,
    document: &Document,
    font_styles_map: &HashMap<String, FontStyles<'static>>,
    cache: &mut Cache<'_>,
    cache_texture: &glium::Texture2d,
    program: &Program,
) -> Result<()> {
    let scale_factor = display.gl_window().window().scale_factor() as f32;
    let mut glyphs = Vec::new();

    let mut caret = point(BORDER_MARGIN, BORDER_MARGIN);
    for content in document.root.iter() {
        let positioned_glyphs = match content {
            Content::Heading { content: text_element } => {
                let glyphs = layout_heading(font_styles_map, text_element, scale_factor, &mut caret)?;
                caret.y += HEADING_SEPARATION;
                glyphs
            }
            Content::Paragraph {
                contents: text_elements,
            } => layout_paragraph(font_styles_map, text_elements, scale_factor, &mut caret)?,
        };
        caret.x = BORDER_MARGIN;

        for glyph in &positioned_glyphs {
            cache.queue_glyph(0, glyph.clone());
        }
        glyphs.extend(positioned_glyphs);
    }
    #[allow(clippy::blocks_in_conditions)]
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
        .map_err(|error| anyhow::anyhow!("Unable to cache the queued glyphs: {}", error))?;

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

    let vertex_buffer = glium::VertexBuffer::new(display, &vertices)
        .map_err(|error| anyhow::anyhow!("Unable to create the vertex buffer: {}", error))?;

    let mut target = display.draw();
    target.clear_color(1.0, 1.0, 1.0, 0.0);
    target
        .draw(
            &vertex_buffer,
            glium::index::NoIndices(glium::index::PrimitiveType::TrianglesList),
            program,
            &uniforms,
            &glium::DrawParameters {
                blend: glium::Blend::alpha_blending(),
                backface_culling: glium::BackfaceCullingMode::CullCounterClockwise,
                ..Default::default()
            },
        )
        .map_err(|error| anyhow::anyhow!("Unable to draw the glyphs: {}", error))?;

    target
        .finish()
        .map_err(|error| anyhow::anyhow!("Unable to finish the drawing operation: {}", error))?;

    Ok(())
}
