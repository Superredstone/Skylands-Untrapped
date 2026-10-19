// ponytail: minimal column-major mat4 without external math crates; upgrade to glam if SIMD needed
pub mod mat4 {
    pub type Mat4 = [[f32; 4]; 4];

    pub const IDENTITY: Mat4 = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];

    pub fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
        let mut out = [[0.0; 4]; 4];
        for col in 0..4 {
            for row in 0..4 {
                out[col][row] = a[0][row] * b[col][0]
                    + a[1][row] * b[col][1]
                    + a[2][row] * b[col][2]
                    + a[3][row] * b[col][3];
            }
        }
        out
    }

    pub fn translation(pos: [f32; 3]) -> Mat4 {
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [pos[0], pos[1], pos[2], 1.0],
        ]
    }

    pub fn rotation_y(angle: f32) -> Mat4 {
        let (sin, cos) = angle.sin_cos();
        [
            [cos, 0.0, -sin, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [sin, 0.0, cos, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]
    }

    pub fn transform(pos: [f32; 3], rotation_y_rad: f32, scale: [f32; 3]) -> Mat4 {
        let (sin, cos) = rotation_y_rad.sin_cos();
        [
            [cos * scale[0], 0.0, -sin * scale[0], 0.0],
            [0.0, scale[1], 0.0, 0.0],
            [sin * scale[2], 0.0, cos * scale[2], 0.0],
            [pos[0], pos[1], pos[2], 1.0],
        ]
    }

    pub fn look_at(eye: [f32; 3], target: [f32; 3], up: [f32; 3]) -> Mat4 {
        let f = normalize([target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]]);
        let r = normalize(cross(f, up));
        let u = cross(r, f);

        [
            [r[0], u[0], -f[0], 0.0],
            [r[1], u[1], -f[1], 0.0],
            [r[2], u[2], -f[2], 0.0],
            [-dot(r, eye), -dot(u, eye), dot(f, eye), 1.0],
        ]
    }

    pub fn perspective(fov_y_rad: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let f = 1.0 / (fov_y_rad * 0.5).tan();
        let range = near - far;
        [
            [f / aspect, 0.0, 0.0, 0.0],
            [0.0, f, 0.0, 0.0],
            [0.0, 0.0, far / range, -1.0],
            [0.0, 0.0, (near * far) / range, 0.0],
        ]
    }

    pub fn orthographic(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Mat4 {
        let r_l = right - left;
        let t_b = top - bottom;
        let f_n = far - near;
        [
            [2.0 / r_l, 0.0, 0.0, 0.0],
            [0.0, 2.0 / t_b, 0.0, 0.0],
            [0.0, 0.0, 1.0 / f_n, 0.0],
            [
                -(right + left) / r_l,
                -(top + bottom) / t_b,
                -near / f_n,
                1.0,
            ],
        ]
    }

    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    fn normalize(v: [f32; 3]) -> [f32; 3] {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if len > 0.00001 {
            [v[0] / len, v[1] / len, v[2] / len]
        } else {
            [0.0, 0.0, 0.0]
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CameraVolumeMode {
    FixedOverhead { height: f32, distance: f32 },
    FollowingChase { height: f32, distance: f32 },
    FixedCinematic { eye: [f32; 3], target: [f32; 3] },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CameraVolume {
    pub name: String,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub mode: CameraVolumeMode,
}

impl CameraVolume {
    pub fn contains(&self, p: [f32; 3]) -> bool {
        p[0] >= self.bounds_min[0]
            && p[0] <= self.bounds_max[0]
            && p[1] >= self.bounds_min[1]
            && p[1] <= self.bounds_max[1]
            && p[2] >= self.bounds_min[2]
            && p[2] <= self.bounds_max[2]
    }
}

pub struct Camera {
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub up: [f32; 3],
    pub fov_y: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
    pub target_eye: [f32; 3],
    pub target_target: [f32; 3],
    pub volumes: Vec<CameraVolume>,
}

impl Camera {
    pub fn new(aspect: f32) -> Self {
        let eye = [0.0, 4.0, 7.0];
        let target = [0.0, 0.0, 0.0];
        Self {
            eye,
            target,
            up: [0.0, 1.0, 0.0],
            fov_y: 60.0f32.to_radians(),
            aspect,
            near: 0.1,
            far: 100.0,
            target_eye: eye,
            target_target: target,
            volumes: Vec::new(),
        }
    }

    pub fn update_interpolation(&mut self, dt: f32) {
        let t = (dt * 5.0).clamp(0.0, 1.0);
        for i in 0..3 {
            self.eye[i] += (self.target_eye[i] - self.eye[i]) * t;
            self.target[i] += (self.target_target[i] - self.target[i]) * t;
        }
    }

    pub fn set_immediate(&mut self, eye: [f32; 3], target: [f32; 3]) {
        self.eye = eye;
        self.target = target;
        self.target_eye = eye;
        self.target_target = target;
    }

    pub fn update_target_for_hero(&mut self, hero_pos: [f32; 3]) {
        if let Some(vol) = self.volumes.iter().find(|v| v.contains(hero_pos)) {
            match vol.mode {
                CameraVolumeMode::FixedOverhead { height, distance } => {
                    self.target_target = hero_pos;
                    self.target_eye = [hero_pos[0], hero_pos[1] + height, hero_pos[2] + distance];
                }
                CameraVolumeMode::FollowingChase { height, distance } => {
                    self.target_target = hero_pos;
                    self.target_eye = [hero_pos[0], hero_pos[1] + height, hero_pos[2] + distance];
                }
                CameraVolumeMode::FixedCinematic { eye, target } => {
                    self.target_eye = eye;
                    self.target_target = target;
                }
            }
        } else {
            // Default following chase camera
            self.target_target = hero_pos;
            self.target_eye = [hero_pos[0], hero_pos[1] + 4.5, hero_pos[2] + 7.5];
        }
    }

    pub fn build_view_projection_matrix(&self) -> mat4::Mat4 {
        let view = mat4::look_at(self.eye, self.target, self.up);
        let proj = mat4::perspective(self.fov_y, self.aspect, self.near, self.far);
        mat4::mul(&proj, &view)
    }

    pub fn project_to_screen(
        &self,
        world_pos: [f32; 3],
        screen_size: (f32, f32),
    ) -> Option<[f32; 2]> {
        let vp = self.build_view_projection_matrix();
        let clip_x =
            vp[0][0] * world_pos[0] + vp[1][0] * world_pos[1] + vp[2][0] * world_pos[2] + vp[3][0];
        let clip_y =
            vp[0][1] * world_pos[0] + vp[1][1] * world_pos[1] + vp[2][1] * world_pos[2] + vp[3][1];
        let clip_w =
            vp[0][3] * world_pos[0] + vp[1][3] * world_pos[1] + vp[2][3] * world_pos[2] + vp[3][3];

        if clip_w <= 0.001 {
            return None;
        }

        let ndc_x = clip_x / clip_w;
        let ndc_y = clip_y / clip_w;

        let screen_x = (ndc_x + 1.0) * 0.5 * screen_size.0;
        let screen_y = (1.0 - ndc_y) * 0.5 * screen_size.1;

        Some([screen_x, screen_y])
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FreeCam {
    pub enabled: bool,
    pub eye: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
    pub sensitivity: f32,
}

impl Default for FreeCam {
    fn default() -> Self {
        Self {
            enabled: false,
            eye: [0.0, 4.0, 7.0],
            yaw: -std::f32::consts::FRAC_PI_2,
            pitch: 0.0,
            speed: 10.0,
            sensitivity: 0.003,
        }
    }
}

impl FreeCam {
    pub fn from_camera(eye: [f32; 3], target: [f32; 3]) -> Self {
        let f = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
        let len = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt();
        let (yaw, pitch) = if len > 0.00001 {
            let norm = [f[0] / len, f[1] / len, f[2] / len];
            let pitch = norm[1].clamp(-1.0, 1.0).asin().clamp(-1.5, 1.5);
            let yaw = norm[2].atan2(norm[0]);
            (yaw, pitch)
        } else {
            (-std::f32::consts::FRAC_PI_2, 0.0)
        };

        Self {
            enabled: true,
            eye,
            yaw,
            pitch,
            speed: 10.0,
            sensitivity: 0.003,
        }
    }

    pub fn forward_vector(&self) -> [f32; 3] {
        let cos_p = self.pitch.cos();
        [
            cos_p * self.yaw.cos(),
            self.pitch.sin(),
            cos_p * self.yaw.sin(),
        ]
    }

    pub fn right_vector(&self) -> [f32; 3] {
        // forward x up(0, 1, 0)
        // [f1*u2 - f2*u1, f2*u0 - f0*u2, f0*u1 - f1*u0] = [-sin(yaw), 0, cos(yaw)]
        [-self.yaw.sin(), 0.0, self.yaw.cos()]
    }

    pub fn up_vector(&self) -> [f32; 3] {
        [0.0, 1.0, 0.0]
    }

    pub fn apply_to_camera(&self, camera: &mut Camera) {
        let f = self.forward_vector();
        camera.eye = self.eye;
        camera.target = [self.eye[0] + f[0], self.eye[1] + f[1], self.eye[2] + f[2]];
        camera.target_eye = camera.eye;
        camera.target_target = camera.target;
    }

    pub fn rotate(&mut self, yaw_delta: f32, pitch_delta: f32) {
        self.yaw += yaw_delta;
        self.pitch = (self.pitch + pitch_delta).clamp(-1.5, 1.5);
    }

    pub fn translate(&mut self, move_vec: [f32; 3], dt: f32) {
        let f = self.forward_vector();
        let r = self.right_vector();
        let u = self.up_vector();

        let dist = self.speed * dt;
        for i in 0..3 {
            self.eye[i] += (f[i] * move_vec[2] + r[i] * move_vec[0] + u[i] * move_vec[1]) * dist;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mat4_identity_mul() {
        let a = mat4::IDENTITY;
        let trans = mat4::translation([1.0, 2.0, 3.0]);
        let res = mat4::mul(&a, &trans);
        assert_eq!(res, trans);
    }

    #[test]
    fn test_camera_world_to_screen_projection() {
        let mut cam = Camera::new(800.0 / 600.0);
        cam.eye = [0.0, 4.0, 7.0];
        cam.target = [0.0, 0.0, 0.0];
        let screen = cam.project_to_screen([0.0, 0.0, 0.0], (800.0, 600.0));
        assert!(screen.is_some());
        let [sx, sy] = screen.unwrap();
        assert!((sx - 400.0).abs() < 1.0);
        assert!((sy - 300.0).abs() < 1.0);

        // Point behind camera should return None
        let behind = cam.project_to_screen([0.0, 10.0, 20.0], (800.0, 600.0));
        assert!(behind.is_none());
    }

    #[test]
    fn test_camera_smooth_lerp() {
        let mut cam = Camera::new(16.0 / 9.0);
        cam.set_immediate([0.0, 5.0, 10.0], [0.0, 0.0, 0.0]);
        assert_eq!(cam.eye, [0.0, 5.0, 10.0]);

        cam.target_eye = [10.0, 5.0, 10.0];
        cam.target_target = [10.0, 0.0, 0.0];

        // Step by 0.016s (approx 60Hz dt)
        cam.update_interpolation(0.016);

        // Camera must move towards target without snapping immediately
        assert!(cam.eye[0] > 0.0 && cam.eye[0] < 10.0);
        assert!(cam.target[0] > 0.0 && cam.target[0] < 10.0);

        // Multiple steps converge
        for _ in 0..60 {
            cam.update_interpolation(0.05);
        }
        assert!((cam.eye[0] - 10.0).abs() < 0.1);
        assert!((cam.target[0] - 10.0).abs() < 0.1);
    }

    #[test]
    fn test_free_cam_vectors_pitch_clamping_and_translation() {
        let mut free_cam = FreeCam::from_camera([0.0, 5.0, 10.0], [0.0, 5.0, 0.0]);
        assert!(free_cam.enabled);
        assert!((free_cam.pitch - 0.0).abs() < 1e-4);
        assert!((free_cam.yaw - (-std::f32::consts::FRAC_PI_2)).abs() < 1e-4);

        let f = free_cam.forward_vector();
        assert!((f[0] - 0.0).abs() < 1e-4);
        assert!((f[1] - 0.0).abs() < 1e-4);
        assert!((f[2] - (-1.0)).abs() < 1e-4);

        let r = free_cam.right_vector();
        assert!((r[0] - 1.0).abs() < 1e-4);
        assert!((r[1] - 0.0).abs() < 1e-4);
        assert!((r[2] - 0.0).abs() < 1e-4);

        // Pitch clamping test: [-1.5, 1.5]
        free_cam.rotate(0.0, 3.0);
        assert_eq!(free_cam.pitch, 1.5);
        free_cam.rotate(0.0, -5.0);
        assert_eq!(free_cam.pitch, -1.5);
        free_cam.pitch = 0.0;

        // Translation test: move forward 1 unit (move_vec = [0.0, 0.0, 1.0])
        let initial_eye = free_cam.eye;
        free_cam.translate([0.0, 0.0, 1.0], 0.1); // speed is 10.0, dist = 1.0 forward (Z - 1.0)
        assert!((free_cam.eye[0] - initial_eye[0]).abs() < 1e-4);
        assert!((free_cam.eye[1] - initial_eye[1]).abs() < 1e-4);
        assert!((free_cam.eye[2] - (initial_eye[2] - 1.0)).abs() < 1e-4);

        // Apply to camera test
        let mut cam = Camera::new(16.0 / 9.0);
        free_cam.apply_to_camera(&mut cam);
        assert_eq!(cam.eye, free_cam.eye);
        assert!((cam.target[0] - free_cam.eye[0]).abs() < 1e-4);
        assert!((cam.target[1] - free_cam.eye[1]).abs() < 1e-4);
        assert!((cam.target[2] - (free_cam.eye[2] - 1.0)).abs() < 1e-4);
    }
}
