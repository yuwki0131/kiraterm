struct Uniforms { screen: vec2<f32>, _pad: vec2<f32>, }
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VIn {
  @location(0) pos: vec2<f32>,
  @location(1) uv: vec2<f32>,
  @location(2) color: vec4<f32>,
  @location(3) is_bg: f32,
}
struct VOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) color: vec4<f32>,
  @location(2) is_bg: f32,
}

@vertex
fn vs_main(v: VIn) -> VOut {
  var o: VOut;
  let ndc = vec2<f32>(v.pos.x / u.screen.x * 2.0 - 1.0, 1.0 - v.pos.y / u.screen.y * 2.0);
  o.pos = vec4<f32>(ndc, 0.0, 1.0);
  o.uv = v.uv;
  o.color = v.color;
  o.is_bg = v.is_bg;
  return o;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
  if (in.is_bg > 0.5) {
    return in.color;
  }
  let a = textureSample(atlas, samp, in.uv).r;
  return vec4<f32>(in.color.rgb * a, in.color.a * a);
}
