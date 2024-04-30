use lopdf::{Object, StringFormat};
use owned_ttf_parser::{AsFaceRef as _, Face, OwnedFace};
use std::{
    collections::{BTreeMap, HashMap},
    io::BufWriter,
    mem,
    path::Path,
};
use time::OffsetDateTime;
use unicode_normalization::UnicodeNormalization as _;

use crate::error::TraceableError;

#[derive(Clone, Copy, Debug, Default)]
pub struct FontMetrics {
    pub ascent: i16,
    pub descent: i16,
    pub units_per_em: u16,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GlyphMetrics {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
struct TtfFontFace {
    inner: std::sync::Arc<owned_ttf_parser::OwnedFace>,
    units_per_em: u16,
}

impl TtfFontFace {
    fn font_metrics(&self) -> FontMetrics {
        FontMetrics {
            ascent: self.face().ascender(),
            descent: self.face().descender(),
            units_per_em: self.units_per_em,
        }
    }

    fn glyph_id(&self, codepoint: char) -> Option<u16> {
        self.face().glyph_index(codepoint).map(|id| id.0)
    }

    fn glyph_ids(&self) -> HashMap<u16, char> {
        let subtables = self
            .face()
            .tables()
            .cmap
            .map(|cmap| cmap.subtables.into_iter().filter(|v| v.is_unicode()));
        let Some(subtables) = subtables else {
            return HashMap::new();
        };
        let mut map = HashMap::with_capacity(self.face().number_of_glyphs().into());
        for subtable in subtables {
            subtable.codepoints(|codepoint| {
                use std::convert::TryFrom as _;

                if let Ok(character) = char::try_from(codepoint) {
                    if let Some(glyph_index) =
                        subtable.glyph_index(codepoint).filter(|index| index.0 > 0)
                    {
                        map.entry(glyph_index.0).or_insert(character);
                    }
                }
            })
        }
        map
    }

    fn glyph_count(&self) -> u16 {
        self.face().number_of_glyphs()
    }

    fn glyph_metrics(&self, glyph_id: u16) -> Option<GlyphMetrics> {
        let glyph_id = owned_ttf_parser::GlyphId(glyph_id);
        if let Some(width) = self.face().glyph_hor_advance(glyph_id) {
            let width = width as u32;
            let height = self
                .face()
                .glyph_bounding_box(glyph_id)
                .map(|bounding_box| {
                    bounding_box.y_max - bounding_box.y_min - self.face().descender()
                })
                .unwrap_or(1000) as u32;
            Some(GlyphMetrics { width, height })
        } else {
            None
        }
    }

    pub fn from_vec(data: &[u8]) -> Result<Self, TraceableError> {
        let face = OwnedFace::from_vec(data.to_vec(), 0)
            .map_err(|error| TraceableError::with_error("Failed to parse font", &error))?;
        let units_per_em = face.as_face_ref().units_per_em();
        Ok(Self {
            inner: std::sync::Arc::new(face),
            units_per_em,
        })
    }

    fn face(&self) -> &Face<'_> {
        self.inner.as_face_ref()
    }
}

#[derive(Debug, Clone)]
struct Font {
    bytes: Vec<u8>,
    ttf_face: TtfFontFace,
    face_identifier: String,
}

/// One layer of PDF data.
#[derive(Debug, Clone)]
pub struct PdfLayer {
    /// Name of the layer. Must be present for the optional content group.
    pub(crate) name: String,
    /// Stream objects in this layer. Usually, one layer == one stream.
    pub(super) operations: Vec<lopdf::content::Operation>,
}

impl From<PdfLayer> for lopdf::Stream {
    fn from(value: PdfLayer) -> Self {
        use lopdf::{Dictionary, Stream};
        let stream_content = lopdf::content::Content {
            operations: value.operations,
        };

        // Page contents should not be compressed
        Stream::new(
            Dictionary::new(),
            stream_content
                .encode()
                .map_err(|error| TraceableError::with_error("Failed to encode PDF content", &error))
                .unwrap(),
        )
        .with_compression(true)
    }
}

use nalgebra_glm as glm;

#[derive(Debug, Clone)]
pub struct ImageXObject {
    /// Width of the image (original width, not scaled width).
    pub width: f32,
    /// Height of the image (original height, not scaled height).
    pub height: f32,
    /// Bits per color component (1, 2, 4, 8, 16) - 1 for black/white, 8 Greyscale / RGB, etc.
    /// If using a JPXDecode filter (for JPEG images), this can be inferred from the image data.
    pub bits_per_component: u16,
    /// Should the image be interpolated when scaled?
    pub interpolate: bool,
    /// The actual data from the image.
    pub image_data: Vec<u8>,
    // SoftMask for transparency, if `None` assumes no transparency. See page 444 of the adope pdf 1.4 reference
    pub soft_mask: Option<lopdf::ObjectId>,
    /// The bounding box of the image.
    pub clipping_bounding_box: Option<glm::Mat4>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
enum XObject {
    Image(ImageXObject),
}

/// Named reference to an `XObject`.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct XObjectReference(String);

impl XObjectReference {
    /// Creates a new reference from a number.
    pub fn new(index: usize) -> Self {
        Self(format!("X{index}"))
    }
}

impl From<XObject> for lopdf::Object {
    fn from(value: XObject) -> Self {
        match value {
            XObject::Image(_) => {
                unimplemented!()
            }
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct XObjectMap(HashMap<String, XObject>);

impl XObjectMap {
    pub fn into_with_document(self, document: &mut lopdf::Document) -> lopdf::Dictionary {
        self.0
            .into_iter()
            .map(|(name, object)| {
                let object: lopdf::Object = object.into();
                let object_reference = document.add_object(object);
                (name, lopdf::Object::Reference(object_reference))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct OcgReference(String);

impl OcgReference {
    /// Creates a new OCG reference from an index.
    pub fn new(index: usize) -> Self {
        Self(format!("MC{index}"))
    }
}

impl OcgLayersMap {
    /// Adds a new OCG List from a reference
    pub fn add_ocg(&mut self, object: lopdf::Object) -> OcgReference {
        let length = self.0.len();
        let ocg_reference = OcgReference::new(length);
        self.0.push((ocg_reference.clone(), object));
        ocg_reference
    }
}

impl From<OcgLayersMap> for lopdf::Dictionary {
    fn from(value: OcgLayersMap) -> Self {
        let mut dictionary = lopdf::Dictionary::new();

        for entry in value.0 {
            dictionary.set(entry.0 .0, entry.1);
        }

        dictionary
    }
}

#[derive(Default, Debug, Clone)]
pub struct OcgLayersMap(Vec<(OcgReference, lopdf::Object)>);

/// Struct for storing the PDF Resources, to be used on a PDF page
#[derive(Default, Debug, Clone)]
pub struct PdfResources {
    /// External graphics objects.
    pub xobjects: XObjectMap,
    /// Layers / optional content ("Properties") in the resource dictionary.
    pub ocg_layers: OcgLayersMap,
}

/// PDF page
#[derive(Debug, Clone)]
pub struct PdfPage {
    /// The index of the page in the document.
    pub(crate) number: usize,
    /// Page width in millimeters.
    pub width: f32,
    /// Page height in millimeters.
    pub height: f32,
    /// Page layers.
    pub layers: Vec<PdfLayer>,
    /// Resources used in this page.
    pub(crate) resources: PdfResources,
    /// Extend the page with custom ad-hoc attributes, as an escape hatch to the low level lopdf library.
    /// Can be used to add annotations to a page.
    /// If your dictionary is wrong it will produce a broken PDF without warning or useful messages.
    pub(crate) extend_with: Option<lopdf::Dictionary>,
}

fn millimeters_to_points(millimeters: f32) -> f32 {
    millimeters * 2.834646
}

pub struct PdfDocument {
    fonts: BTreeMap<String, (lopdf::ObjectId, Font)>,
    inner_document: lopdf::Document,
    identifier: String,
    pages: Vec<PdfPage>,
}

impl PdfDocument {
    pub fn new(identifier: String) -> Self {
        PdfDocument {
            fonts: BTreeMap::default(),
            inner_document: lopdf::Document::with_version("1.3"),
            identifier,
            pages: Vec::new(),
        }
    }

    pub fn add_page_with_layer(&mut self, page_width: f32, page_height: f32) -> (usize, usize) {
        let mut pdf_page = PdfPage {
            number: self.pages.len() + 1,
            width: millimeters_to_points(page_width),
            height: millimeters_to_points(page_height),
            layers: Vec::new(),
            resources: PdfResources::default(),
            extend_with: None,
        };

        let pdf_layer = PdfLayer {
            name: "Layer0".into(),
            operations: Vec::new(),
        };
        pdf_page.layers.push(pdf_layer);
        self.pages.push(pdf_page);

        let page_index = self.pages.len() - 1;
        let layer_index_in_page = 0;
        (page_index, layer_index_in_page)
    }

    pub fn add_font(&mut self, font_path: &Path) -> Result<usize, TraceableError> {
        let font_bytes = std::fs::read(font_path)
            .map_err(|error| TraceableError::with_error("Failed to read font", &error))?;

        let ttf_font_face = TtfFontFace::from_vec(&font_bytes)
            .map_err(|error| TraceableError::with_error("Failed to parse font", &error))?;
        let font = Font {
            bytes: font_bytes,
            ttf_face: ttf_font_face,
            face_identifier: format!("F{}", self.fonts.len()),
        };
        let font_object_id = self.inner_document.new_object_id();
        self.fonts
            .insert(font.face_identifier.clone(), (font_object_id, font.clone()));

        let font_index = self.fonts.len() - 1;
        Ok(font_index)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn write_text_to_layer_in_page(
        &mut self,
        page_index: usize,
        layer_index: usize,
        color: [f32; 3],
        text: String,
        font_index: usize,
        font_size: f32,
        caret_position: [f32; 2],
    ) -> Result<(), TraceableError> {
        let font = self.get_font(font_index)?.1.clone(); // TODO: I shouldn't have to clone the font data
        self.add_operations_to_layer_in_page(
            layer_index,
            page_index,
            vec![
                lopdf::content::Operation::new("BT", vec![]), // Begin text section
                lopdf::content::Operation::new(
                    "Tf",
                    vec![font.face_identifier.clone().into(), (font_size).into()],
                ), // Set the font and the font size
                lopdf::content::Operation::new("Td", {
                    let [x, y] = caret_position;
                    vec![
                        millimeters_to_points(x).into(),
                        millimeters_to_points(y).into(),
                    ]
                }),
                lopdf::content::Operation::new("rg", {
                    let [r, g, b] = color;
                    vec![r, g, b].into_iter().map(lopdf::Object::Real).collect()
                }),
            ],
        )?;

        let mut gid_list = Vec::<u16>::new();
        for character in text.nfc() {
            if let Some(glyph_id) = font.ttf_face.glyph_id(character) {
                gid_list.push(glyph_id);
            }
        }
        let gid_bytes = gid_list
            .iter()
            .flat_map(|x| vec![(x >> 8) as u8, (x & 255) as u8])
            .collect::<Vec<u8>>();
        self.add_operations_to_layer_in_page(
            layer_index,
            page_index,
            vec![lopdf::content::Operation::new(
                "Tj",
                vec![lopdf::Object::String(
                    gid_bytes,
                    lopdf::StringFormat::Hexadecimal,
                )],
            )],
        )?;

        self.add_operations_to_layer_in_page(
            layer_index,
            page_index,
            vec![lopdf::content::Operation::new("ET", vec![])],
        )?;

        Ok(())
    }

    pub fn save_to_bytes(mut self, instance_id: String) -> Result<Vec<u8>, TraceableError> {
        let pages_id = self.inner_document.new_object_id();

        use lopdf::Object::*;
        use lopdf::StringFormat::*;
        // TODO: The user might like to choose all these parameters
        let document_info_id =
            self.inner_document
                .add_object(Dictionary(lopdf::Dictionary::from_iter(vec![
                    ("Trapped", "False".into()),
                    (
                        "CreationDate",
                        String(
                            to_pdf_timestamp_format(&OffsetDateTime::UNIX_EPOCH).into_bytes(),
                            Literal,
                        ),
                    ),
                    (
                        "ModDate",
                        String(
                            to_pdf_timestamp_format(&OffsetDateTime::UNIX_EPOCH).into_bytes(),
                            Literal,
                        ),
                    ),
                    (
                        "GTS_PDFX_Version",
                        String("PDF/A-3:2012".to_string().into_bytes(), Literal),
                    ),
                    ("Title", String("Unknown".to_string().into_bytes(), Literal)),
                    (
                        "Author",
                        String("Unknown".to_string().into_bytes(), Literal),
                    ),
                    (
                        "Creator",
                        String("Unknown".to_string().into_bytes(), Literal),
                    ),
                    (
                        "Producer",
                        String("Unknown".to_string().into_bytes(), Literal),
                    ),
                    (
                        "Subject",
                        String("Unknown".to_string().into_bytes(), Literal),
                    ),
                    (
                        "Identifier",
                        String(self.identifier.clone().into_bytes(), Literal),
                    ),
                    ("Keywords", String("".to_string().into_bytes(), Literal)),
                ])));

        let mut catalog = lopdf::Dictionary::from_iter(vec![
            ("Type", "Catalog".into()),
            ("PageLayout", "OneColumn".into()),
            ("PageMode", "UseNone".into()),
            ("Pages", Reference(pages_id)),
        ]);

        let mut pages = lopdf::Dictionary::from_iter(vec![
            ("Type", "Pages".into()),
            ("Count", Integer(self.pages.len() as i64)),
        ]);
        let mut page_ids = Vec::<lopdf::Object>::new();
        let page_layer_names: Vec<(usize, Vec<::std::string::String>)> = self
            .pages
            .iter()
            .map(|page| {
                (
                    page.number,
                    page.layers.iter().map(|layer| layer.name.clone()).collect(),
                )
            })
            .collect();

        let usage_ocg_dictionary_id =
            self.inner_document
                .add_object(lopdf::Dictionary::from_iter(vec![
                    ("Type", Name("OCG".into())),
                    (
                        "CreatorInfo",
                        Dictionary(lopdf::Dictionary::from_iter(vec![
                            ("Creator", String("Adobe Illustrator 14.0".into(), Literal)), // TODO: What the hell is this?
                            ("Subtype", Name("Artwork".into())),
                        ])),
                    ),
                ]));
        let intent_array_id = self
            .inner_document
            .add_object(Array(vec![Name("View".into()), Name("Design".into())]));

        // Page index, layer index, reference to OCG dictionary
        let ocg_list: Vec<(usize, Vec<(usize, lopdf::Object)>)> = page_layer_names
            .into_iter()
            .map(|(page_index, layer_names)| {
                (
                    page_index,
                    layer_names
                        .into_iter()
                        .enumerate()
                        .map(|(layer_index, layer_name)| {
                            (
                                layer_index,
                                Reference(self.inner_document.add_object(Dictionary(
                                    lopdf::Dictionary::from_iter(vec![
                                        ("Type", Name("OCG".into())),
                                        ("Name", String(layer_name.into(), Literal)),
                                        ("Intent", Reference(intent_array_id)),
                                        ("Usage", Reference(usage_ocg_dictionary_id)),
                                    ]),
                                ))),
                            )
                        })
                        .collect(),
                )
            })
            .collect();

        let flattened_ocg_list: Vec<lopdf::Object> = ocg_list
            .iter()
            .flat_map(|(_, layers)| layers.iter().map(|(_, object)| object.clone()))
            .collect();

        catalog.set(
            "OCProperties",
            Dictionary(lopdf::Dictionary::from_iter(vec![
                ("OCGs", Array(flattened_ocg_list.clone())),
                (
                    "D",
                    Dictionary(lopdf::Dictionary::from_iter(vec![
                        ("Order", Array(flattened_ocg_list.clone())),
                        ("RBGroups", Array(vec![])),
                        ("ON", Array(flattened_ocg_list)),
                    ])),
                ),
            ])),
        );

        let fonts_dictionary = Self::insert_fonts_into_document(&mut self);
        let fonts_dictionary_id = self.inner_document.add_object(fonts_dictionary);

        for (index, page) in self.pages.into_iter().enumerate() {
            let mut page_dictionary = lopdf::Dictionary::from_iter(vec![
                ("Type", "Page".into()),
                ("Rotate", Integer(0)),
                (
                    "MediaBox",
                    vec![0.into(), 0.into(), page.width.into(), page.height.into()].into(),
                ),
                (
                    "TrimBox",
                    vec![0.into(), 0.into(), page.width.into(), page.height.into()].into(),
                ),
                (
                    "CropBox",
                    vec![0.into(), 0.into(), page.width.into(), page.height.into()].into(),
                ),
                ("Annots", vec![].into()),
                ("Parent", Reference(pages_id)),
            ]);

            if let Some(extension) = &page.extend_with {
                for (key, value) in extension.iter() {
                    page_dictionary.set(key.to_vec(), value.clone())
                }
            }

            // Will collect the resources needed for rendering this page
            let unmerged_layers = ocg_list.iter().find(|ocg| ocg.0 - 1 == index).ok_or({
                let comparisons = ocg_list.iter().map(|ocg| ocg.0).collect::<Vec<_>>();
                TraceableError::with_context(
                    format!("Unable to collect the resources needed for rendering the page: can't find {:?} in {:?}", index, comparisons),
                )
            })?;
            let (mut resources_page, layer_streams) =
                page.collect_resources_and_streams(&mut self.inner_document, &unmerged_layers.1);

            resources_page.set("Font", Reference(fonts_dictionary_id)); // TODO: There was a Some here, but I do not know where it went

            if !resources_page.is_empty() {
                let resources_page_id = self.inner_document.add_object(Dictionary(resources_page));
                page_dictionary.set("Resources", Reference(resources_page_id));
            }

            // Merge all streams of the individual layers into one big stream
            let mut layer_streams_merged_vector = Vec::<u8>::new();
            for mut stream in layer_streams {
                layer_streams_merged_vector.append(&mut stream.content);
            }

            let merged_layer_stream =
                lopdf::Stream::new(lopdf::Dictionary::new(), layer_streams_merged_vector);
            let page_content_id = self.inner_document.add_object(merged_layer_stream);

            page_dictionary.set("Contents", Reference(page_content_id));
            let page_id = self.inner_document.add_object(page_dictionary);
            page_ids.push(Reference(page_id))
        }

        pages.set::<_, lopdf::Object>("Kids".to_string(), page_ids.into());

        self.inner_document
            .objects
            .insert(pages_id, Dictionary(pages));

        // Save inner document
        let catalog_id = self.inner_document.add_object(catalog);

        self.inner_document
            .trailer
            .set("Root", Reference(catalog_id));
        self.inner_document
            .trailer
            .set("Info", Reference(document_info_id));
        self.inner_document.trailer.set(
            "ID",
            Array(vec![
                String(self.identifier.into_bytes(), Literal),
                String(instance_id.as_bytes().to_vec(), Literal),
            ]),
        );

        self.inner_document.prune_objects();
        self.inner_document.compress();
        self.inner_document.delete_zero_length_streams();

        let mut pdf_document_bytes = Vec::new();
        let mut writer = BufWriter::new(&mut pdf_document_bytes);
        self.inner_document.save_to(&mut writer).map_err(|error| {
            TraceableError::with_error("Error while saving the PDF document to bytes", &error)
        })?;
        mem::drop(writer);

        Ok(pdf_document_bytes)
    }
}

impl PdfResources {
    pub fn into_with_document_and_layers(
        self,
        inner_document: &mut lopdf::Document,
        layers: Vec<lopdf::Object>,
    ) -> (lopdf::Dictionary, Vec<OcgReference>) {
        let mut dictionary = lopdf::Dictionary::new();

        let mut ocg_dictionary = self.ocg_layers;
        let mut ocg_references = Vec::<OcgReference>::new();

        let xobjects_dictionary: lopdf::Dictionary =
            self.xobjects.into_with_document(inner_document);

        if !layers.is_empty() {
            for layer in layers {
                ocg_references.push(ocg_dictionary.add_ocg(layer));
            }

            let current_ocg_dictionary: lopdf::Dictionary = ocg_dictionary.into();

            if !current_ocg_dictionary.is_empty() {
                dictionary.set(
                    "Properties",
                    lopdf::Object::Dictionary(current_ocg_dictionary),
                );
            }
        }

        if !xobjects_dictionary.is_empty() {
            dictionary.set("XObject", lopdf::Object::Dictionary(xobjects_dictionary));
        }

        (dictionary, ocg_references)
    }
}

impl PdfPage {
    pub(crate) fn collect_resources_and_streams(
        self,
        inner_document: &mut lopdf::Document,
        layers: &[(usize, lopdf::Object)],
    ) -> (lopdf::Dictionary, Vec<lopdf::Stream>) {
        let current_layers = layers.iter().map(|layer| layer.1.clone()).collect();
        let (resource_dictionary, ocg_references) = self
            .resources
            .into_with_document_and_layers(inner_document, current_layers);

        // Set contents
        let mut layer_streams = Vec::<lopdf::Stream>::new();
        use lopdf::content::Operation;
        use lopdf::Object::*;

        for (index, mut layer) in self.layers.into_iter().enumerate() {
            // Push OCG and q to the beginning of the layer
            layer.operations.insert(0, Operation::new("q", vec![]));
            layer.operations.insert(
                0,
                Operation::new(
                    "BDC",
                    vec![
                        Name("OC".into()),
                        Name(ocg_references[index].0.clone().into()),
                    ],
                ),
            );

            // Push OCG END and Q to the end of the layer stream
            layer.operations.push(Operation::new("Q", vec![]));
            layer.operations.push(Operation::new("EMC", vec![]));

            let layer_stream = layer.into();
            layer_streams.push(layer_stream);
        }

        (resource_dictionary, layer_streams)
    }
}

type GlyphId = u32;
type UnicodeCodePoint = u32;
type CmapBlock = Vec<(GlyphId, UnicodeCodePoint)>;

/// Generates a CMAP (character map) from valid cmap blocks.
fn generate_cid_to_unicode_map(face_name: String, all_cmap_blocks: Vec<CmapBlock>) -> String {
    let mut cid_to_unicode_map =
        format!(include_str!("../assets/gid_to_unicode_beg.txt"), face_name);

    for cmap_block in all_cmap_blocks
        .into_iter()
        .filter(|block| !block.is_empty() || block.len() < 100)
    {
        cid_to_unicode_map.push_str(format!("{} beginbfchar\r\n", cmap_block.len()).as_str());
        for (glyph_id, unicode) in cmap_block {
            cid_to_unicode_map.push_str(format!("<{glyph_id:04x}> <{unicode:04x}>\n").as_str());
        }
        cid_to_unicode_map.push_str("endbfchar\r\n");
    }

    cid_to_unicode_map.push_str(include_str!("../assets/gid_to_unicode_end.txt"));
    cid_to_unicode_map
}

impl PdfDocument {
    /// Converts the fonts into a dictionary.
    fn insert_fonts_into_document(&mut self) -> lopdf::Dictionary {
        let mut font_dictionary = lopdf::Dictionary::new();

        for (font_id, font) in self.fonts.iter_mut() {
            let collected_font_dictionary = font.1.insert_into_document(&mut self.inner_document);

            self.inner_document
                .objects
                .insert(font.0, lopdf::Object::Dictionary(collected_font_dictionary));
            font_dictionary.set(font_id.clone(), lopdf::Object::Reference(font.0));
        }
        font_dictionary
    }
}

impl Font {
    fn insert_into_document(&self, inner_document: &mut lopdf::Document) -> lopdf::Dictionary {
        use lopdf::Object::*;
        let face_metrics = self.ttf_face.font_metrics();

        let font_stream = lopdf::Stream::new(
            lopdf::Dictionary::from_iter(vec![("Length1", Integer(self.bytes.len() as i64))]),
            self.bytes.clone(),
        )
        .with_compression(true);

        // Begin stting required font attributes
        let mut font_vector: Vec<(::std::string::String, lopdf::Object)> = vec![
            ("Type".into(), Name("Font".into())),
            ("Subtype".into(), Name("Type0".into())),
            (
                "BaseFont".into(),
                Name(self.face_identifier.clone().into_bytes()),
            ),
            // Identity-H for horizontal writing, Identity-V for vertical writing
            ("Encoding".into(), Name("Identity-H".into())),
            // Missing DescendantFonts and ToUnicode
        ];

        let mut font_descriptor_vector: Vec<(::std::string::String, lopdf::Object)> = vec![
            ("Type".into(), Name("FontDescriptor".into())),
            (
                "FontName".into(),
                Name(self.face_identifier.clone().into_bytes()),
            ),
            ("Ascent".into(), Integer(i64::from(face_metrics.ascent))),
            ("Descent".into(), Integer(i64::from(face_metrics.descent))),
            ("CapHeight".into(), Integer(i64::from(face_metrics.ascent))),
            ("ItalicAngle".into(), Integer(0)),
            ("Flags".into(), Integer(32)),
            ("StemV".into(), Integer(80)),
        ];

        // Maximum height of a single character in the font
        let mut maximum_character_height = 0;
        // Total width of all characters
        let mut total_width = 0;
        // Widths (or heights, depends on self.vertical_writing) of the individual characters, indexed by glyph id
        let mut character_widths = Vec::<(u32, u32)>::new();

        // Glyph IDs - (Unicode IDs - character width, character height)
        let mut cmap = BTreeMap::<u32, (u32, u32, u32)>::new();
        cmap.insert(0, (0, 1000, 1000));

        for (glyph_id, character) in self.ttf_face.glyph_ids() {
            if let Some(glyph_metrics) = self.ttf_face.glyph_metrics(glyph_id) {
                if glyph_metrics.height > maximum_character_height {
                    maximum_character_height = glyph_metrics.height;
                }

                total_width += glyph_metrics.width;
                cmap.insert(
                    glyph_id as u32,
                    (character as u32, glyph_metrics.width, glyph_metrics.height),
                );
            }
        }

        let mut current_first_bit: u16 = 0; // Current first bit of the glyph id (0x10 or 0x12) for example

        let mut all_cmap_blocks = Vec::new();

        {
            let mut current_cmap_block = Vec::new();

            for (glyph_id, unicode_width_tuple) in &cmap {
                if (*glyph_id >> 8) as u16 != current_first_bit || current_cmap_block.len() >= 100 {
                    // End the current (beginbfchar endbfchar) block
                    all_cmap_blocks.push(current_cmap_block.clone());
                    current_cmap_block = Vec::new();
                    current_first_bit = (*glyph_id >> 8) as u16;
                }

                let (unicode, width, _) = *unicode_width_tuple;
                current_cmap_block.push((*glyph_id, unicode));
                character_widths.push((*glyph_id, width));
            }

            all_cmap_blocks.push(current_cmap_block);
        }

        let cid_to_unicode_map =
            generate_cid_to_unicode_map(self.face_identifier.clone(), all_cmap_blocks);
        let cid_to_unicode_map_stream = lopdf::Stream::new(
            lopdf::Dictionary::new(),
            cid_to_unicode_map.as_bytes().to_vec(),
        );
        let cid_to_unicode_map_stream_id = inner_document.add_object(cid_to_unicode_map_stream);

        let mut widths_list = Vec::<Object>::new();
        let mut current_low_gid = 0;
        let mut current_high_gid = 0;
        let mut current_width_vector = Vec::<Object>::new();

        // Scale the font width so that it sort-of fits into an 1000 unit square
        let percentage_font_scaling = 1000.0 / (face_metrics.units_per_em as f32);

        for gid in 0..self.ttf_face.glyph_count() {
            if let Some(GlyphMetrics { width, .. }) = self.ttf_face.glyph_metrics(gid) {
                if gid == current_high_gid {
                    current_width_vector
                        .push(Integer((width as f32 * percentage_font_scaling) as i64));
                    current_high_gid += 1;
                } else {
                    widths_list.push(Integer(current_low_gid as i64));
                    widths_list.push(Array(std::mem::take(&mut current_width_vector)));

                    current_width_vector
                        .push(Integer((width as f32 * percentage_font_scaling) as i64));
                    current_low_gid = gid;
                    current_high_gid = gid + 1;
                }
            } else {
                continue;
            }
        }
        // Push the last widths, because the loop is delayed by one iteration
        widths_list.push(Integer(current_low_gid as i64));
        widths_list.push(Array(std::mem::take(&mut current_width_vector)));

        let mut font_descriptors = lopdf::Dictionary::from_iter(vec![
            ("Type", Name("Font".into())),
            ("Subtype", Name("CIDFontType2".into())),
            ("BaseFont", Name(self.face_identifier.clone().into())),
            (
                "CIDSystemInfo",
                Dictionary(lopdf::Dictionary::from_iter(vec![
                    ("Registry", String("Adobe".into(), StringFormat::Literal)),
                    ("Ordering", String("Identity".into(), StringFormat::Literal)),
                    ("Supplement", Integer(0)),
                ])),
            ),
            ("W", Array(widths_list)),
            ("DW", Integer(1000)),
        ]);

        let font_bounding_box = vec![
            Integer(0),
            Integer(maximum_character_height as i64),
            Integer(total_width as i64),
            Integer(maximum_character_height as i64),
        ];
        font_descriptor_vector.push((
            "FontFile2".into(),
            Reference(inner_document.add_object(font_stream)),
        ));

        // Although the following entry is technically not needed, Adobe Reader needs it
        font_descriptor_vector.push(("FontBBox".into(), Array(font_bounding_box)));

        let font_descriptor_vec_id =
            inner_document.add_object(lopdf::Dictionary::from_iter(font_descriptor_vector));

        font_descriptors.set("FontDescriptor", Reference(font_descriptor_vec_id));

        font_vector.push((
            "DescendantFonts".into(),
            Array(vec![Dictionary(font_descriptors)]),
        ));
        font_vector.push(("ToUnicode".into(), Reference(cid_to_unicode_map_stream_id)));

        lopdf::Dictionary::from_iter(font_vector)
    }
}

impl PdfDocument {
    fn add_operations_to_layer_in_page(
        &mut self,
        layer_index: usize,
        page_index: usize,
        operations: Vec<lopdf::content::Operation>,
    ) -> Result<(), TraceableError> {
        let pdf_layer_reference = self.get_mut_layer_in_page(layer_index, page_index)?;
        pdf_layer_reference.operations.extend(operations);

        Ok(())
    }
    fn get_font(&mut self, font_index: usize) -> Result<&((u32, u16), Font), TraceableError> {
        self.fonts
            .get(&format!("F{font_index}"))
            .ok_or(TraceableError::with_context(format!(
                "Failed to find font {}",
                font_index
            )))
    }

    fn get_mut_layer_in_page(
        &mut self,
        layer_index: usize,
        page_index: usize,
    ) -> Result<&mut PdfLayer, TraceableError> {
        let pdf_page = self
            .pages
            .get_mut(page_index)
            .ok_or(TraceableError::with_context(format!(
                "Failed to find the page with index {}",
                page_index
            )))?;
        let pdf_layer =
            pdf_page
                .layers
                .get_mut(layer_index)
                .ok_or(TraceableError::with_context(format!(
                    "Failed to find the layer with index {}",
                    layer_index
                )))?;

        Ok(pdf_layer)
    }
}

// D:20170505150224+02'00'
fn to_pdf_timestamp_format(date: &OffsetDateTime) -> String {
    let offset = date.offset();
    let offset_sign = if offset.is_negative() { '-' } else { '+' };
    format!(
        "D:{:04}{:02}{:02}{:02}{:02}{:02}{offset_sign}{:02}'{:02}'",
        date.year(),
        u8::from(date.month()),
        date.day(),
        date.hour(),
        date.minute(),
        date.second(),
        offset.whole_hours().abs(),
        offset.minutes_past_hour().abs(),
    )
}
