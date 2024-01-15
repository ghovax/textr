use anyhow::Ok;
use bytemuck::{Pod, Zeroable};
use freetype::{Bitmap, GlyphSlot};
use glm::IVec2;
use image::{
    DynamicImage, EncodableLayout, GenericImageView, GrayImage, ImageBuffer, ImageFormat, Luma,
    Rgba32FImage, RgbaImage,
};
use itertools::Itertools;
use nalgebra_glm as glm;
use sendable::SendRc;
use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    ops::Deref,
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use ultraviolet::Vec2;
use unicode_normalization::UnicodeNormalization;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::*,
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopWindowTarget},
    window::{Window, WindowBuilder},
};

use wgpu::{util::DeviceExt, InstanceFlags};
use winit_input_helper::WinitInputHelper;

#[derive(Debug)]
pub struct Texture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl Texture {
    pub fn from_bitmap_data(
        render_state: &RenderState,
        bitmap_data: BitmapData,
        label: Option<&str>,
    ) -> anyhow::Result<Self> {
        let size = wgpu::Extent3d {
            width: bitmap_data.width,
            height: bitmap_data.rows,
            depth_or_array_layers: 1,
        };
        let format = wgpu::TextureFormat::R8Unorm;
        let texture = render_state
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label,
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[format],
            });

        render_state.queue.write_texture(
            wgpu::ImageCopyTexture {
                aspect: wgpu::TextureAspect::All,
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            &bitmap_data.buffer,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(bitmap_data.pitch),
                rows_per_image: Some(bitmap_data.rows),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(format),
            ..Default::default()
        });
        let sampler = render_state
            .device
            .create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });

        Ok(Self {
            texture,
            view,
            sampler,
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub texture_coordinates: [f32; 2],
}

impl Vertex {
    fn descriptor<'a>() -> wgpu::VertexBufferLayout<'a> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

pub struct RenderState {
    window: Window,
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    configuration: wgpu::SurfaceConfiguration,
    physical_size: winit::dpi::PhysicalSize<u32>,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    render_pipeline: wgpu::RenderPipeline,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_groups: HashMap<char, wgpu::BindGroup>,
    vertex_buffers: Vec<CharacterData>,
}

struct CharacterData {
    character: char,
    vertex_buffer: wgpu::Buffer,
}

impl RenderState {
    // Creating some of the wgpu types requires async code
    fn new(window: Window) -> Self {
        // The instance is a handle to our GPU
        // Backends::all => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            dx12_shader_compiler: Default::default(),
            ..Default::default()
        });

        // # Safety
        //
        // The surface needs to live as long as the window that created it.
        // State owns the window so this should be safe.
        let surface = unsafe { instance.create_surface(&window) }.unwrap();

        let adapter = pollster::block_on(async {
            instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                })
                .await
        })
        .unwrap();

        let (device, queue) = pollster::block_on(async {
            adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        features: wgpu::Features::empty(), // wgpu::Features::CONSERVATIVE_RASTERIZATION,
                        // WebGL doesn't support all of wgpu's features, so if
                        // we're building for the web we'll have to disable some.
                        limits: if cfg!(target_arch = "wasm32") {
                            wgpu::Limits::downlevel_webgl2_defaults()
                        } else {
                            wgpu::Limits::default()
                        },
                        label: None,
                    },
                    None, // Trace path
                )
                .await
        })
        .unwrap();

        let surface_capabilities = surface.get_capabilities(&adapter);
        // Shader code assumes an sRGB surface texture. Using a different
        // one will result all the colors coming out darker. If you want to support non
        // sRGB surfaces, you'll need to account for that when drawing to the frame.
        let surface_format = surface_capabilities
            .formats
            .iter()
            .copied()
            .filter(|format| format.is_srgb())
            .next()
            .unwrap_or(surface_capabilities.formats[0]);

        let inner_size = window.inner_size();
        let configuration = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: inner_size.width,
            height: inner_size.height,
            present_mode: surface_capabilities.present_modes[0], // Could be surface_capabilities.present_modes[0] but Intel Arc A770 go brrr.
            alpha_mode: surface_capabilities.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &configuration);

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
                label: Some("Texture Bind Group Layout"),
            });

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let camera_uniform = CameraUniform {
            projection_matrix: glm::ortho(
                0.0,
                inner_size.width as f32,
                0.0,
                inner_size.height as f32,
                -1.0,
                1.0,
            )
            .into(),
        };

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("Camera Bind Group Layout"),
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("Camera Bind Group"),
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&texture_bind_group_layout, &camera_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vertex_main",
                buffers: &[Vertex::descriptor()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fragment_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: configuration.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw, // 2.
                cull_mode: Some(wgpu::Face::Back),
                // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        Self {
            window,
            surface,
            device,
            queue,
            configuration,
            physical_size: inner_size,
            texture_bind_group_layout,
            camera_buffer,
            camera_bind_group,
            render_pipeline,
            texture_bind_groups: HashMap::new(),
            vertex_buffers: Vec::new(),
        }
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    // Return true if an event has been captured
    fn input(&mut self, event: &Event<()>) -> bool {
        false
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[
                    // This is what @location(0) in the fragment shader targets
                    Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 1.0,
                                g: 1.0,
                                b: 1.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            render_pass.set_pipeline(&self.render_pipeline);

            // TODO(*)
            let mut character_count = 0;
            for (
                character_index,
                CharacterData {
                    character,
                    vertex_buffer,
                },
            ) in self.vertex_buffers.iter().enumerate()
            {
                let bind_group = match self.texture_bind_groups.get(character) {
                    Some(bind_group) => bind_group,
                    None => {
                        if *character != ' ' {
                            log::error!("No texture bind group for character {}", character);
                            return Err(wgpu::SurfaceError::OutOfMemory);
                        }
                        continue;
                    }
                };
                render_pass.set_bind_group(0, bind_group, &[]);

                render_pass.set_bind_group(1, &self.camera_bind_group, &[]);
                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));

                character_count += 1;
                render_pass.draw(6 * (character_count - 1)..6 * character_count, 0..1);
            }
            log::debug!("Finished drawing the characters");
        }

        // `submit` will accept anything that implements IntoIter
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        std::result::Result::Ok(())
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.physical_size = new_size;
            self.configuration.width = new_size.width;
            self.configuration.height = new_size.height;
            self.surface.configure(&self.device, &self.configuration);
        }
    }
}

#[derive(Clone, Copy)]
pub enum WindowMode {
    Windowed(u32, u32),
    Fullscreen,
}

enum CustomEvent {
    SaveFile,
    ReceivedCharacter(char),
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    projection_matrix: [[f32; 4]; 4],
}

fn main() {
    env_logger::init();

    let event_loop: EventLoop<_> = EventLoopBuilder::<CustomEvent>::with_user_event().build();
    let mut builder = WindowBuilder::new().with_title("TeXtr");

    let mut window_size = (1800, 600);
    let window_mode = WindowMode::Windowed(window_size.0, window_size.1);

    match window_mode {
        WindowMode::Windowed(width, height) => {
            let monitor = event_loop.primary_monitor().unwrap();
            let size = monitor.size();
            let position = PhysicalPosition::new(
                (size.width - width) as i32 / 2,
                (size.height - height) as i32 / 2,
            );
            builder = builder
                .with_inner_size(PhysicalSize::new(width, height))
                .with_position(position);
        }
        WindowMode::Fullscreen => {
            builder = builder.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }
    };

    let window = builder.build(&event_loop).unwrap();

    let mut render_state = RenderState::new(window);
    // let mut input_helper = WinitInputHelper::new();

    let (texture_bind_group_request_sender, texture_bind_group_request_receiver) =
        std::sync::mpsc::channel();
    let (vertex_buffer_request_sender, vertex_buffer_request_receiver) = std::sync::mpsc::channel();

    // Logic events thread

    let (custom_event_sender, custom_event_receiver) = std::sync::mpsc::channel();
    let _logic_events_thread = std::thread::spawn(move || {
        let document_path = "assets/textTest.txt";
        let font_path = "fonts/cmunrm.ttf";

        log::info!(
            "The document '{}' will be loaded with the font '{}'",
            document_path,
            font_path
        );

        // Load the library Freetype for using the font glyphs

        let library: freetype::Library = freetype::Library::init().unwrap();

        // Load the text from the file path given
        let mut text = std::fs::read_to_string(document_path).unwrap();
        log::debug!("Imported the text: {:?}", text);
        let font_face = library.new_face(font_path, 0).unwrap();

        // Calculate the line length based on the average character advance

        let font_size = 50.0; // Arbitrary unit of measurement
        font_face.set_pixel_sizes(0, font_size as u32).unwrap(); // TODO: `pixel_width` is 0? Probably it means "take the default one"

        let margins = Margins {
            top: 60.0,
            bottom: 60.0,
            left: 30.0,
            right: 30.0,
        };

        let mut line_height = window_size.1 as f32 - font_size - margins.top;

        let mut character_advances = Vec::new();
        let mut space_character_advance = 0;
        for character in
            r#" abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890'`~<,.>/?"\|;:]}[{=+"#
                .chars()
                .nfc()
        {
            font_face
                .load_char(character as usize, freetype::face::LoadFlag::RENDER)
                .unwrap();
            let glyph = font_face.glyph();
            if character == ' ' {
                space_character_advance = glyph.advance().x as u32;
            }

            character_advances.push((glyph.advance().x as u32) >> 6); // Bitshift by 6 to convert in pixels
        }
        let average_character_advance = (character_advances.iter().sum::<u32>() as f32
            / character_advances.len() as f32) as u32;

        let mut average_line_length = ((window_size.0 as f32 - margins.left - margins.right)
            / average_character_advance as f32) as u32;
        log::debug!("Average line length in characters: {}", average_line_length);

        // Load the characters in the text from the chosen font

        let mut characters_map: HashMap<char, Character> = HashMap::new();

        for character_to_load in text.nfc().unique() {
            font_face
                .load_char(character_to_load as usize, freetype::face::LoadFlag::RENDER)
                .unwrap();
            let glyph = font_face.glyph();
            glyph.render_glyph(freetype::RenderMode::Mono).unwrap();
            let glyph_bitmap = glyph.bitmap();
            // log::trace!("Pixel mode of the gyph: {:?}", glyph_bitmap.pixel_mode());
            let character = Character {
                size: IVec2::new(glyph_bitmap.width(), glyph_bitmap.rows()),
                bearing: IVec2::new(glyph.bitmap_left(), glyph.bitmap_top()),
                advance: glyph.advance().x as u32,
            };
            characters_map.insert(character_to_load, character);
            if glyph_bitmap.width() == 0 || glyph_bitmap.rows() == 0 {
                log::debug!(
                    "Skipped the initial loading of the character {:?}",
                    character_to_load
                );
                continue;
            }
            texture_bind_group_request_sender
                .send(TextureBindGroupRequest {
                    character_to_load,
                    bitmap_data: BitmapData {
                        width: glyph_bitmap.width() as u32,
                        rows: glyph_bitmap.rows() as u32,
                        buffer: glyph_bitmap.buffer().to_vec(),
                        pitch: glyph_bitmap.pitch() as u32,
                    },
                })
                .unwrap();
        }

        log::debug!(
            "Characters requested to be loaded: {}",
            characters_map.len()
        );

        let wrapped_text: Vec<_> = textwrap::wrap(&text, average_line_length as usize)
            .iter()
            .map(|line| line.to_string())
            .collect();

        // TODO(*): The caret is the '|' character, at least for now. Load it.

        let mut input_buffer = Vec::new();

        // Assign a value to cap the ticks-per-second
        let tps_cap: Option<u32> = Some(10);

        let desired_frame_time = tps_cap.map(|tps| Duration::from_secs_f64(1.0 / tps as f64));

        loop {
            let frame_start_time = Instant::now();
            log::trace!("Logic event loop frame start time: {:?}", frame_start_time);

            use std::result::Result::Ok;
            while let Ok(custom_event) = custom_event_receiver.try_recv() {
                match custom_event {
                    CustomEvent::SaveFile => {
                        // TODO(*): Save the text to the file path given, but if it fails, the file is overwritten and
                        // all content is lost
                        let mut file = File::create(document_path).unwrap();
                        file.write_all(text.clone().as_bytes()).unwrap();
                        log::info!(
                            "The document has been successfully saved to the path: {:?}",
                            document_path
                        );
                    }
                    CustomEvent::ReceivedCharacter(character) => {
                        input_buffer.push(character);
                    }
                }
            }

            // Each iteration of the loop, wrap the text in lines...
            let wrapped_text: Vec<_> = textwrap::wrap(&text, average_line_length as usize)
                .iter()
                .map(|line| line.to_string())
                .collect();

            // ...then calculate their lengths in order to set the caret index, used to find the caret
            // in the text to perform the usual insertions/removals/edits.
            let line_lengths = wrapped_text.iter().map(|line| line.len());

            // Insert the text in the input buffer at the caret position and load each newly present
            // character in the input.
            for input_character in input_buffer.drain(..) {
                text.push(input_character);

                let character = input_character.nfc().next().unwrap();
                if characters_map.get(&character).is_none() {
                    // TODO(*): Load the character if uninitialized
                }
            }

            // Vertices placing algorithm

            let mut raw_vertices_data = Vec::new();

            // ...then, for each line in the wrapped text...
            for (line_index, line) in wrapped_text.iter().enumerate() {
                let mut horizontal_origin = margins.left;

                // ...draw each character therein present...
                for (character_index, character) in line.chars().enumerate() {
                    // ...and skip the space character
                    if character == ' ' {
                        horizontal_origin += (space_character_advance >> 6) as f32;

                        continue;
                    }
                    let character_data = match characters_map.get(&character) {
                        Some(character) => character,
                        None => {
                            log::error!("Unable to retrieve the character {:?} from the map, it is not (at least yet) loaded", character);
                            return;
                        }
                    };

                    let x = horizontal_origin + character_data.bearing.x as f32;
                    let y = line_height - (character_data.size.y - character_data.bearing.y) as f32;

                    let width = character_data.size.x as f32;
                    let height = character_data.size.y as f32;

                    let raw_vertex_data = [
                        Vertex {
                            position: [x, y + height],
                            texture_coordinates: [0.0, 0.0],
                        },
                        Vertex {
                            position: [x, y],
                            texture_coordinates: [0.0, 1.0],
                        },
                        Vertex {
                            position: [x + width, y],
                            texture_coordinates: [1.0, 1.0],
                        },
                        Vertex {
                            position: [x, y + height],
                            texture_coordinates: [0.0, 0.0],
                        },
                        Vertex {
                            position: [x + width, y],
                            texture_coordinates: [1.0, 1.0],
                        },
                        Vertex {
                            position: [x + width, y + height],
                            texture_coordinates: [1.0, 0.0],
                        },
                    ];

                    raw_vertices_data.push(raw_vertex_data);

                    // Move the origin by the character advance in order to draw the characters side-to-side.
                    horizontal_origin += (character_data.advance >> 6) as f32;
                    // Bitshift by 6 to get value in pixels (2^6 = 64)
                }

                // Move the line height below by the font size when each line is finished
                line_height -= font_size;
            }

            vertex_buffer_request_sender
                .send(VertexBufferRequest {
                    vertices: raw_vertices_data,
                    text: text.clone(),
                })
                .unwrap();

            // In the end, reset the line height to its original value
            line_height = window_size.1 as f32 - margins.top - font_size;

            // Cap the ticks-per-second
            if let Some(desired_frame_time) = desired_frame_time {
                let elapsed_time = frame_start_time.elapsed();
                if elapsed_time < desired_frame_time {
                    thread::sleep(desired_frame_time - elapsed_time);
                }
            }
        }
    });

    // std::thread::sleep(Duration::from_secs(1));

    event_loop.run(move |event, target, control_flow| {
        use std::result::Result::Ok;
        while let Ok(TextureBindGroupRequest {
            character_to_load,
            bitmap_data,
        }) = texture_bind_group_request_receiver.try_recv()
        {
            // TODO(*): ...load it
            // TODO(!): Handle the error, don't just unwrap it
            let texture = Texture::from_bitmap_data(
                &render_state,
                bitmap_data,
                Some(format!("Glyph Texture {:?}", character_to_load).as_str()),
            )
            .unwrap();

            let texture_bind_group =
                render_state
                    .device
                    .create_bind_group(&wgpu::BindGroupDescriptor {
                        layout: &render_state.texture_bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&texture.view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&texture.sampler),
                            },
                        ],
                        label: Some(
                            format!("Glyph {:?} Texture Bind Group", character_to_load).as_str(),
                        ),
                    });
            match render_state
                .texture_bind_groups
                .insert(character_to_load, texture_bind_group)
            {
                Some(_) => {
                    log::warn!(
                        "The texture bind group for the glyph {:?} has already been loaded",
                        character_to_load
                    );
                }
                None => (),
            };
            // log::debug!("Loaded the texture bind group for the glyph {:?}", character_to_load);
        }

        if let Ok(VertexBufferRequest { vertices, text }) =
            vertex_buffer_request_receiver.try_recv()
        {
            // Clear the vertex buffers
            render_state.vertex_buffers.clear();

            for (character_index, character) in text.chars().enumerate() {
                let vertex_buffer =
                    render_state
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some(&format!("Glyph {:?} Vertex Buffer", character)),
                            contents: bytemuck::cast_slice(&vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                render_state.vertex_buffers.push(CharacterData {
                    character,
                    vertex_buffer,
                });
                // log::debug!("Created the vertex buffer for the glyph {:?} at index {} in the text", character, character_index);
            }
        }

        let mut ctrl_is_pressed = false;
        match event {
            Event::WindowEvent {
                ref event,
                window_id,
            } if window_id == render_state.window().id() => match event {
                WindowEvent::ModifiersChanged(modifiers_state) => match *modifiers_state {
                    ModifiersState::CTRL => {
                        ctrl_is_pressed = true;
                    }
                    _ => (),
                },
                _ => (),
            },
            _ => (),
        };

        match event {
            Event::WindowEvent {
                ref event,
                window_id,
            } if window_id == render_state.window().id() => match event {
                WindowEvent::CloseRequested
                | WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            state: ElementState::Pressed,
                            virtual_keycode: Some(VirtualKeyCode::Escape),
                            ..
                        },
                    ..
                } => control_flow.set_exit(),
                WindowEvent::Resized(physical_size) => {
                    render_state.resize(*physical_size);
                }
                WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                    render_state.resize(**new_inner_size);
                }
                WindowEvent::ReceivedCharacter(character) => {
                    custom_event_sender
                        .send(CustomEvent::ReceivedCharacter(*character))
                        .unwrap();
                }
                WindowEvent::KeyboardInput { input, .. } => match input {
                    // If the user presses Ctrl + S key, save the document
                    KeyboardInput {
                        state: ElementState::Pressed,
                        virtual_keycode: Some(VirtualKeyCode::S),
                        ..
                    } => {
                        if ctrl_is_pressed {
                            custom_event_sender.send(CustomEvent::SaveFile).unwrap();
                        }
                    }
                    _ => (),
                },
                _ => (),
            },
            Event::RedrawRequested(window_id) if window_id == render_state.window().id() => {
                // system_state.input(&input_helper, render_state.view.x, render_state.view.y);

                match render_state.render() {
                    std::result::Result::Ok(_) => {}
                    // Reconfigure the surface if lost
                    Err(wgpu::SurfaceError::Lost) => {
                        render_state.resize(render_state.physical_size)
                    }
                    // The system is out of memory, we should probably quit
                    Err(wgpu::SurfaceError::OutOfMemory) => control_flow.set_exit(),
                    // All other errors (Outdated, Timeout) should be resolved by the next frame
                    Err(error) => log::error!("{:?}", error),
                }
            }
            Event::MainEventsCleared => {
                // RedrawRequested will only trigger once, unless we manually request it.
                render_state.window().request_redraw();
            }
            _ => (),
        }
    });
}

#[derive(Debug, Clone, Default)]
struct Character {
    size: IVec2,    // Size of glyph
    bearing: IVec2, // Offset from baseline to left/top of glyph
    advance: u32,   // Offset to advance to the next glyph
}

#[derive(Debug)]
struct Caret {
    position: IVec2,
}

/// Represents margins around a block of text.
#[derive(Debug, Clone, Copy)]
pub struct Margins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

struct TextureBindGroupRequest {
    character_to_load: char,
    bitmap_data: BitmapData,
}

pub struct BitmapData {
    width: u32,
    rows: u32,
    buffer: Vec<u8>,
    pitch: u32,
}

struct VertexBufferRequest {
    vertices: Vec<[Vertex; 6]>,
    text: String,
}
