use glam::{Mat4, Vec3};

/// Z-up orbit camera: left-drag orbits, right-drag pans, wheel zooms.
#[derive(Clone, Debug)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    /// Radians around +Z.
    pub yaw: f32,
    /// Radians above the XY plane.
    pub pitch: f32,
    pub fov_y: f32,
}

impl Default for OrbitCamera {
    /// SolidWorks-like isometric-ish start view.
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 200.0,
            yaw: -1.0,
            pitch: 0.55,
            fov_y: 35f32.to_radians(),
        }
    }
}

impl OrbitCamera {
    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + Vec3::new(cp * cy, cp * sy, sp) * self.distance
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye(), self.target, Vec3::Z)
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let proj = Mat4::perspective_rh(
            self.fov_y,
            aspect.max(1e-3),
            self.distance * 0.01,
            self.distance * 100.0,
        );
        proj * self.view()
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.008;
        self.pitch = (self.pitch + dy * 0.008).clamp(-1.55, 1.55);
    }

    /// `dx, dy` in screen points; keeps the point under the cursor under the cursor.
    pub fn pan(&mut self, dx: f32, dy: f32, viewport_height: f32) {
        let per_point = 2.0 * self.distance * (self.fov_y * 0.5).tan() / viewport_height.max(1.0);
        let view = self.view();
        let right = view.row(0).truncate();
        let up = view.row(1).truncate();
        self.target += (up * dy - right * dx) * per_point;
    }

    pub fn zoom(&mut self, scroll: f32) {
        self.distance = (self.distance * (-scroll * 0.002).exp()).clamp(0.1, 1e5);
    }

    /// Frames an axis-aligned box.
    pub fn fit(&mut self, min: Vec3, max: Vec3) {
        self.target = (min + max) * 0.5;
        let radius = ((max - min).length() * 0.5).max(1.0);
        self.distance = radius / (self.fov_y * 0.5).sin() * 1.1;
    }
}
