struct Uniforms { screen: vec2<f32>, _pad: vec2<f32>, }
@group(0) @binding(0) var<uniform> u: Uniforms;

struct VIn {
  @location(0) pos: vec2<f32>,
  @location(1) local: vec2<f32>,
  @location(2) color: vec4<f32>,
}
struct VOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) local: vec2<f32>,
  @location(1) color: vec4<f32>,
}
@vertex
fn vs_main(v: VIn) -> VOut {
  var o: VOut;
  let ndc = vec2<f32>(v.pos.x / u.screen.x * 2.0 - 1.0, 1.0 - v.pos.y / u.screen.y * 2.0);
  o.pos = vec4<f32>(ndc, 0.0, 1.0);
  o.local = v.local;
  o.color = v.color;
  return o;
}
@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
  let d = length(in.local);
  if (d > 1.0) { discard; }
  let g = pow(1.0 - d, 2.0);
  return vec4<f32>(in.color.rgb * g, g * in.color.a);
}
