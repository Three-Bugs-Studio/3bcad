use glam::{Mat3, Mat4, Quat, Vec2, Vec3, Vec4Swizzles};

/// How long a Normal To / view change animation takes, seconds.
const ANIMATION: f32 = 0.35;

/// Orbit camera around `target`. The orientation is a quaternion (camera-to-world; the camera looks
/// down its local -Z with +Y up), so views straight down any plane are fine, unlike yaw/pitch.
#[derive(Clone, Debug)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub rotation: Quat,
    pub fov_y: f32,
    animation: Option<(Quat, Quat, f32)>,
}

impl Default for OrbitCamera {
    /// SolidWorks-like isometric-ish start view, Z up.
    fn default() -> Self {
        let (yaw, pitch) = (-1.0_f32, 0.55_f32);
        let z = Vec3::new(pitch.cos() * yaw.cos(), pitch.cos() * yaw.sin(), pitch.sin());
        Self {
            target: Vec3::ZERO,
            distance: 200.0,
            rotation: basis(z, Vec3::Z),
            fov_y: 35f32.to_radians(),
            animation: None,
        }
    }
}

/// Rotation whose local Z is `z` and whose local Y is as close to `up` as possible.
fn basis(z: Vec3, up: Vec3) -> Quat {
    let z = z.normalize();
    let mut x = up.cross(z);
    if x.length_squared() < 1e-8 {
        x = Vec3::X.cross(z);
    }
    let x = x.normalize();
    Quat::from_mat3(&Mat3::from_cols(x, z.cross(x), z)).normalize()
}

impl OrbitCamera {
    pub fn eye(&self) -> Vec3 {
        self.target + self.rotation * Vec3::Z * self.distance
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye(), self.target, self.rotation * Vec3::Y)
    }

    pub fn proj(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(
            self.fov_y,
            aspect.max(1e-3),
            self.distance * 0.01,
            self.distance * 100.0,
        )
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.proj(aspect) * self.view()
    }

    /// Turntable around world Z (horizontal drag) and tumble around the screen's x axis (vertical drag).
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.animation = None;
        let yaw = Quat::from_rotation_z(-dx * 0.008);
        let pitch = Quat::from_axis_angle(self.rotation * Vec3::X, -dy * 0.008);
        self.rotation = (yaw * pitch * self.rotation).normalize();
    }

    /// `dx, dy` in screen points; keeps the point under the cursor under the cursor.
    pub fn pan(&mut self, dx: f32, dy: f32, viewport_height: f32) {
        let per_point = 2.0 * self.distance * (self.fov_y * 0.5).tan() / viewport_height.max(1.0);
        let right = self.rotation * Vec3::X;
        let up = self.rotation * Vec3::Y;
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

    /// Normal To: animates to look straight at a plane, with the plane's x axis to the right.
    pub fn normal_to(&mut self, normal: Vec3, x: Vec3) {
        let n = normal.normalize();
        let x = (x - n * x.dot(n)).normalize();
        let to = Quat::from_mat3(&Mat3::from_cols(x, n.cross(x), n)).normalize();
        self.animation = Some((self.rotation, to, 0.0));
    }

    /// Advances an animation by `dt` seconds; true while one is running (keep repainting).
    pub fn animate(&mut self, dt: f32) -> bool {
        let Some((from, to, t)) = self.animation else {
            return false;
        };
        let t = (t + dt / ANIMATION).min(1.0);
        let eased = t * t * (3.0 - 2.0 * t);
        self.rotation = from.slerp(to, eased).normalize();
        self.animation = (t < 1.0).then_some((from, to, t));
        true
    }

    /// World point to viewport pixels (origin top-left); `None` when behind the camera.
    pub fn project(&self, world: Vec3, viewport: Vec2) -> Option<Vec2> {
        let clip = self.view_proj(viewport.x / viewport.y) * world.extend(1.0);
        if clip.w <= 1e-6 {
            return None;
        }
        let ndc = clip.xy() / clip.w;
        Some(Vec2::new(
            (ndc.x + 1.0) * 0.5 * viewport.x,
            (1.0 - ndc.y) * 0.5 * viewport.y,
        ))
    }

    /// Ray through a viewport pixel: (origin on the near plane, unit direction).
    pub fn ray(&self, pixel: Vec2, viewport: Vec2) -> (Vec3, Vec3) {
        let inv = self.view_proj(viewport.x / viewport.y).inverse();
        let ndc = Vec2::new(pixel.x / viewport.x * 2.0 - 1.0, 1.0 - pixel.y / viewport.y * 2.0);
        let near = inv * ndc.extend(0.0).extend(1.0);
        let far = inv * ndc.extend(1.0).extend(1.0);
        let (near, far) = (near.xyz() / near.w, far.xyz() / far.w);
        (near, (far - near).normalize())
    }

    /// World units covered by one pixel at the depth of `world` (turns pixel tolerances into lengths).
    pub fn world_per_pixel(&self, world: Vec3, viewport_height: f32) -> f32 {
        let depth = (world - self.eye())
            .dot(self.rotation * -Vec3::Z)
            .max(self.distance * 0.01);
        2.0 * depth * (self.fov_y * 0.5).tan() / viewport_height.max(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_and_ray_round_trip() {
        let cam = OrbitCamera {
            target: Vec3::new(10.0, -5.0, 3.0),
            ..Default::default()
        };
        let vp = Vec2::new(800.0, 600.0);
        let p = Vec3::new(12.0, -3.0, 4.0);
        let px = cam.project(p, vp).unwrap();
        let (o, d) = cam.ray(px, vp);
        let closest = o + d * (p - o).dot(d);
        assert!(closest.distance(p) < 1e-2, "{closest} vs {p}");
        assert!(
            cam.project(cam.eye() + (cam.eye() - cam.target), vp).is_none(),
            "behind the camera"
        );
        // The target is at the center of the screen.
        assert!(cam.project(cam.target, vp).unwrap().distance(vp / 2.0) < 1e-2);
    }

    #[test]
    fn normal_to_looks_straight_at_the_plane() {
        let mut cam = OrbitCamera::default();
        cam.normal_to(Vec3::Z, Vec3::X);
        while cam.animate(0.05) {}
        assert!(
            cam.eye().normalize().distance(Vec3::Z) < 1e-5,
            "eye above the Top plane"
        );
        let view = cam.view();
        assert!(
            view.transform_vector3(Vec3::X).distance(Vec3::X) < 1e-5,
            "plane x to the right"
        );
        assert!(view.transform_vector3(Vec3::Y).distance(Vec3::Y) < 1e-5, "plane y up");
        // Front plane (normal -Y): world Z points up on screen.
        cam.normal_to(-Vec3::Y, Vec3::X);
        while cam.animate(0.05) {}
        assert!(cam.view().transform_vector3(Vec3::Z).distance(Vec3::Y) < 1e-5);
    }

    #[test]
    fn orbit_keeps_distance_and_can_go_over_the_top() {
        let mut cam = OrbitCamera::default();
        let d = cam.eye().distance(cam.target);
        for _ in 0..200 {
            cam.orbit(3.0, 7.0);
        }
        assert!((cam.eye().distance(cam.target) - d).abs() < 1e-2);
        assert!(cam.view().is_finite());
    }

    #[test]
    fn world_per_pixel_matches_projection() {
        let cam = OrbitCamera::default();
        let vp = Vec2::new(1000.0, 800.0);
        let p = cam.target;
        let per = cam.world_per_pixel(p, vp.y);
        let a = cam.project(p, vp).unwrap();
        let b = cam.project(p + (cam.rotation * Vec3::X) * per * 10.0, vp).unwrap();
        assert!((a.distance(b) - 10.0).abs() < 0.05, "{}", a.distance(b));
    }
}
