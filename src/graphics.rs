use glium::{
    glutin::{dpi::PhysicalSize, event_loop::EventLoop, window::WindowBuilder, ContextBuilder},
    implement_vertex,
    uniforms::UniformsStorage,
    Program, Surface as _,
};
use itertools::Itertools as _;
use rusttype::{gpu_cache::Cache, point, vector, Rect};
use std::{borrow::Cow, collections::HashMap, path::PathBuf};

use crate::{
    configuration_format::Configuration,
    custom_error::CustomError,
    document_format::{Content, Document},
    layouting::{layout_heading, layout_paragraph, FontStyles, BORDER_MARGIN, HEADING_SEPARATION},
    TestFlag,
};

#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 2],
    texture_coordinates: [f32; 2],
    color: [f32; 4],
}

implement_vertex!(Vertex, position, texture_coordinates, color);

pub struct GraphicsHandle {
    display: glium::Display,
    program: Program,
    cache: Cache<'static>,
    cache_texture: glium::texture::Texture2d,
}

const SIMILARITY_THRESHOLD: f64 = 1.0 - 1.0e-6;

impl GraphicsHandle {
    pub fn new(
        event_loop: &EventLoop<()>,
        configuration: Configuration,
    ) -> Result<Self, CustomError> {
        let window = WindowBuilder::new().with_inner_size(PhysicalSize::new(
            configuration.window_width,
            configuration.window_height,
        ));
        let context = ContextBuilder::new().with_vsync(true);
        let display = glium::Display::new(window, context, event_loop).map_err(|error| {
            CustomError::with_source("Unable to create the display".into(), error.into())
        })?;

        let program = Program::from_source(
            &display,
            include_str!("glyphVertexShader.glsl"),
            include_str!("glyphFragmentShader.glsl"),
            None,
        )
        .map_err(|error| {
            CustomError::with_source("Unable to create the program".into(), error.into())
        })?;

        let scale_factor = display.gl_window().window().scale_factor() as f32;
        let (cache_width, cache_height) = (
            (configuration.window_width as f32 * scale_factor) as u32,
            (configuration.window_height as f32 * scale_factor) as u32,
        );
        let cache: Cache<'static> = Cache::builder().dimensions(cache_width, cache_height).build();

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
        .map_err(|error| {
            CustomError::with_source("Unable to create the cache texture".into(), error.into())
        })?;

        Ok(Self { display, cache, cache_texture, program })
    }

    const DOCUMENTS_DIRECTORY: &'static str = "documents";

    pub fn run_tests(
        &mut self,
        test_flag: TestFlag,
        font_styles_map: HashMap<String, FontStyles<'static>>,
    ) -> Result<(), CustomError> {
        log::info!("Executing with the test flag: {:?}", test_flag);

        let documents = if let Ok(document_files) = std::fs::read_dir(Self::DOCUMENTS_DIRECTORY) {
            document_files
                .filter_map(|document_entry| {
                    document_entry.ok().and_then(|document_file| {
                        let document_file_path = document_file.path();

                        if document_file_path.is_file()
                            && document_file_path
                                .extension()
                                .map_or(false, |extension| extension == "json")
                        {
                            let document_content =
                                match std::fs::read_to_string(&document_file_path) {
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
            return Err(CustomError::with_context("Unable to read the documents directory".into()));
        };
        log::info!("Found {} documents to test", documents.len());

        let mut similarity_scores = Vec::new();
        for (document, document_path) in documents.iter() {
            self.draw_glyphs(document, &font_styles_map)?;

            let front_buffer: glium::texture::RawImage2d<'_, u8> =
                self.display.read_front_buffer().map_err(|error| {
                    CustomError::with_source("Unable to read the front buffer".into(), error.into())
                })?;
            let test_image_buffer = image::ImageBuffer::from_raw(
                front_buffer.width,
                front_buffer.height,
                front_buffer.data.into_owned(),
            )
            .ok_or(CustomError::with_context(
                "Unable to create the image buffer from the front buffer".into(),
            ))?;
            let test_image =
                image::DynamicImage::ImageRgba8(test_image_buffer).flipv().into_rgba8();

            let document_file_name = document_path
                .file_stem()
                .ok_or(CustomError::with_context("Unable to get the file stem for the document".into()))?
                .to_str()
                .ok_or(CustomError::with_context(format!(
                    "Unable to convert the file name of the document {:?} to a string compatible with UTF-8",
                    document_path.display()
                )))?;
            let image_path = format!("reference_images/{}.png", document_file_name);

            match test_flag {
                TestFlag::GenerateReferenceImages => {
                    test_image.save(image_path).map_err(|error| {
                        CustomError::with_source(
                            "Unable to save the reference image".into(),
                            error.into(),
                        )
                    })?;
                }
                TestFlag::CompareWithReferenceImages => {
                    let reference_image = image::open(image_path)
                        .map_err(|error| {
                            CustomError::with_source(
                                format!(
                                    "Unable to open the reference image for the document {}",
                                    document_file_name
                                ),
                                error.into(),
                            )
                        })?
                        .into_rgba8();

                    let comparison_results =
                        image_compare::rgba_hybrid_compare(&test_image, &reference_image).map_err(
                            |error| {
                                CustomError::with_source(
                                    "Unable to compare the images".into(),
                                    error.into(),
                                )
                            },
                        )?;
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
                    log::error!("Unable to get the file stem for the document {:?}", document_path.display());
                    None
                }
            })
            .collect_vec();

        match test_flag {
            TestFlag::GenerateReferenceImages => {
                log::info!(
                    "Generated reference images for the documents {:?}",
                    document_file_names
                );
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
                    return Err(CustomError::with_context(format!(
                        "The documents {:?} have failed the similarity test with the reference images",
                        failed_tests
                    )));
                }
            }
        }

        Ok(())
    }

    pub fn draw_glyphs(
        &mut self,
        document: &Document,
        font_styles_map: &HashMap<String, FontStyles<'static>>,
    ) -> Result<(), CustomError> {
        let scale_factor = self.display.gl_window().window().scale_factor() as f32;
        let mut glyphs = Vec::new();

        let mut caret = point(BORDER_MARGIN, BORDER_MARGIN);
        for content in document.root.iter() {
            let positioned_glyphs = match content {
                Content::Heading { content: text_element } => {
                    let glyphs =
                        layout_heading(font_styles_map, text_element, scale_factor, &mut caret)?;
                    caret.y += HEADING_SEPARATION;
                    glyphs
                }
                Content::Paragraph { contents: text_elements } => {
                    layout_paragraph(font_styles_map, text_elements, scale_factor, &mut caret)?
                }
            };
            caret.x = BORDER_MARGIN;

            for glyph in &positioned_glyphs {
                self.cache.queue_glyph(0, glyph.clone());
            }
            glyphs.extend(positioned_glyphs);
        }
        #[allow(clippy::blocks_in_conditions)]
        self.cache
            .cache_queued(|rectangle, data| {
                self.cache_texture.main_level().write(
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
            .map_err(|error| {
                CustomError::with_source("Unable to cache the queued glyphs".into(), error.into())
            })?;

        let uniforms = UniformsStorage::new(
            "texture_sampler",
            self.cache_texture
                .sampled()
                .magnify_filter(glium::uniforms::MagnifySamplerFilter::Nearest),
        );

        let color = [0.0, 0.0, 0.0, 1.0];
        let (screen_width, screen_height) = {
            let (width, height) = self.display.get_framebuffer_dimensions();
            (width as f32, height as f32)
        };
        let origin = point(0.0, 0.0);
        let vertices: Vec<Vertex> = glyphs
            .iter()
            .filter_map(|glyph| self.cache.rect_for(0, glyph).ok().flatten())
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

        let vertex_buffer =
            glium::VertexBuffer::new(&self.display, &vertices).map_err(|error| {
                CustomError::with_source("Unable to create the vertex buffer".into(), error.into())
            })?;

        let mut target = self.display.draw();
        target.clear_color(1.0, 1.0, 1.0, 0.0);
        target
            .draw(
                &vertex_buffer,
                glium::index::NoIndices(glium::index::PrimitiveType::TrianglesList),
                &self.program,
                &uniforms,
                &glium::DrawParameters {
                    blend: glium::Blend::alpha_blending(),
                    backface_culling: glium::BackfaceCullingMode::CullCounterClockwise,
                    ..Default::default()
                },
            )
            .map_err(|error| {
                CustomError::with_source("Unable to draw the glyphs".into(), error.into())
            })?;

        target.finish().map_err(|error| {
            CustomError::with_source("Unable to finish the drawing operation".into(), error.into())
        })?;

        Ok(())
    }

    pub fn set_window_title(&mut self, document_path: PathBuf) {
        let window_title = format!("{}", document_path.display());
        {
            let window = self.display.gl_window();
            window.window().set_title(&window_title);
        }
    }
}
