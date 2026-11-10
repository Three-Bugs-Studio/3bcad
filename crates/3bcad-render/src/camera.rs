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
    /// Orthographic projection: no perspective, parallel lines stay parallel.
    pub ortho: bool,
    animation: Option<(Quat, Quat, f32)>,
}

impl Default for OrbitCamera {
    /// Isometric-ish start view, Z up.
    fn default() -> Self {
        let (yaw, pitch) = (-1.0_f32, 0.55_f32);
        let z = Vec3::new(pitch.cos() * yaw.cos(), pitch.cos() * yaw.sin(), pitch.sin());
        Self {
            target: Vec3::ZERO,
            distance: 200.0,
            rotation: basis(z, Vec3::Z),
            fov_y: 35f32.to_radians(),
            ortho: false,
            animation: None,
        }
    }
}

/// The standard views (Z up, Front looks along +Y): where the eye looks from, and up on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandardView {
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
    Isometric,
}

impl StandardView {
    pub const ALL: [Self; 7] = [
        Self::Front,
        Self::Back,
        Self::Left,
        Self::Right,
        Self::Top,
        Self::Bottom,
        Self::Isometric,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Front => "Front",
            Self::Back => "Back",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Top => "Top",
            Self::Bottom => "Bottom",
            Self::Isometric => "Isometric",
        }
    }

    /// (from, up).
    pub fn direction(self) -> (Vec3, Vec3) {
        match self {
            Self::Front => (-Vec3::Y, Vec3::Z),
            Self::Back => (Vec3::Y, Vec3::Z),
            Self::Left => (-Vec3::X, Vec3::Z),
            Self::Right => (Vec3::X, Vec3::Z),
            Self::Top => (Vec3::Z, Vec3::Y),
            Self::Bottom => (-Vec3::Z, -Vec3::Y),
            Self::Isometric => (Vec3::new(1.0, -1.0, 1.0).normalize(), Vec3::Z),
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
        let aspect = aspect.max(1e-3);
        if self.ortho {
            // As big as the perspective view at the target, so switching keeps the model's size;
            // the near plane is behind the eye, so zooming in never cuts the model.
            let h = self.half_height();
            return Mat4::orthographic_rh(
                -h * aspect,
                h * aspect,
                -h,
                h,
                -self.distance * 50.0,
                self.distance * 100.0,
            );
        }
        Mat4::perspective_rh(self.fov_y, aspect, self.distance * 0.01, self.distance * 100.0)
    }

    /// Half the view's height at the target, in world units.
    fn half_height(&self) -> f32 {
        self.distance * (self.fov_y * 0.5).tan()
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
        let per_point = 2.0 * self.half_height() / viewport_height.max(1.0);
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

    /// Animates to look at the target from direction `from` (towards the eye), with `up` up on screen.
    pub fn look_from(&mut self, from: Vec3, up: Vec3) {
        self.animation = Some((self.rotation, basis(from, up), 0.0));
    }

    /// Turns the view about the screen's vertical and horizontal axes by `yaw` and `pitch` radians
    /// (the arrow keys).
    pub fn turn(&mut self, yaw: f32, pitch: f32) {
        self.orbit(yaw / 0.008, pitch / 0.008);
    }

    /// Normal To: animates to look straight at a plane, with the plane's x axis to the right.
    pub fn normal_to(&mut self, normal: Vec3, x: Vec3) {
        let n = normal.normalize();
        let x = (x - n * x.dot(n)).normalize();
        let to = Quat::from_mat3(&Mat3::from_cols(x, n.cross(x), n)).normalize();
        self.animation = Some((self.rotation, to, 0.0));
    }

    pub fn is_animating(&self) -> bool {
        self.animation.is_some()
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
        if self.ortho {
            return 2.0 * self.half_height() / viewport_height.max(1.0);
        }
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
    fn standard_views_look_from_the_named_side() {
        let mut cam = OrbitCamera::default();
        let (from, up) = StandardView::Front.direction();
        cam.look_from(from, up);
        let mut frames = 0;
        while cam.animate(0.05) {
            frames += 1;
        }
        assert!(frames > 2, "the change is animated");
        // Front: the eye is on -Y, world X to the right and Z up.
        assert!(cam.eye().normalize().distance(-Vec3::Y) < 1e-5);
        assert!(cam.view().transform_vector3(Vec3::X).distance(Vec3::X) < 1e-5);
        assert!(cam.view().transform_vector3(Vec3::Z).distance(Vec3::Y) < 1e-5);
        let (from, up) = StandardView::Isometric.direction();
        cam.look_from(from, up);
        while cam.animate(0.05) {}
        // Isometric: the three axes look equally long on screen.
        let len = |v: Vec3| cam.view().transform_vector3(v).truncate().length();
        assert!((len(Vec3::X) - len(Vec3::Y)).abs() < 1e-4 && (len(Vec3::Y) - len(Vec3::Z)).abs() < 1e-4);
    }

    #[test]
    fn orthographic_projection_keeps_size_and_rays_parallel() {
        let mut cam = OrbitCamera {
            target: Vec3::new(10.0, -5.0, 3.0),
            ..Default::default()
        };
        let vp = Vec2::new(800.0, 600.0);
        // A 1 mm step at the target covers as many pixels in both projections.
        let step = cam.rotation * Vec3::X;
        let px = |c: &OrbitCamera| {
            let at = |p: Vec3| c.project(p, vp).unwrap();
            at(c.target + step).distance(at(c.target))
        };
        let persp = px(&cam);
        cam.ortho = true;
        assert!((px(&cam) - persp).abs() < 1e-3 * persp, "{} vs {persp}", px(&cam));
        // Rays through any pixel run the same way, and lead back to the point they were cast at.
        let (o1, d1) = cam.ray(Vec2::new(10.0, 20.0), vp);
        let (o2, d2) = cam.ray(Vec2::new(700.0, 500.0), vp);
        assert!(d1.distance(d2) < 1e-5 && o1.distance(o2) > 1.0);
        let p = Vec3::new(12.0, -3.0, 4.0);
        let (o, d) = cam.ray(cam.project(p, vp).unwrap(), vp);
        assert!((o + d * (p - o).dot(d)).distance(p) < 1e-2);
        // Geometry behind the eye is still drawn: zooming in never cuts the model.
        assert!(cam.project(cam.eye() + (cam.eye() - cam.target) * 3.0, vp).is_some());
        assert!((cam.world_per_pixel(p, vp.y) * persp - 1.0).abs() < 1e-3);
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
