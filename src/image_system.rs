// #![deny(clippy::unwrap_used, clippy::expect_used)]

use image::{DynamicImage, Rgba, RgbaImage};
use rusttype::{point, Scale};

use crate::{
    document::Document, document_configuration::DocumentConfiguration,
    traceable_error::TraceableError,
};

pub trait DocumentInterface {
    type RenderedDocument;

    fn render_document(
        &mut self,
        document: &Document,
        document_configuration: &DocumentConfiguration,
    ) -> Result<Self::RenderedDocument, TraceableError>;
}
pub struct ImageSystem {}

impl DocumentInterface for ImageSystem {
    type RenderedDocument = RgbaImage;

    fn render_document(
        &mut self,
        document: &Document,
        document_configuration: &DocumentConfiguration,
    ) -> Result<Self::RenderedDocument, TraceableError> {
        let scale = Scale::uniform(document_configuration.font_size as f32);
        let mut positioned_glyphs = Vec::new();
        document.root_environment.layout(
            document_configuration,
            scale,
            None,
            &mut point(0.0, 0.0),
            &mut positioned_glyphs,
        )?;

        let mut image = DynamicImage::new_rgba8(
            document_configuration.page_width,
            document_configuration.page_height,
        )
        .to_rgba8();
        let color = (0, 0, 0);

        for glyph in positioned_glyphs {
            if let Some(bounding_box) = glyph.pixel_bounding_box() {
                // Draw the glyph into the image per-pixel by using the draw closure
                glyph.draw(|x, y, coverage| {
                    image.put_pixel(
                        // Offset the position by the glyph bounding box
                        x + bounding_box.min.x as u32,
                        y + bounding_box.min.y as u32,
                        // Turn the coverage into an alpha value
                        Rgba([color.0, color.1, color.2, (coverage * 255.0) as u8]),
                    )
                });
            }
        }

        Ok(image)
    }
}
