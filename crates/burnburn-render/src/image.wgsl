// BurnBurn canvas, picture pass: a textured mesh (the camera picture on the bed). Positions arrive
// in clip space; the texture is straight-alpha RGBA, scaled by one opacity.

struct Opacity {
    value: vec4<f32>,
};

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var picture_sampler: sampler;
@group(0) @binding(2) var<uniform> opacity: Opacity;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@location(0) position: vec2<f32>, @location(1) uv: vec2<f32>) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let c = textureSample(picture, picture_sampler, in.uv);
    return vec4<f32>(c.rgb, c.a * opacity.value.x);
}
