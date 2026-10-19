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

pub struct Camera {
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub up: [f32; 3],
    pub fov_y: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn new(aspect: f32) -> Self {
        Self {
            eye: [0.0, 4.0, 7.0],
            target: [0.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            fov_y: 60.0f32.to_radians(),
            aspect,
            near: 0.1,
            far: 100.0,
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
}
