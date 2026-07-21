struct Uniforms {
  size: vec2<f32>,
  time: f32,
  glitch: f32,
}
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var scene: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
  var pos = array<vec2<f32>, 3>(
    vec2<f32>(-1.0, -3.0),
    vec2<f32>(-1.0,  1.0),
    vec2<f32>( 3.0,  1.0),
  );
  return vec4<f32>(pos[vi], 0.0, 1.0);
}

fn hash21(p: vec2<f32>) -> f32 {
  return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn glow_sample(uv: vec2<f32>) -> vec3<f32> {
  var acc = vec3<f32>(0.0);
  let step = 1.5 / u.size;
  var w_total = 0.0;
  for (var y: i32 = -3; y <= 3; y = y + 1) {
    for (var x: i32 = -3; x <= 3; x = x + 1) {
      let off = vec2<f32>(f32(x), f32(y)) * step;
      let w = exp(-f32(x*x + y*y) / 4.0);
      acc = acc + textureSample(scene, samp, uv + off).rgb * w;
      w_total = w_total + w;
    }
  }
  return acc / w_total;
}

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
  let uv = frag.xy / u.size;
  let cx = uv.x - 0.5;
  let cy = uv.y - 0.5;
  let r2 = cx*cx + cy*cy;
  let warp = 1.0 + 0.05 * r2;
  var suv = vec2<f32>(0.5 + cx * warp, 0.5 + cy * warp);
  let band = step(0.98, fract(sin((suv.y + u.time * 0.1) * 40.0) * 43758.5453));
  suv.x = suv.x + band * (hash21(vec2<f32>(u.time * 5.0, floor(suv.y * 60.0))) - 0.5) * 0.02 * u.glitch;
  let sh = 2.0 / u.size.x + u.glitch * 4.0 / u.size.x;
  let r = textureSample(scene, samp, suv + vec2<f32>(sh, 0.0)).r;
  let g = textureSample(scene, samp, suv).g;
  let b = textureSample(scene, samp, suv - vec2<f32>(sh, 0.0)).b;
  var color = vec3<f32>(r, g, b);
  let glow = glow_sample(suv);
  color = color + glow * 0.55;
  color = color + glow * glow * 0.7;
  let sl = 0.85 + 0.15 * sin(frag.y * 3.14159);
  color = color * sl;
  let vig = smoothstep(1.0, 0.3, r2 * 2.5);
  color = color * vig;
  let n = hash21(frag.xy + vec2<f32>(u.time * 60.0, 0.0)) * 0.06;
  color = color + n * vec3<f32>(0.4, 0.6, 0.7);
  if (suv.x < 0.0 || suv.x > 1.0 || suv.y < 0.0 || suv.y > 1.0) {
    color = vec3<f32>(0.0);
  }
  return vec4<f32>(color, 1.0);
}
