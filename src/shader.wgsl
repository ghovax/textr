struct CameraUniform {
    projection_matrix: mat4x4<f32>,
};
@group(1) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) texture_coordinates: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) texture_coordinates: vec2<f32>,
};

@vertex
fn vertex_main(
    input: VertexInput,
) -> VertexOutput {
    var output: VertexOutput;

    output.texture_coordinates = input.texture_coordinates;
    output.clip_position = camera.projection_matrix * vec4<f32>(input.position, 0.0, 1.0);
    return output;
}

@group(0) @binding(0)
var texture_view: texture_2d<f32>;
@group(0) @binding(1)
var texture_sampler: sampler;

@fragment
fn fragment_main(vertex_output: VertexOutput) -> @location(0) vec4<f32> {
    let texture_sample = textureSample(texture_view, texture_sampler, vertex_output.texture_coordinates);
    return vec4(0.0, 0.0, 0.0, 1.0) * vec4(1.0, 1.0, 1.0, texture_sample.r);
}