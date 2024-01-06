use anyhow::Ok;
use bytemuck::{Pod, Zeroable};
use glm::IVec2;
use image::GenericImageView;
use nalgebra_glm as glm;
use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    ops::Deref,
    rc::Rc,
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

#[repr(C)]
#[derive(Copy, Clone)]
pub struct View {
    position: Vec2,
    scale: f32,
    x: u16,
    y: u16,
}

unsafe impl Pod for View {}
unsafe impl Zeroable for View {}

#[derive(Debug)]
pub struct Texture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl Texture {
    pub fn from_bytes(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bytes: &[u8],
        label: &str,
    ) -> anyhow::Result<Self> {
        let image = image::load_from_memory(bytes)?;
        Self::from_image(device, queue, &image, Some(label))
    }

    pub fn from_glyph(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        glyph: &freetype::GlyphSlot,
        label: Option<&str>,
    ) -> Self {
        let dimensions = (glyph.bitmap().width() as u32, glyph.bitmap().rows() as u32);
        let size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };

        let format = wgpu::TextureFormat::Rgba8UnormSrgb; // TODO: Maybe replace?
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                aspect: wgpu::TextureAspect::All,
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            glyph.bitmap().buffer(),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * dimensions.0),
                rows_per_image: Some(dimensions.1),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
        }
    }

    pub fn from_image(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &image::DynamicImage,
        label: Option<&str>,
    ) -> anyhow::Result<Self> {
        let rgba = image.to_rgba8();
        let dimensions = image.dimensions();

        let size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                aspect: wgpu::TextureAspect::All,
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            &rgba,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * dimensions.0),
                rows_per_image: Some(dimensions.1),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
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
#[derive(Clone, Copy)]
pub struct Vertex {
    pub position: Vec2,
    pub color: u32,
}

unsafe impl Pod for Vertex {}
unsafe impl Zeroable for Vertex {}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Uint32];

    fn descriptor<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

struct RenderState {
    window: Window,
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    configuration: wgpu::SurfaceConfiguration,
    physical_size: winit::dpi::PhysicalSize<u32>,
    vertices: u32,
    vertex_buffer: wgpu::Buffer,
    render_pipeline: wgpu::RenderPipeline,
    view: View,
    view_buffer: wgpu::Buffer,
    view_bind_group: wgpu::BindGroup,
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
            present_mode: wgpu::PresentMode::AutoVsync, // Could be surface_capabilities.present_modes[0] but Intel Arc A770 go brrr.
            alpha_mode: surface_capabilities.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &configuration);

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let view = View {
            position: Vec2::zero(),
            scale: 1.0,
            x: configuration.width as u16,
            y: configuration.height as u16,
        };

        let view_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("View Buffer"),
            contents: bytemuck::cast_slice(&[view]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let view_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("View Bind Group Layout"),
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
            });

        let view_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("View Bind Group"),
            layout: &view_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view_buffer.as_entire_binding(),
            }],
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&view_bind_group_layout],
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
                    blend: Some(wgpu::BlendState::REPLACE),
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

        let vertices = 0;

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertex Buffer"),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST,
            size: 1 << 24,
            mapped_at_creation: false,
        });

        Self {
            window,
            surface,
            device,
            queue,
            configuration,
            physical_size: inner_size,
            vertices,
            vertex_buffer,
            render_pipeline,
            view,
            view_buffer,
            view_bind_group,
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
        self.queue
            .write_buffer(&self.view_buffer, 0, bytemuck::cast_slice(&[self.view]));

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
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
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
            render_pass.set_bind_group(0, &self.view_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.draw(0..self.vertices, 0..1);
        }

        // submit will accept anything that implements IntoIter
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

            self.view.x = new_size.width as u16;
            self.view.y = new_size.height as u16;
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
}

fn main() {
    env_logger::init();

    let event_loop: EventLoop<_> = EventLoopBuilder::<CustomEvent>::with_user_event().build();
    let mut builder = WindowBuilder::new().with_title("TeXtr");

    let mut window_size = (800, 600);
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
    let mut input_helper = WinitInputHelper::new();

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
    for normalized_utf8_character in
        r#"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890'`~<,.>/?"\|;:]}[{=+"#
            .chars()
            .nfc()
    {
        font_face
            .load_char(
                normalized_utf8_character as usize,
                freetype::face::LoadFlag::RENDER,
            )
            .unwrap();
        let glyph = font_face.glyph();

        character_advances.push((glyph.advance().x as u32) >> 6); // Bitshift by 6 to convert in pixels
    }
    let average_character_advance =
        (character_advances.iter().sum::<u32>() as f32 / character_advances.len() as f32) as u32;

    let mut average_line_length = ((window_size.0 as f32 - margins.left - margins.right)
        / average_character_advance as f32) as u32;
    log::trace!("Average line length in characters: {}", average_line_length);

    let projection_matrix = glm::ortho(
        0.0,
        window_size.0 as f32,
        0.0,
        window_size.1 as f32,
        -1.0,
        1.0,
    );

    // Load the characters in the text from the chosen font

    let mut characters: HashMap<char, Character> = HashMap::new();

    for normalized_utf8_character in text.nfc() {
        // If it hasn't already been loaded...
        if characters.get(&normalized_utf8_character).is_some() {
            continue;
        } else {
            // ...load it
            match load_character(
                &render_state.device,
                &render_state.queue,
                &font_face,
                &mut characters,
                normalized_utf8_character,
            ) {
                std::result::Result::Ok(_) => (),
                Err(error) => panic!("error loading the character: {:?}", error),
            };
        }
    }
    log::debug!(
        "Characters initially loaded from the text: {}",
        characters.len()
    );

    let wrapped_text: Vec<_> = textwrap::wrap(&text, average_line_length as usize)
        .iter()
        .map(|line| line.to_string())
        .collect();

    // The caret is the '|' character, at least for now. Load it.

    match load_character(
        &render_state.device,
        &render_state.queue,
        &font_face,
        &mut characters,
        '|',
    ) {
        std::result::Result::Ok(_) => (),
        Err(error) => panic!("error loading the caret character: {:?}", error),
    };
    let caret_character = characters.get(&'|').unwrap().clone();
    // Set the caret position at the end of the wrapped text
    let mut caret = Caret {
        character: caret_character,
        position: IVec2::new(
            wrapped_text.last().unwrap().chars().count() as i32,
            wrapped_text.len() as i32 - 1,
        ),
    };

    let (custom_event_sender, custom_event_receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // Assign a value to cap the ticks-per-second
        let tps_cap: Option<u32> = Some(60);

        let desired_frame_time = tps_cap.map(|tps| Duration::from_secs_f64(1.0 / tps as f64));

        loop {
            let frame_start_time = Instant::now();
            log::trace!("{:?}", frame_start_time);

            use std::result::Result::Ok;
            match custom_event_receiver.try_recv() {
                Ok(CustomEvent::SaveFile) => {
                    // TODO(!): If the file creation is unsuccessful, then the user will lose their data
                    let mut file = File::create(document_path).unwrap();
                    file.write_all(text.clone().as_bytes()).unwrap();
                    log::info!("The document has been successfully saved");
                }
                _ => (),
            }

            // Cap the ticks-per-second
            if let Some(desired_frame_time) = desired_frame_time {
                let elapsed_time = frame_start_time.elapsed();
                if elapsed_time < desired_frame_time {
                    thread::sleep(desired_frame_time - elapsed_time);
                }
            }
        }
    });

    event_loop.run(move |event, target, control_flow| {
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
                } => *control_flow = ControlFlow::Exit,
                WindowEvent::Resized(physical_size) => {
                    render_state.resize(*physical_size);
                }
                WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                    render_state.resize(**new_inner_size);
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

#[derive(Debug, Clone)]
struct Character {
    texture: sendable::SendRc<Texture>, // ID handle of the glyph texture
    size: IVec2,                        // Size of glyph
    bearing: IVec2,                     // Offset from baseline to left/top of glyph
    advance: u32,                       // Offset to advance to the next glyph
}

#[derive(Debug, Clone)]
struct Caret {
    position: IVec2,
    character: Character,
}

fn load_character(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    face: &freetype::Face,
    characters: &mut HashMap<char, Character>,
    normalized_utf8_character: char,
) -> anyhow::Result<()> {
    face.load_char(
        normalized_utf8_character as usize,
        freetype::face::LoadFlag::RENDER,
    )?;
    let glyph = face.glyph();
    if glyph.bitmap().width() == 0 || glyph.bitmap().rows() == 0 {
        log::debug!(
            "Skipped the loading of the character `{}`",
            normalized_utf8_character
        );
        return Ok(());
    }
    let texture = Texture::from_glyph(
        device,
        queue,
        glyph,
        Some(format!("Glyph Texture {}", normalized_utf8_character).as_str()),
    );

    let character = Character {
        texture: sendable::SendRc::new(texture),
        size: IVec2::new(glyph.bitmap().width(), glyph.bitmap().rows()),
        bearing: IVec2::new(glyph.bitmap_left(), glyph.bitmap_top()),
        advance: glyph.advance().x as u32,
    };
    characters.insert(normalized_utf8_character, character);
    Ok(())
}

/// Represents margins around a block of text.
#[derive(Debug, Clone, Copy)]
pub struct Margins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}
