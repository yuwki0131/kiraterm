use rand::Rng;

pub struct Particle {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub color: [f32; 3],
    pub life: f32,
    pub max_life: f32,
    pub size: f32,
}
#[derive(Default)]
pub struct Particles {
    pub items: Vec<Particle>,
}

impl Particles {
    pub fn emit(&mut self, pos: [f32; 2], color: [f32; 3], count: usize) {
        let mut rng = rand::thread_rng();
        for _ in 0..count {
            let angle = rng.gen_range(0.0..std::f32::consts::TAU);
            let speed = rng.gen_range(60.0..320.0);
            let max_life = rng.gen_range(0.4..1.0);
            self.items.push(Particle {
                pos,
                vel: [angle.cos() * speed, angle.sin() * speed - 40.0],
                color,
                life: 1.0,
                max_life,
                size: rng.gen_range(1.5..4.0),
            });
        }
    }
    pub fn update(&mut self, dt: f32) {
        for p in &mut self.items {
            p.pos[0] += p.vel[0] * dt;
            p.pos[1] += p.vel[1] * dt;
            p.vel[1] += 180.0 * dt;
            p.vel[0] *= 1.0 - 1.4 * dt;
            p.life -= dt / p.max_life;
        }
        self.items.retain(|p| p.life > 0.0);
    }
}
