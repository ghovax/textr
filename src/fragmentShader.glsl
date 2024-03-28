#version 140

uniform sampler2D texture_sampler;

in vec2 vertex_texture_coordinates;
in vec4 vertex_color;

out vec4 fragment_color;

void main() {
    fragment_color = vertex_color * vec4(1.0, 1.0, 1.0, texture(texture_sampler, vertex_texture_coordinates).r);
}