#version 140

in vec2 position;
in vec2 texture_coordinates;
in vec4 color;

out vec2 vertex_texture_coordinates;
out vec4 vertex_color;

void main() {
    gl_Position = vec4(position, 0.0, 1.0);
    vertex_texture_coordinates = texture_coordinates;
    vertex_color = color;
}