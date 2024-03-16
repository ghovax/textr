use crate::BitmapData;
use anyhow::Ok;
use nalgebra_glm as glm;
use std::collections::HashMap;
use wgpu::util::DeviceExt;
use winit::window::Window;

/// Container for all of the texture-related information such as the size, the
/// data and the format options needed for rendering it onto the screen.
#[derive(Debug)]
pub struct Texture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl Texture {
    /// By using the global render state, attempt to load the texture from
    /// bitmap data.
    pub fn from_bitmap_data(
        render_state: &RenderState,
        bitmap_data: BitmapData,
        label: Option<&str>,
    ) -> anyhow::Result<Self> {
        // First configure the format and the size...
        let size = wgpu::Extent3d { width: bitmap_data.width, height: bitmap_data.rows, depth_or_array_layers: 1 };
        let format = wgpu::TextureFormat::R8Unorm;
        // ...then actually load the texture by employing a texture descriptor
        let texture = render_state.device.create_texture(&wgpu::TextureDescriptor {
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

        // Instruct the system to load the texture data to the GPU
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

        // At last, give some parameters which are needed for rendering the texture
        let view = texture.create_view(&wgpu::TextureViewDescriptor { format: Some(format), ..Default::default() });
        let sampler = render_state.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Then return the texture if no errors have been generated before
        Ok(Self { texture, view, sampler })
    }
}

/// The data type that will be passed on to the GPU when loading the data.
/// This is converted to a C-compatible form by means of `#[repr(C)]`,
/// `bytemuck::Pod` and `bytemuck::Zeroable`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub texture_coordinates: [f32; 2],
}

impl Vertex {
    /// Returns the vertex buffer layout which is needed for that GPU to
    /// understand the format of data it is receiving.
    fn descriptor<'a>() -> wgpu::VertexBufferLayout<'a> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // This is specifying the vertex positions
                wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x2 },
                // This is specifying the texture coordinates, which are offset by the position
                // attribute
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

/// The state of the graphics system. It contains all the variables needed for
/// configuring and rendering to a window system the text. It needs to be owned
/// by the main thread.
pub struct RenderState {
    window: Window,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    configuration: wgpu::SurfaceConfiguration,
    pub physical_size: winit::dpi::PhysicalSize<u32>,
    camera_bind_group: wgpu::BindGroup,
    render_pipeline: wgpu::RenderPipeline,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    /// The cache for the textures. It allows a retrieval with the associated
    /// character.
    texture_bind_groups: HashMap<char, wgpu::BindGroup>,
    vertex_buffer: wgpu::Buffer,
    /// The characters which are actually expected to be loaded onto the GPU.
    text_characters: Vec<char>,
}

/// The data type which is passed on to the GPU for rendering the vertices with
/// a given projection.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    projection_matrix: [[f32; 4]; 4],
}

impl RenderState {
    /// Initialize the novel render state from the window.
    pub async fn new(window: Window) -> Result<Self, anyhow::Error> {
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
        // RenderState owns the window so this should be safe.
        let surface = unsafe { instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::from_window(&window)?) }?;

        // Creating some of the wgpu types requires async code
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or(anyhow::anyhow!("No adapter found"))?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(), // wgpu::Features::CONSERVATIVE_RASTERIZATION,
                    // WebGL doesn't support all of wgpu's features, so if
                    // we're building for the web we'll have to disable some.
                    required_limits: if cfg!(target_arch = "wasm32") {
                        panic!("compilation for wasm32 is not yet implemented");
                    } else {
                        wgpu::Limits::default()
                    },
                    label: None,
                },
                None, // Trace path
            )
            .await?;

        let surface_capabilities = surface.get_capabilities(&adapter);
        // Shader code assumes an sRGB surface texture. Using a different
        // one will result all the colors coming out darker. If you want to support non
        // sRGB surfaces, you'll need to account for that when drawing to the frame.
        let surface_format = match surface_capabilities.formats.iter().copied().find(|format| format.is_srgb()) {
            Some(format) => format,
            None => match surface_capabilities.formats.first() {
                Some(format) => *format,
                None => {
                    return Err(anyhow::anyhow!("No supported surface format found"));
                }
            },
        };

        let inner_size = window.inner_size();
        let configuration = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: inner_size.width,
            height: inner_size.height,
            present_mode: surface_capabilities.present_modes[0],
            alpha_mode: surface_capabilities.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &configuration);

        let texture_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
            projection_matrix: glm::ortho(0.0, inner_size.width as f32, 0.0, inner_size.height as f32, -1.0, 1.0)
                .into(),
        };

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: camera_buffer.as_entire_binding() }],
            label: Some("Camera Bind Group"),
        });

        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&texture_bind_group_layout, &camera_bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: "vertex_main", buffers: &[Vertex::descriptor()] },
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
            multisample: wgpu::MultisampleState { count: 1, mask: !0, alpha_to_coverage_enabled: false },
            multiview: None,
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Glyphs Vertex Buffer"),
            contents: bytemuck::cast_slice(&Vec::<[Vertex; 6]>::new()),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Ok(Self {
            window,
            surface,
            device,
            queue,
            configuration,
            physical_size: inner_size,
            texture_bind_group_layout,
            camera_bind_group,
            render_pipeline,
            texture_bind_groups: HashMap::new(),
            vertex_buffer,
            text_characters: Vec::new(),
        })
    }

    /// Load the texture bind group for the glyph associated to the given
    /// character requested to be loaded.
    pub fn load_texture(&mut self, texture: Texture, character_to_load: char) {
        let texture_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&texture.view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&texture.sampler) },
            ],
            label: Some(format!("Glyph {:?} Texture Bind Group", character_to_load).as_str()),
        });

        match self.texture_bind_groups.insert(character_to_load, texture_bind_group) {
            Some(_) => {
                log::warn!("The texture bind group for the glyph {:?} has already been loaded", character_to_load);
            }
            None => (),
        };
    }

    /// Update the vertex buffers associated with the characters with the new
    /// vertices.
    pub fn update_vertex_buffer(&mut self, vertex_data: Vec<[Vertex; 6]>, text_characters: Vec<char>) {
        self.vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Glyphs Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        });
        self.text_characters = text_characters;
    }

    /// Get a reference to the window.
    pub fn window(&self) -> &Window {
        &self.window
    }

    /// Render the frame. This function may fail because the surface may be
    /// lost.
    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;

        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[
                    // This is what @location(0) in the fragment shader targets
                    Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            render_pass.set_pipeline(&self.render_pipeline);

            render_pass.set_bind_group(1, &self.camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

            for (character_index, character) in self.text_characters.iter().enumerate() {
                let bind_group = match self.texture_bind_groups.get(character) {
                    Some(bind_group) => bind_group,
                    None => {
                        log::error!(
                            "There is no texture bind group present in the cache for the character: {:?}",
                            character
                        );
                        return Err(wgpu::SurfaceError::OutOfMemory);
                    }
                };
                render_pass.set_bind_group(0, bind_group, &[]);

                render_pass.draw(6 * (character_index as u32)..6 * (character_index as u32 + 1), 0..1);
            }
            log::debug!("Finished drawing the characters");
        }

        // `submit` will accept anything that implements IntoIter
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        std::result::Result::Ok(())
    }

    /// Resize the surface and reconfigure it.
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.physical_size = new_size;
            self.configuration.width = new_size.width;
            self.configuration.height = new_size.height;
            self.surface.configure(&self.device, &self.configuration);
        }
    }
}
