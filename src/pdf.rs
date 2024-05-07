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

use crate::error::ContextError;

/// The (insofar) relevant vertical metrics of a font.
#[derive(Clone, Copy, Debug, Default)]
pub struct FontMetrics {
    /// The ascent of the font.
    pub ascent: i16,
    /// The descent of the font.
    pub descent: i16,
    /// The number of units per em of the font.
    pub units_per_em: u16,
}

/// The (insofar) relevant metrics associated to a single glyph of a font.
#[derive(Clone, Copy, Debug, Default)]
pub struct GlyphMetrics {
    /// The width of the glyph.
    pub width: u32,
    /// The height of the glyph.
    pub height: u32,
}

/// A font face loaded from a TTF font, together with its measure of units per em.
#[derive(Clone, Debug)]
struct TtfFontFace {
    /// The underlying font face which is represented through the `ttf_parser` crate.
    inner: std::sync::Arc<owned_ttf_parser::OwnedFace>,
    /// The number of units per em of the font face.
    units_per_em: u16,
}

impl TtfFontFace {
    /// Retrieve the font metrics from the associated font face.
    fn font_metrics(&self) -> FontMetrics {
        FontMetrics {
            ascent: self.face().ascender(),
            descent: self.face().descender(),
            units_per_em: self.units_per_em,
        }
    }

    /// Retrieve the glyph ID of a specific codepoint, which in our case is just a `char`.
    fn glyph_id(&self, codepoint: char) -> Option<u16> {
        self.face()
            .glyph_index(codepoint)
            .map(|glyph_id| glyph_id.0)
    }

    /// Retrieve the mapping between the glyph IDs and the characters (codepoints), that specifically
    /// contains exactly the number of unicode glyphs present in the font.
    fn glyph_ids(&self) -> HashMap<u16, char> {
        // Retrieve all the unicode subtables of the font face
        let font_subtables = self.face().tables().cmap.map(|cmap| {
            cmap.subtables
                .into_iter()
                .filter(|font_subtable| font_subtable.is_unicode())
        });
        // If no suitable subtables have been found, then return an empty association between
        // glyph IDs and characters
        let Some(font_subtables) = font_subtables else {
            return HashMap::new();
        };

        // Once the subtables have been fetched, creates an association between the glyph IDs
        // and the characters (codepoints) that contains the number of glyphs of the font face
        let mut gid_to_codepoint_map =
            HashMap::with_capacity(self.face().number_of_glyphs().into());
        for font_subtable in font_subtables {
            font_subtable.codepoints(|codepoint| {
                use std::convert::TryFrom as _;

                if let Ok(character) = char::try_from(codepoint) {
                    // For each character in each subtable, if it is a valid UTF-8 codepoint, then
                    // retrieve its glyph index only if it is positive and insert it in to the
                    // association between glyph IDs and characters
                    if let Some(glyph_index) = font_subtable
                        .glyph_index(codepoint)
                        .filter(|index| index.0 > 0)
                    {
                        gid_to_codepoint_map
                            .entry(glyph_index.0)
                            .or_insert(character);
                    }
                }
            })
        }

        gid_to_codepoint_map
    }

    /// Retrieve the total number of glyphs present in the font face.
    fn glyph_count(&self) -> u16 {
        self.face().number_of_glyphs()
    }

    /// Attempt to calculate the metrics of a glyph from the associated glyph ID, taken as input.
    fn glyph_metrics(&self, glyph_id: u16) -> Option<GlyphMetrics> {
        // Wrap an integer into a `GlyphId` for enabling the associated traits
        let glyph_id = owned_ttf_parser::GlyphId(glyph_id);

        if let Some(width) = self.face().glyph_hor_advance(glyph_id) {
            let width = width as u32;
            // The height of the glyph is corrected by employing the descender vertical metric
            // of the font face (this is supposedly valid only for horizontally-laid fonts).
            let height = self
                .face()
                .glyph_bounding_box(glyph_id)
                .map(|bounding_box| {
                    bounding_box.y_max - bounding_box.y_min - self.face().descender()
                })
                // If it fails to retrieve the height, default to 1000. This is not a problem
                // for us as we employ properly-built fonts from the CMU family.
                .unwrap_or(1000) as u32;

            Some(GlyphMetrics { width, height })
        } else {
            // If it cannot find the horizontal glyph advance, return accordingly nothing
            None
        }
    }

    /// Constructs a font face from the underlying raw data extracted from the TTF font file.
    pub fn from_bytes(data: &[u8]) -> Result<Self, ContextError> {
        let face = OwnedFace::from_vec(data.to_vec(), 0)
            .map_err(|error| ContextError::with_error("Failed to parse font", &error))?;
        let units_per_em = face.as_face_ref().units_per_em();

        Ok(Self {
            inner: std::sync::Arc::new(face),
            units_per_em,
        })
    }

    /// Retrieve the underlying font face as a reference.
    fn face(&self) -> &Face<'_> {
        self.inner.as_face_ref()
    }
}

/// A font loaded from a TTF font, together with its measure of units per em, the byte data
/// data was loaded from and an identifier for the font face.
#[derive(Debug, Clone)]
pub struct Font {
    /// The byte data the font was loaded from.
    bytes: Vec<u8>,
    /// The actual font face, together with its measure of units per em.
    ttf_face: TtfFontFace,
    /// The identifier of the font face.
    face_identifier: String,
}

impl Font {
    /// Takes a well-formed font and inserts it into the PDF document, returning the associated PDF dictionary.
    fn insert_into_document(&self, inner_document: &mut lopdf::Document) -> lopdf::Dictionary {
        use lopdf::Object::*;
        // Retrieve the font metrics of the underlying font face
        let face_metrics = self.ttf_face.font_metrics();

        // Construct the PDF stream which sets the length in bytes of the font data, this is requested by
        // the PDF specification because the PDF format with mixed text and byte data
        let font_stream = lopdf::Stream::new(
            lopdf::Dictionary::from_iter(vec![("Length1", Integer(self.bytes.len() as i64))]),
            self.bytes.clone(),
        )
        .with_compression(false); // Do not compress it

        // Begin setting the required font attributes
        let mut font_vector: Vec<(::std::string::String, lopdf::Object)> = vec![
            ("Type".into(), Name("Font".into())),
            ("Subtype".into(), Name("Type0".into())),
            (
                "BaseFont".into(),
                Name(self.face_identifier.clone().into_bytes()),
            ),
            // `Identity-H` is used for horizontal writing, while `Identity-V` for vertical writing
            ("Encoding".into(), Name("Identity-H".into())),
            // Although it is missing `DescendantFonts` and `ToUnicode`, these will be inserted later on
        ];

        // Specify the font properties which will be used by PDF renderers to position the glyphs
        let mut font_descriptor_vector: Vec<(::std::string::String, lopdf::Object)> = vec![
            ("Type".into(), Name("FontDescriptor".into())),
            (
                "FontName".into(),
                Name(self.face_identifier.clone().into_bytes()),
            ),
            ("Ascent".into(), Integer(i64::from(face_metrics.ascent))),
            ("Descent".into(), Integer(i64::from(face_metrics.descent))),
            ("CapHeight".into(), Integer(i64::from(face_metrics.ascent))),
            ("ItalicAngle".into(), Integer(0)), // I don't know any way of extracting this value from the font data
            // This means that the font uses the Adobe standard Latin character set or a subset of it (https://pdfium.patagames.com/help/html/T_Patagames_Pdf_Enums_FontFlags.htm)
            ("Flags".into(), Integer(32)),
            // This is a very complicated parameter to determine (https://stackoverflow.com/questions/35485179/stemv-value-of-the-truetype-font)
            // The value 80 is the default value for `StemV` and is used here as an approximately appropriate value
            ("StemV".into(), Integer(80)),
        ];

        // Maximum height of a single character in the font
        let mut maximum_character_height = 0;
        // Total width of all characters
        let mut total_width = 0;

        // This is an association between glyph IDs and triplets of Unicode IDs, character widths and character heights
        let mut gid_to_glyph_properties_map = BTreeMap::<u32, (u32, u32, u32)>::new();

        // TODO(ghovax): Figure out why the original author of this library originally inserted this line of code,
        // because I don't really know what it does, but it doesn't seem to break anything.
        gid_to_glyph_properties_map.insert(0, (0, 1000, 1000));

        // For each pair ofglyph ID and associated character present in the font face...
        for (glyph_id, character) in self.ttf_face.glyph_ids() {
            // Retrieve the glyph metrics for that glyph ID
            if let Some(glyph_metrics) = self.ttf_face.glyph_metrics(glyph_id) {
                if glyph_metrics.height > maximum_character_height {
                    // Save the maximum character heights registered so far into a variable to be later used
                    maximum_character_height = glyph_metrics.height;
                }

                // Register what is the total width of the glyphs so far encountered
                total_width += glyph_metrics.width;
                // Save the glyph metrics and the character when associated to a specific glyph ID, again to be later used
                gid_to_glyph_properties_map.insert(
                    glyph_id as u32,
                    (character as u32, glyph_metrics.width, glyph_metrics.height),
                );
            }
        }

        // NOTE(ghovax): The following is a comment from the original author, I found the explanation to be good enough
        // but the comment of the code lackluster, so I've added more to clarify what the code is actually doing.

        // The following operations map the character index to a Unicode value, this will then be added to the "ToUnicode" dictionary
        //
        // To explain this structure: Glyph IDs have to be in segments where the first byte of the
        // first and last element have to be the same. A range from 0x1000 - 0x10FF is valid
        // but a range from 0x1000 - 0x12FF is not (0x10 != 0x12)
        // Plus, the maximum number of Glyph-IDs in one range is 100
        //
        // Since the glyph IDs are sequential, all we really have to do is to enumerate the vector
        // and create buckets of 100 / rest to 256 if needed

        let mut current_first_bit: u16 = 0; // Current first bit of the glyph ID (0x10 or 0x12) for example
        let mut all_gid_to_character_blocks = Vec::new();

        // Widths (or heights, depends on `self.vertical_writing`) of the individual characters, indexed by glyph ID
        let mut character_widths = Vec::<(u32, u32)>::new();

        let mut current_gid_to_character_block = Vec::new();
        // For each previously collected glyph ID, extract the associated character and width of the corresponding glyph...
        for (glyph_id, (character, glyph_width, _glyph_height)) in
            gid_to_glyph_properties_map.iter()
        {
            // Remap the glyph ID into the accepted range for the PDF specification and make sure that
            // we haven't reached the first bit of the current bucket, or either that we haven't exceeded the maximum bucket length of 100 elements
            if (*glyph_id >> 8) as u16 != current_first_bit
                || current_gid_to_character_block.len() >= 100
            {
                // End the current (beginbfchar endbfchar) block
                all_gid_to_character_blocks.push(current_gid_to_character_block.clone());
                current_gid_to_character_block = Vec::new();
                current_first_bit = (*glyph_id >> 8) as u16;
            }

            // Add the glyph ID and the associated character to the current block and register the character widths for future usage
            current_gid_to_character_block.push((*glyph_id, *character));
            character_widths.push((*glyph_id, *glyph_width));
        }

        // Do not forget to append the last block
        all_gid_to_character_blocks.push(current_gid_to_character_block);

        // Generate the mapping between the character IDs and the Unicode equivalents, then construct the associated PDF stream
        // Finally, add it to the PDF document and save the associated object ID for later usage
        let cid_to_unicode_map =
            generate_cid_to_unicode_map(self.face_identifier.clone(), all_gid_to_character_blocks);
        let cid_to_unicode_map_stream = lopdf::Stream::new(
            lopdf::Dictionary::new(),
            cid_to_unicode_map.as_bytes().to_vec(),
        );
        let cid_to_unicode_map_stream_id = inner_document.add_object(cid_to_unicode_map_stream);

        // NOTE(ghovax): The following is a comments from the original author.

        // Encode widths and heights so that they fit into what PDF expects
        // See page 439 in the PDF 1.7 reference
        // Basically `widths_objects` will contain objects like this: 20 [21, 99, 34, 25],
        // which means that the character with the GID 20 has a width of 21 units and the character with the GID 21 has a width of 99 units
        let mut width_objects = Vec::<Object>::new();
        let mut current_lesser_glyph_id = 0;
        let mut current_upper_gid = 0;
        let mut current_widths_vector = Vec::<Object>::new();

        // Scale the font width so that it sort-of fits into an 1000 unit square
        // TODO(ghovax): Why does he exactly need to do that?
        let percentage_font_scaling = 1000.0 / (face_metrics.units_per_em as f32);

        // For each glyph ID present in the font face...
        for glyph_id in 0..self.ttf_face.glyph_count() {
            // If it has an available width extracted from the font itself...
            if let Some(GlyphMetrics { width, .. }) = self.ttf_face.glyph_metrics(glyph_id) {
                if glyph_id == current_upper_gid {
                    // Register its width (corrected by the font scaling) as a PDF object if its glyph ID
                    // is the same as the current upper bound of the glyph ID range
                    current_widths_vector
                        .push(Integer((width as f32 * percentage_font_scaling) as i64));
                    current_upper_gid += 1;
                } else {
                    // Otherwise, drain the current width vector to the width objects and update the current lesser and upper glyph IDs
                    width_objects.push(Integer(current_lesser_glyph_id as i64));
                    width_objects.push(Array(std::mem::take(&mut current_widths_vector)));

                    current_widths_vector
                        .push(Integer((width as f32 * percentage_font_scaling) as i64));
                    current_lesser_glyph_id = glyph_id;
                    current_upper_gid = glyph_id + 1;
                }
            } else {
                // If the width is not available, then we just skip the character and log it
                log::warn!("Glyph ID {} for the font {:?} has no width, skipping it when adding it to the document from the font", glyph_id, self.face_identifier);
                continue;
            }
        }

        // Push the last widths in any case because the loop is delayed by one iteration
        width_objects.push(Integer(current_lesser_glyph_id as i64));
        width_objects.push(Array(std::mem::take(&mut current_widths_vector)));

        // Configure the descriptors of the font for it to adhere to the PDF specification
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
            ("W", Array(width_objects)), // Include the widths of the characters
            ("DW", Integer(1000)),       // TODO(ghovax): Why is the default width 1000?
        ]);

        // Add to the document the bounding box for the glyphs of the chosen font face
        // NOTE(ghovax): From first hand experience I've seen that this encoding overestimates the glyphs'
        // bounding box when highlighting them with the cursor in any PDF viewer. After parsing the document
        // through ghostscript, the issue is resolved.
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

        // NOTE(ghovax): The following is a comment from the original author.
        // Although the following entry is technically not needed, Adobe Reader needs it
        font_descriptor_vector.push(("FontBBox".into(), Array(font_bounding_box)));

        // Finally, add the font descriptors to the PDF document and set the associated key according to the specification
        let font_descriptor_vector_id =
            inner_document.add_object(lopdf::Dictionary::from_iter(font_descriptor_vector));
        font_descriptors.set("FontDescriptor", Reference(font_descriptor_vector_id));

        // Do not forget to add the fields to the font information that we have initially omitted
        // because we need to calculate them, logically this is a chain of elements of elements which
        // are added to the PDF document in succession, one after the other
        font_vector.push((
            "DescendantFonts".into(),
            Array(vec![Dictionary(font_descriptors)]),
        ));
        font_vector.push(("ToUnicode".into(), Reference(cid_to_unicode_map_stream_id)));

        // In the end return the constructed font PDF dictionary to be inserted into the document
        lopdf::Dictionary::from_iter(font_vector)
    }
}

/// One layer of PDF data. It can be converted into a `lopdf::Stream` by calling `Into<lopdf::Stream>::into`.
#[derive(Debug, Clone)]
pub struct PdfLayer {
    /// Name of the layer. Must be present for the optional content group.
    pub(crate) name: String,
    /// Stream objects in this layer. Usually, one layer equals to one stream.
    pub(super) operations: Vec<lopdf::content::Operation>,
}

impl From<PdfLayer> for lopdf::Stream {
    fn from(value: PdfLayer) -> Self {
        use lopdf::{Dictionary, Stream};
        // Construct the stream content from the actual underlying operations of the layer
        let stream_content = lopdf::content::Content {
            operations: value.operations,
        };

        // Encode the uncompressed stream content into the stream
        Stream::new(
            Dictionary::new(),
            stream_content
                .encode()
                .map_err(|error| {
                    ContextError::with_error("Failed to encode PDF layer content", &error)
                })
                .unwrap(),
        )
        .with_compression(false) // Page contents should not be compressed
    }
}

use nalgebra_glm as glm;

/// The low-level image representation for a PDF document.
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
    // SoftMask for transparency, if `None` assumes no transparency. See page 444 of the adope pdf 1.4 reference.
    pub soft_mask: Option<lopdf::ObjectId>,
    /// The bounding box of the image.
    pub clipping_bounding_box: Option<glm::Mat4>,
}

/// `XObject`s are parts of the PDF specification. They allow for complex behavior to be
/// inserted into the PDF document: this comprises bookmarks, annotations and even images.
/// My implementation is only partial as it allows only for images.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum XObject {
    /// The `XObject` interface for an image. It can be converted into a `lopdf::Object`.
    Image(ImageXObject),
}

impl From<XObject> for lopdf::Object {
    fn from(value: XObject) -> Self {
        match value {
            // TODO(ghovax): The conversion from an `XObject` to a PDF object is not yet implemented.
            XObject::Image(_) => {
                unimplemented!()
            }
        }
    }
}

/// Named reference to an `XObject`.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct XObjectReference(String);

impl XObjectReference {
    /// Creates a new reference for an `XObject` from a number.
    pub fn new(index: usize) -> Self {
        Self(format!("X{index}"))
    }
}

/// The association between the `XObject`s properties and the actual `XObject`s themselves.
#[derive(Default, Debug, Clone)]
pub struct XObjectMap(HashMap<String, XObject>);

impl XObjectMap {
    /// Inserts the `XObject`s into the document, simultaneously constructing a PDF dictionary of them.
    pub fn into_with_document(&self, document: &mut lopdf::Document) -> lopdf::Dictionary {
        self.0
            .iter()
            .map(|(name, object)| {
                // For each `XObject` present into the map, add it to the document by first converting it into a PDF object
                let object: lopdf::Object = object.clone().into();
                let object_reference = document.add_object(object);
                // Then collect the associated object name and reference to it into a PDF dictionary, which is returned in the end
                (name.clone(), lopdf::Object::Reference(object_reference))
            })
            .collect()
    }
}

/// A named reference to an OCG (Optional Content Group), which is parts of the PDF specification.
#[derive(Debug, Clone)]
pub struct OcgReference(String);

impl OcgReference {
    /// Creates a new OCG reference from an index.
    pub fn new(index: usize) -> Self {
        Self(format!("MC{index}"))
    }
}

/// The association between the OCG references and the actual PDF objects.
#[derive(Default, Debug, Clone)]
pub struct OcgLayersMap(Vec<(OcgReference, lopdf::Object)>);

impl OcgLayersMap {
    /// Adds a PDF object to the map for the OCG layers. Returns the reference to the added object.
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

        // Construct a dictionary from the pairs of the mapping
        for entry in value.0 {
            dictionary.set((entry.0).0, entry.1);
        }

        dictionary
    }
}

/// Struct for storing the PDF Resources, to be used on a PDF page.
#[derive(Default, Debug, Clone)]
pub(crate) struct PdfResources {
    /// External graphics objects.
    pub xobjects: XObjectMap,
    /// Layers / optional content ("Properties") in the resource dictionary.
    pub ocg_layers: OcgLayersMap,
}

impl PdfResources {
    /// Inserts the resources into the document, simultaneously constructing a PDF dictionary of them.
    /// Returns the constructed dictionary and the vector of the OCG references.
    pub(crate) fn with_document_and_layers(
        &self,
        inner_document: &mut lopdf::Document,
        layers: Vec<lopdf::Object>,
    ) -> (lopdf::Dictionary, Vec<OcgReference>) {
        let mut dictionary = lopdf::Dictionary::new();

        let mut ocg_layers_dictionary = self.ocg_layers.clone();
        let mut ocg_references = Vec::<OcgReference>::new();

        // Insert the in `XObjects` into the document and obtain the associated dictionary
        let xobjects_dictionary: lopdf::Dictionary =
            self.xobjects.into_with_document(inner_document);

        // If the given layers are not empty..
        if !layers.is_empty() {
            for layer in layers {
                // Add each layer to the OCG dictionary
                ocg_references.push(ocg_layers_dictionary.add_ocg(layer));
            }

            // Construct a dictionary from the OCG layers
            let current_ocg_dictionary: lopdf::Dictionary = ocg_layers_dictionary.into();

            // If the OCG dictionary is not empty..
            if !current_ocg_dictionary.is_empty() {
                // Add the OCG dictionary to the PDF dictionary
                dictionary.set(
                    "Properties",
                    lopdf::Object::Dictionary(current_ocg_dictionary),
                );
            }
        }

        // Again, if the `XObjects` dictionary isn't empty, set the associated PDF key to the appropriated value
        if !xobjects_dictionary.is_empty() {
            dictionary.set("XObject", lopdf::Object::Dictionary(xobjects_dictionary));
        }

        // Finally, return the constructed dictionary and the OCG references for later usage
        (dictionary, ocg_references)
    }
}

/// The representation of a PDF page. Utility functions are implemented for this struct
/// so that its content can be inserted into the underlying PDF document.
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

impl PdfPage {
    /// Iterates over all the layers in order to construct the dictionary for the PDF resources
    /// and the PDF streams contained into the page so that they can be inserted in to the document.
    /// Returns the dictionary of the resources and the vector containing all the streams associated
    /// to the layers.
    ///
    /// # Arguments
    ///
    /// * `inner_document` - The underlying PDF document.
    /// * `layers` - The layers to be iterated over.
    pub(crate) fn collect_resources_and_streams(
        &mut self,
        inner_document: &mut lopdf::Document,
        layers: &[(usize, lopdf::Object)],
    ) -> Result<(lopdf::Dictionary, Vec<lopdf::Stream>), ContextError> {
        // Collects all the objects present in the given layers
        let current_layers = layers.iter().map(|layer| layer.1.clone()).collect();
        // Collect the resources dictionary and the references to the OCG from the resources of the page,
        // simultaneously inserting them into the PDF document
        let (resource_dictionary, ocg_references) = self
            .resources
            .with_document_and_layers(inner_document, current_layers);

        let mut layer_streams = Vec::<lopdf::Stream>::new();
        use lopdf::content::Operation;
        use lopdf::Object::*;

        for (index, layer) in self.layers.iter_mut().enumerate() {
            // Push OCG and q to the beginning of the layer
            // In the PDF specification the q/Q operator is an operator which creates an isolated graphics state block
            // In our case we are creating one with no state
            layer.operations.insert(0, Operation::new("q", vec![]));
            // In the PDF specification the BDC operator represents the beginning of a marked-content sequence
            layer.operations.insert(
                0,
                Operation::new(
                    "BDC",
                    vec![
                        // In the PDF specification the OC operand relates to optional content
                        Name("OC".into()),
                        Name(
                            ocg_references
                                .get(index)
                                .ok_or(ContextError::with_context(
                                    "Unable to find the index in the OCG references",
                                ))?
                                .0
                                .clone()
                                .into(),
                        ),
                    ],
                ),
            );

            // Push OCG END and Q to the end of the layer stream
            layer.operations.push(Operation::new("Q", vec![]));
            layer.operations.push(Operation::new("EMC", vec![]));

            let layer_stream = layer.clone().into();
            layer_streams.push(layer_stream);
        }

        Ok((resource_dictionary, layer_streams))
    }
}

/// Converts millimeters to points. This function is used in order to present the data
/// in the format required by the PDF specification, while the end user might want to work in
/// millimeters which are easier to reason about.
fn millimeters_to_points(millimeters: f32) -> f32 {
    millimeters * 2.834646
}

/// This struct represents the actual PDF document on a high-level. It is an interface to the actual underlying
/// `lopdf::document` with the addition of the PDF pages, the document ID and the fonts used in the document.
///
/// Various convenience functions are exposed for this struct, such as `add_page_with_layer`, `add_font`,
/// `write_text_to_layer_in_page`, `save_to_bytes`, which make the creation of a PDF document very much simplified.
pub struct PdfDocument {
    /// The association between the fonts ID, the object it is represented by and its face data.
    fonts: BTreeMap<String, (lopdf::ObjectId, Font)>,
    /// The underlying PDF document: this is a low-level interface and shouldn't be directly interacted with
    /// unless strictly necessary, anyway this is why it is exposed to the user.
    pub inner_document: lopdf::Document,
    /// The identifier of the document, it is used to in order to set the PDF `ID` tag.
    pub identifier: String,
    /// The pages of the PDF document.
    pub(crate) pages: Vec<PdfPage>,
}

impl PdfDocument {
    /// Create a new `PdfDocument` by defaulting the underlying PDF document to version 1.5
    /// of the PDF specification and customly specifying the PDF identifier.
    ///
    /// # Arguments
    ///
    /// * `pdf_document_identifier` - The identifier to be given to the PDF document.
    pub fn new(pdf_document_identifier: String) -> Self {
        PdfDocument {
            fonts: BTreeMap::default(),
            inner_document: lopdf::Document::with_version("1.5"),
            identifier: pdf_document_identifier,
            pages: Vec::new(),
        }
    }

    /// Adds a page of given width and height in millimeters with an empty layer for contents to be added to.
    /// The function returns the index of the page and of the layer in the page, these are to be passed
    /// to the other functions when calling them, such as to `write_text_to_layer_in_page`.
    /// The reason why we work with indices is because it notably simplifies the handling of the pages and the layers.
    ///
    /// # Arguments
    ///
    /// * `page_width` - The width of the PDF page to be created as expressed in millimeters.
    /// * `page_height` - The height of the PDF page to be created as expressed in millimeters.
    pub fn add_page_with_layer(&mut self, page_width: f32, page_height: f32) -> (usize, usize) {
        // Creates a new PDF page correctly numbered
        let mut pdf_page = PdfPage {
            number: self.pages.len() + 1,
            width: millimeters_to_points(page_width), // Convert millimeters to points because this is what `lopdf` expects
            height: millimeters_to_points(page_height),
            layers: Vec::new(), // The layer will be later added
            resources: PdfResources::default(),
            extend_with: None, // NOTE(ghovax): This could be actually further on inserted, but it's not clear how even from the original author's work.
        };

        // Create a new PDF layer with a pre-given name and then append it to the current page.
        let pdf_layer = PdfLayer {
            name: "Layer0".into(),
            operations: Vec::new(),
        };
        pdf_page.layers.push(pdf_layer);
        self.pages.push(pdf_page);

        let page_index = self.pages.len() - 1;
        let layer_index_in_page = 0;
        // Return the page and layer in page indices
        (page_index, layer_index_in_page)
    }

    /// Add a font from the given path to the document. This function expects the font to be TTF, or either way
    /// an OTF font which is just a wrapper around a TTF font. If successful, the function returns
    /// the index of the font which is then to be used in order to write text via the `write_text_to_layer_in_page` function.
    ///
    /// # Arguments
    ///
    /// * `font_path` - The path to the TTF/OTF font to be loaded into the PDF document.
    pub fn add_font(&mut self, font_path: &Path) -> Result<usize, ContextError> {
        // Load the bytes associated to the font from the given path
        let font_bytes = std::fs::read(font_path).map_err(|error| {
            ContextError::with_error("Failed to read font, probably the path is wrong", &error)
        })?;

        // Parse the font face from the given data and then construct the font
        let ttf_font_face = TtfFontFace::from_bytes(&font_bytes)
            .map_err(|error| ContextError::with_error("Failed to parse font", &error))?;
        let font = Font {
            bytes: font_bytes,
            ttf_face: ttf_font_face,
            face_identifier: format!("F{}", self.fonts.len()),
        };
        // Inserts the object into the fonts of the PDF document, to be later processed
        let font_object_id = self.inner_document.new_object_id();
        self.fonts
            .insert(font.face_identifier.clone(), (font_object_id, font.clone()));

        let font_index = self.fonts.len() - 1;
        // Return the font index
        Ok(font_index)
    }

    /// Writes the text in the specified font, color at the caret position to the PDF document. The information is
    /// inserted onto the given layer of the specified page (refer to the other functions documentation for more details).
    /// If the operation is successful, then return nothing.
    ///
    /// # Arguments
    ///
    /// * `page_index` - The index of the page to write the text to (should be previously obtained).
    /// * `layer_index` - The index of the layer to write the text to (should be previously obtained).
    /// * `color` - The RGB color employed for filling of the text.
    /// * `text` - The text to be written at the given layer in the given page.
    /// * `font_index` - The index of the font to be used when writing the text (should be previously obtained).
    /// * `font_size` - The size of the font.
    /// * `caret_position` - The position in millimeters where the text should begin to be drawn.
    ///
    /// This function might appear to have too many arguments, but this is on purpose in order to keep the
    /// API or this library quite on the simpler side. Any external algorithm for layouting text should
    /// take into consideration the way in which text is inserted into the PDF. Checkout the PDF specification for more details.
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
    ) -> Result<(), ContextError> {
        // Retrieve the font at the given font index
        let font = self.get_font(font_index)?.1.clone(); // TODO: I shouldn't have to clone the font data

        // Insert the required operations for writing text to the layer
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
                }), // Set the position where the text begins to be written
                lopdf::content::Operation::new("rg", {
                    let [r, g, b] = color;
                    vec![r, g, b].into_iter().map(lopdf::Object::Real).collect()
                }),
                // Set the filling color of the text
            ],
        )?;

        let mut glyph_id_list = Vec::<u16>::new();
        // Normalize the text in the NFC form before processing
        for character in text.nfc() {
            // Retrieve the glyph ID of each character from the font
            if let Some(glyph_id) = font.ttf_face.glyph_id(character) {
                glyph_id_list.push(glyph_id);
            } else {
                // Otherwise, if the character is not present in the font, log the event
                log::warn!("Unable to find the character {:?} in the font", character)
            }
        }

        // Convert each glyph ID into the required byte format which is accepted by the PDF specification
        let glyph_id_bytes = glyph_id_list
            .iter()
            .flat_map(|x| vec![(x >> 8) as u8, (x & 255) as u8])
            .collect::<Vec<u8>>();
        // Insert the actual text content into the PDF document as bytes.
        self.add_operations_to_layer_in_page(
            layer_index,
            page_index,
            vec![lopdf::content::Operation::new(
                "Tj",
                vec![lopdf::Object::String(
                    glyph_id_bytes,
                    lopdf::StringFormat::Hexadecimal,
                )],
            )],
        )?;

        // Finalize the writing operation by including the text ending section
        self.add_operations_to_layer_in_page(
            layer_index,
            page_index,
            vec![lopdf::content::Operation::new("ET", vec![])],
        )?;

        // Return that no error has happened
        Ok(())
    }

    /// Write the operations so far specified to the PDF file and finalize it.
    ///
    /// # Disclaimer
    ///
    /// One mandatory argument needed by the PDF specification is the instance ID, which needs to be a
    /// 32 characters-long string. Also, saving the PDF to an actual document is a complicated process, so I recommend
    /// end-users of this library to even tinker with this function and adapt it to their needs.
    /// The output of this function is not optimized and should be fed into either ghostscript or `ps2pdf`.
    pub fn write_all(&mut self, instance_id: String) -> Result<(), ContextError> {
        use lopdf::Object::*;
        use lopdf::StringFormat::*;

        // Construct all the general info that the PDF document needs in order to be parsed correctly
        // and insert it into the PDF document itself
        // TODO(ghovax): The user might want to choose all these parameters.
        let document_info = lopdf::Dictionary::from_iter(vec![
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
        ]);
        let document_info_id = self.inner_document.add_object(Dictionary(document_info));

        // Construct the catalog, required by the PDF specification
        let pages_id = self.inner_document.new_object_id();
        let mut catalog = lopdf::Dictionary::from_iter(vec![
            ("Type", "Catalog".into()),
            ("PageLayout", "OneColumn".into()),
            ("PageMode", "UseNone".into()),
            ("Pages", Reference(pages_id)),
        ]);

        // Begin constructing the pages dictionary
        let mut pages = lopdf::Dictionary::from_iter(vec![
            ("Type", "Pages".into()),
            ("Count", Integer(self.pages.len() as i64)),
        ]);

        // Construct the dictionary for clarifying the OCG usage and insert it into the PDF document
        let ocg_usage_dictionary = lopdf::Dictionary::from_iter(vec![
            ("Type", Name("OCG".into())),
            (
                "CreatorInfo",
                Dictionary(lopdf::Dictionary::from_iter(vec![
                    ("Creator", String("Adobe Illustrator 14.0".into(), Literal)), // TODO: What the hell is this?
                    ("Subtype", Name("Artwork".into())),
                ])),
            ),
        ]);
        let usage_ocg_dictionary_id = self.inner_document.add_object(ocg_usage_dictionary);

        // Construct the array which explains the intents
        let intent_array = Array(vec![Name("View".into()), Name("Design".into())]);
        let intent_array_id = self.inner_document.add_object(intent_array);

        let page_layer_numbers_and_names: Vec<(usize, Vec<::std::string::String>)> = self
            .pages
            .iter()
            .map(|page| {
                // For each page in our PDF document, retrieve the number of the page and the
                // names of the layers composing it in order to construct the OCG list
                (
                    page.number,
                    page.layers.iter().map(|layer| layer.name.clone()).collect(),
                )
            })
            .collect();

        // For each page number and layer name in each page...
        let ocg_association: Vec<(usize, Vec<(usize, lopdf::Object)>)> =
            page_layer_numbers_and_names
                .into_iter()
                .map(|(page_index, layer_names)| {
                    // Collect the layer index and the reference to OCG dictionary just inserted into the document
                    let layer_indices_and_dictionary_references = layer_names
                        .into_iter()
                        .enumerate()
                        .map(|(layer_index, layer_name)| {
                            // Insert the OCG dictionary with the intents, layer name and usage into the PDF document
                            let ocg_dictionary = lopdf::Dictionary::from_iter(vec![
                                ("Type", Name("OCG".into())),
                                ("Name", String(layer_name.into(), Literal)),
                                ("Intent", Reference(intent_array_id)),
                                ("Usage", Reference(usage_ocg_dictionary_id)),
                            ]);
                            let ocg_dictionary_id =
                                self.inner_document.add_object(Dictionary(ocg_dictionary));

                            (layer_index, Reference(ocg_dictionary_id))
                        })
                        .collect();

                    // For each page index, collect the layer indices and the reference to OCG dictionaries inserted into the PDF document
                    (page_index, layer_indices_and_dictionary_references)
                })
                .collect();

        // For each layer present in the OCG association just constructed, retrieve each object
        let ocg_dictionary_references: Vec<lopdf::Object> = ocg_association
            .iter()
            .flat_map(|(_, layers)| {
                layers
                    .iter()
                    .map(|(_, dictionary_reference)| dictionary_reference.clone())
            })
            .collect();

        // Update the PDF catalog with the OCGs just inserted into the document
        catalog.set(
            "OCProperties",
            Dictionary(lopdf::Dictionary::from_iter(vec![
                ("OCGs", Array(ocg_dictionary_references.clone())),
                (
                    "D",
                    Dictionary(lopdf::Dictionary::from_iter(vec![
                        ("Order", Array(ocg_dictionary_references.clone())),
                        ("RBGroups", Array(vec![])),
                        ("ON", Array(ocg_dictionary_references)),
                    ])),
                ),
            ])),
        );

        // Save the catalog after inserting it into the PDF document
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
                String(self.identifier.clone().into_bytes(), Literal),
                String(instance_id.as_bytes().to_vec(), Literal),
            ]),
        );

        // Load the set fonts and insert them into the PDF document
        let fonts_dictionary = self.insert_fonts_into_document();
        let fonts_dictionary_id = self.inner_document.add_object(fonts_dictionary);

        let mut page_ids = Vec::<lopdf::Object>::new();

        // For each page present in the document...
        for (index, page) in self.pages.iter_mut().enumerate() {
            // Construct the dictionary which specifies all the page information
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

            // If present, extend the page dictionary with further settings
            if let Some(extension) = &page.extend_with {
                for (key, value) in extension.iter() {
                    page_dictionary.set(key.to_vec(), value.clone())
                }
            }

            // Collect the layers of the OCG associated to the current document page
            let unmerged_layer = ocg_association.iter().find(|ocg| ocg.0 - 1 == index).ok_or({
                // If this operation fails, return an error with context
                let comparisons = ocg_association.iter().map(|ocg| ocg.0 - 1).collect::<Vec<_>>();
                ContextError::with_context(
                    format!("Unable to collect the resources needed for rendering the page: can't find {:?} in {:?}", index, comparisons),
                )
            })?;

            // Collect the streams and the resources associated to the current layer
            let (mut resource_dictionary, layer_streams) =
                page.collect_resources_and_streams(&mut self.inner_document, &unmerged_layer.1)?;

            // Set the fonts for the resource associated to the current layer, insert it into the PDF document
            // and then inserts the resource dictionary into the one for the pages
            resource_dictionary.set("Font", Reference(fonts_dictionary_id));
            let resources_page_id = self
                .inner_document
                .add_object(Dictionary(resource_dictionary));
            page_dictionary.set("Resources", Reference(resources_page_id));

            // Merge all streams of the individual layers into one unified stream, then
            // it into the PDF document as a whole by setting the "Contents" field
            let mut merged_layer_streams = Vec::<u8>::new();
            for mut stream in layer_streams {
                merged_layer_streams.append(&mut stream.content);
            }
            let merged_layer_stream =
                lopdf::Stream::new(lopdf::Dictionary::new(), merged_layer_streams);
            let page_content_id = self.inner_document.add_object(merged_layer_stream);
            page_dictionary.set("Contents", Reference(page_content_id));

            // Inserts the page dictionary into the document and save the associated reference
            let page_id = self.inner_document.add_object(page_dictionary);
            page_ids.push(Reference(page_id))
        }

        // Use all the collected page references in order to set the "Kids" field of the PDF document
        // and then insert the pages dictionary into the document itself as a last operation
        pages.set::<_, lopdf::Object>("Kids".to_string(), page_ids.into());
        self.inner_document
            .objects
            .insert(pages_id, Dictionary(pages));

        Ok(())
    }

    /// Optimize the PDF document (only superficially).
    pub fn optimize(&mut self) {
        self.inner_document.prune_objects();
        self.inner_document.delete_zero_length_streams();
        self.inner_document.renumber_objects();
        self.inner_document.compress();
    }

    /// Save the `PdfDocument` to bytes in order for it to be written to a file or further processed.
    pub fn save_to_bytes(&mut self) -> Result<Vec<u8>, ContextError> {
        let mut pdf_document_bytes = Vec::new();
        let mut writer = BufWriter::new(&mut pdf_document_bytes);
        self.inner_document.save_to(&mut writer).map_err(|error| {
            ContextError::with_error("Error while saving the PDF document to bytes", &error)
        })?;
        mem::drop(writer);

        Ok(pdf_document_bytes)
    }

    /// Converts the fonts into a dictionary and inserts them into the document.
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

    /// This function is responsible for adding the given operations to the specified layer and page.
    fn add_operations_to_layer_in_page(
        &mut self,
        layer_index: usize,
        page_index: usize,
        operations: Vec<lopdf::content::Operation>,
    ) -> Result<(), ContextError> {
        let pdf_layer_reference = self.get_mut_layer_in_page(layer_index, page_index)?;
        pdf_layer_reference.operations.extend(operations);

        Ok(())
    }

    // Retrieve the font at the given font index.
    fn get_font(&mut self, font_index: usize) -> Result<&((u32, u16), Font), ContextError> {
        self.fonts
            .get(&format!("F{font_index}"))
            .ok_or(ContextError::with_context(format!(
                "Failed to find font {} into the fonts map",
                font_index
            )))
    }

    // Retrieve the specified layer in the given page via the respective indices.
    fn get_mut_layer_in_page(
        &mut self,
        layer_index: usize,
        page_index: usize,
    ) -> Result<&mut PdfLayer, ContextError> {
        let pdf_page = self
            .pages
            .get_mut(page_index)
            .ok_or(ContextError::with_context(format!(
                "Failed to find the page with index {}",
                page_index
            )))?;
        let pdf_layer = pdf_page
            .layers
            .get_mut(layer_index)
            .ok_or(ContextError::with_context(format!(
                "Failed to find the layer with index {}",
                layer_index
            )))?;

        Ok(pdf_layer)
    }
}

type GlyphId = u32;
type UnicodeCodePoint = u32;
type CmapBlock = Vec<(GlyphId, UnicodeCodePoint)>;

/// Generates a CMAP (character map) from valid cmap blocks by iterating over them. This function adheres to
/// the PDF specification by employing a predefined beginning and end section which is inserted at compile time.
fn generate_cid_to_unicode_map(face_name: String, all_cmap_blocks: Vec<CmapBlock>) -> String {
    // Initialize the mapping with the predefined beginning (this is a text section)
    let mut cid_to_unicode_map =
        format!(include_str!("../assets/gid_to_unicode_beg.txt"), face_name);

    // For each cmap block present into the given list of blocks, which isn't empty or doesn't exceed 100 elements in length...
    for cmap_block in all_cmap_blocks
        .into_iter()
        .filter(|block| !block.is_empty() || block.len() < 100)
    {
        // Configure the mapping so that a cmap block section of data is initialized
        cid_to_unicode_map.push_str(format!("{} beginbfchar\r\n", cmap_block.len()).as_str());
        for (glyph_id, unicode) in cmap_block {
            // Add all data present in the block as expected by the PDF specification
            cid_to_unicode_map.push_str(format!("<{glyph_id:04x}> <{unicode:04x}>\n").as_str());
        }
        // Terminate the block
        cid_to_unicode_map.push_str("endbfchar\r\n");
    }

    // Finalize the mapping between the character IDs and the Unicode characters
    cid_to_unicode_map.push_str(include_str!("../assets/gid_to_unicode_end.txt"));

    cid_to_unicode_map
}

/// Formats the given time so that it matches what the PDF specification expects.
/// An example of it is the following: D:20170505150224+02'00'.
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

/// This function is used to optimize the PDF file by running ghostscript on it. The command which is run
/// is the following:
///
/// ```bash
/// $ gs -sDEVICE=pdfwrite -dCompatibilityLevel=1.4 -dPDFSETTINGS=/ebook -dNOPAUSE -dQUIET -dBATCH -sOutputFile=output.pdf input.pdf
/// ```
///
/// What we do though is to create an intermediate `.swp` file and then rename it to the expected one.
/// This is procedure of creating an intermediate file is a workaround to a limitation of the shell.
///
/// # Arguments
///
/// * `pdf_path` - The path to the PDF file to be optimized.
pub fn optimize_pdf_file_with_gs(pdf_path: &str) -> Result<(), ContextError> {
    // Run ghostscript to optimize the PDF file
    // $ gs -sDEVICE=pdfwrite -dCompatibilityLevel=1.4 -dPDFSETTINGS=/ebook -dNOPAUSE -dQUIET -dBATCH -sOutputFile=output.pdf input.pdf
    let child = std::process::Command::new("gs")
        .arg("-sDEVICE=pdfwrite")
        .arg("-dCompatibilityLevel=1.5")
        .arg("-dPDFSETTINGS=/ebook")
        .arg("-dNOPAUSE")
        .arg("-dQUIET")
        .arg("-dBATCH")
        .arg(format!("-sOutputFile={}.swp", pdf_path))
        .arg(pdf_path)
        .spawn();
    match child {
        Ok(mut child) => {
            let status = child.wait().map_err(|error| {
                ContextError::with_error("Unable to wait for the gs command execution", &error)
            })?;
            if !status.success() {
                return Err(ContextError::with_context(format!(
                    "gs failed with status {:?}",
                    status
                )));
            }
            std::fs::rename(format!("{}.swp", pdf_path), pdf_path).map_err(|error| {
                ContextError::with_error("Unable to rename the optimized PDF file", &error)
            })?;
        }
        Err(error) => {
            return Err(ContextError::with_error(
                "Unable to run the gs command",
                &error,
            ));
        }
    }

    Ok(())
}

/// This function is used to optimize the PDF file by running ps2pdf on it.
/// An intermediate file with the `.swp` extension is created and then renamed immediately
/// to the expected one, which is the given path.
///
/// # Arguments
///
/// * `pdf_path` - The path to the PDF file to be optimized.
///
/// This is procedure of creating an intermediate file is a workaround to a limitation of the shell.
pub fn optimize_pdf_file_with_ps2pdf(pdf_path: &str) -> Result<(), ContextError> {
    // Run ps2pdf to optimize the PDF file
    let child = std::process::Command::new("ps2pdf")
        .arg(pdf_path)
        .arg(format!("{}.swp", pdf_path))
        .spawn();
    match child {
        Ok(mut child) => {
            let status = child.wait().map_err(|error| {
                ContextError::with_error("Unable to wait for the ps2pdf command execution", &error)
            })?;
            if !status.success() {
                return Err(ContextError::with_context(format!(
                    "ps2pdf failed with status {:?}",
                    status
                )));
            }
            std::fs::rename(format!("{}.swp", pdf_path), pdf_path).map_err(|error| {
                ContextError::with_error("Unable to rename the optimized PDF file", &error)
            })?;
        }
        Err(error) => {
            return Err(ContextError::with_error(
                "Unable to run the ps2pdf command",
                &error,
            ));
        }
    }

    Ok(())
}
