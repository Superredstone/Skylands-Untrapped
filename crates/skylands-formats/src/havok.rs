use crate::igb::{AnimationClip, BoneTrack, Keyframe, Skeleton};
use std::f32::consts::PI;

pub const HAVOK_TAG: [u8; 8] = [0x57, 0xe0, 0xe0, 0x57, 0x10, 0xc0, 0xc0, 0x10];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarQuantization {
    Bits8 = 0,
    Bits16 = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationQuantization {
    Polar32 = 0,
    ThreeComp40 = 1,
    ThreeComp48 = 2,
    ThreeComp24 = 3,
    Straight16 = 4,
    Uncompressed = 5,
}

pub fn read_quat_polar32(val: u32) -> [f32; 4] {
    let r_mask: u32 = (1 << 10) - 1;
    let r_frac: f32 = 1.0 / (r_mask as f32);
    let pi2 = 0.5 * PI;
    let pi4 = 0.5 * pi2;
    let phi_frac = pi2 / 511.0;

    let r_raw = ((val >> 18) & r_mask) as f32;
    let mut r = r_raw * r_frac;
    r = 1.0 - (r * r);

    let phi_theta = (val & 0x3ffff) as f32;
    let phi_floor = phi_theta.sqrt().floor();
    let mut theta = 0.0f32;
    let mut phi = phi_floor;
    if phi_floor > 0.0 {
        theta = pi4 * (phi_theta - phi_floor * phi_floor) / phi_floor;
        phi = phi_frac * phi_floor;
    }

    let mag_sq = 1.0 - r * r;
    let magnitude = if mag_sq > 0.0 { mag_sq.sqrt() } else { 0.0 };

    let mut x = phi.sin() * theta.cos() * magnitude;
    let mut y = phi.sin() * theta.sin() * magnitude;
    let mut z = phi.cos() * magnitude;
    let mut w = r;

    if (val & 0x1000_0000) != 0 {
        x = -x;
    }
    if (val & 0x2000_0000) != 0 {
        y = -y;
    }
    if (val & 0x4000_0000) != 0 {
        z = -z;
    }
    if (val & 0x8000_0000) != 0 {
        w = -w;
    }

    let len = (x * x + y * y + z * z + w * w).sqrt();
    if len > 1e-6 {
        [x / len, y / len, z / len, w / len]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

pub fn read_quat_threecomp40(bytes5: &[u8; 5]) -> [f32; 4] {
    let c_val = (bytes5[0] as u64)
        | ((bytes5[1] as u64) << 8)
        | ((bytes5[2] as u64) << 16)
        | ((bytes5[3] as u64) << 24)
        | ((bytes5[4] as u64) << 32);

    let mask = (1u64 << 12) - 1;
    let pos_mask = mask >> 1;
    let fractal = 0.000345436f32;

    let x = ((c_val & mask) as i64 - pos_mask as i64) as f32 * fractal;
    let y = (((c_val >> 12) & mask) as i64 - pos_mask as i64) as f32 * fractal;
    let z = (((c_val >> 24) & mask) as i64 - pos_mask as i64) as f32 * fractal;

    let result_shift = ((c_val >> 36) & 3) as usize;
    let temp = [x, y, z];
    let mut ret = [0.0f32; 4];
    for i in 0..4 {
        if i < result_shift {
            ret[i] = temp[i];
        } else if i > result_shift {
            ret[i] = temp[i - 1];
        }
    }
    let missing_sq = 1.0 - x * x - y * y - z * z;
    let mut missing = if missing_sq > 0.0 {
        missing_sq.sqrt()
    } else {
        0.0
    };
    if ((c_val >> 38) & 1) != 0 {
        missing = -missing;
    }
    ret[result_shift] = missing;

    let len = (ret[0] * ret[0] + ret[1] * ret[1] + ret[2] * ret[2] + ret[3] * ret[3]).sqrt();
    if len > 1e-6 {
        [ret[0] / len, ret[1] / len, ret[2] / len, ret[3] / len]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

pub fn read_quat_threecomp48(bytes6: &[u8; 6]) -> [f32; 4] {
    let mut x = i16::from_le_bytes([bytes6[0], bytes6[1]]);
    let mut y = i16::from_le_bytes([bytes6[2], bytes6[3]]);
    let z_raw = i16::from_le_bytes([bytes6[4], bytes6[5]]);

    let result_shift = (((y >> 14) & 2) | ((x >> 15) & 1)) as usize;
    let r_sign = (z_raw >> 15) != 0;

    let mask = 0x7fff_i16;
    x &= mask;
    x -= mask >> 1;
    y &= mask;
    y -= mask >> 1;
    let mut z = z_raw & mask;
    z -= mask >> 1;

    let fractal = 0.000043161f32;
    let fx = x as f32 * fractal;
    let fy = y as f32 * fractal;
    let fz = z as f32 * fractal;
    let temp = [fx, fy, fz];

    let mut ret = [0.0f32; 4];
    for i in 0..4 {
        if i < result_shift {
            ret[i] = temp[i];
        } else if i > result_shift {
            ret[i] = temp[i - 1];
        }
    }
    let missing_sq = 1.0 - fx * fx - fy * fy - fz * fz;
    let mut missing = if missing_sq > 0.0 {
        missing_sq.sqrt()
    } else {
        0.0
    };
    if r_sign {
        missing = -missing;
    }
    ret[result_shift] = missing;

    let len = (ret[0] * ret[0] + ret[1] * ret[1] + ret[2] * ret[2] + ret[3] * ret[3]).sqrt();
    if len > 1e-6 {
        [ret[0] / len, ret[1] / len, ret[2] / len, ret[3] / len]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

pub fn find_knot_span(degree: usize, frame: f32, cpoints_len: usize, knots: &[u8]) -> usize {
    if cpoints_len == 0 {
        return 0;
    }
    if knots.len() > cpoints_len && frame >= knots[cpoints_len] as f32 {
        return cpoints_len - 1;
    }
    let mut low = degree;
    let mut high = cpoints_len;
    let mut mid = (low + high) / 2;
    while mid + 1 < knots.len() && (frame < knots[mid] as f32 || frame >= knots[mid + 1] as f32) {
        if frame < knots[mid] as f32 {
            high = mid;
        } else {
            low = mid;
        }
        mid = (low + high) / 2;
        if low >= high {
            break;
        }
    }
    mid
}

pub fn eval_nurbs_float(
    knot_span: usize,
    degree: usize,
    frame: f32,
    knots: &[u8],
    cpoints: &[f32],
) -> f32 {
    let mut n = [1.0f32, 0.0, 0.0, 0.0, 0.0];
    for i in 1..=degree {
        for j in (0..i).rev() {
            let k_idx1 = knot_span + i - j;
            let k_idx0 = knot_span - j;
            let denom = if k_idx1 < knots.len() && k_idx0 < knots.len() {
                knots[k_idx1] as f32 - knots[k_idx0] as f32
            } else {
                0.0
            };
            let a = if denom != 0.0 {
                (frame - knots[k_idx0] as f32) / denom
            } else {
                0.0
            };
            let tmp = n[j] * a;
            n[j + 1] += n[j] - tmp;
            n[j] = tmp;
        }
    }
    let mut val = 0.0f32;
    for (i, &factor) in n.iter().enumerate().take(degree + 1) {
        if knot_span >= i && knot_span - i < cpoints.len() {
            val += cpoints[knot_span - i] * factor;
        }
    }
    val
}

pub fn eval_nurbs_quat(
    knot_span: usize,
    degree: usize,
    frame: f32,
    knots: &[u8],
    cpoints: &[[f32; 4]],
) -> [f32; 4] {
    let mut n = [1.0f32, 0.0, 0.0, 0.0, 0.0];
    for i in 1..=degree {
        for j in (0..i).rev() {
            let k_idx1 = knot_span + i - j;
            let k_idx0 = knot_span - j;
            let denom = if k_idx1 < knots.len() && k_idx0 < knots.len() {
                knots[k_idx1] as f32 - knots[k_idx0] as f32
            } else {
                0.0
            };
            let a = if denom != 0.0 {
                (frame - knots[k_idx0] as f32) / denom
            } else {
                0.0
            };
            let tmp = n[j] * a;
            n[j + 1] += n[j] - tmp;
            n[j] = tmp;
        }
    }
    let mut q = [0.0f32; 4];
    for (i, &factor) in n.iter().enumerate().take(degree + 1) {
        if knot_span >= i && knot_span - i < cpoints.len() {
            let cp = cpoints[knot_span - i];
            for c in 0..4 {
                q[c] += cp[c] * factor;
            }
        }
    }
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if len > 1e-6 {
        [q[0] / len, q[1] / len, q[2] / len, q[3] / len]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

pub struct SplineVectorChannel {
    pub degree: usize,
    pub knots: Vec<u8>,
    pub bounds: [Option<(f32, f32)>; 3],
    pub cpoints: Vec<[f32; 3]>,
}

pub struct SplineQuatChannel {
    pub degree: usize,
    pub knots: Vec<u8>,
    pub cpoints: Vec<[f32; 4]>,
}

pub struct HavokSplineTrack {
    pub bone_index: usize,
    pub static_pos: Option<[f32; 3]>,
    pub spline_pos: Option<SplineVectorChannel>,
    pub static_rot: Option<[f32; 4]>,
    pub spline_rot: Option<SplineQuatChannel>,
    pub static_scale: Option<[f32; 3]>,
    pub spline_scale: Option<SplineVectorChannel>,
}

impl HavokSplineTrack {
    pub fn sample_frame(&self, frame: f32) -> ([f32; 3], [f32; 4], [f32; 3]) {
        let pos = if let Some(p) = self.static_pos {
            p
        } else if let Some(ref ch) = self.spline_pos {
            let span = find_knot_span(ch.degree, frame, ch.cpoints.len(), &ch.knots);
            let mut p = [0.0f32; 3];
            let mut n = [1.0f32, 0.0, 0.0, 0.0, 0.0];
            for i in 1..=ch.degree {
                for j in (0..i).rev() {
                    let k_idx1 = span + i - j;
                    let k_idx0 = span - j;
                    let denom = if k_idx1 < ch.knots.len() && k_idx0 < ch.knots.len() {
                        ch.knots[k_idx1] as f32 - ch.knots[k_idx0] as f32
                    } else {
                        0.0
                    };
                    let a = if denom != 0.0 {
                        (frame - ch.knots[k_idx0] as f32) / denom
                    } else {
                        0.0
                    };
                    let tmp = n[j] * a;
                    n[j + 1] += n[j] - tmp;
                    n[j] = tmp;
                }
            }
            for (i, &factor) in n.iter().enumerate().take(ch.degree + 1) {
                if span >= i && span - i < ch.cpoints.len() {
                    let cp = ch.cpoints[span - i];
                    for c in 0..3 {
                        p[c] += cp[c] * factor;
                    }
                }
            }
            p
        } else {
            [0.0, 0.0, 0.0]
        };

        let rot = if let Some(r) = self.static_rot {
            r
        } else if let Some(ref ch) = self.spline_rot {
            let span = find_knot_span(ch.degree, frame, ch.cpoints.len(), &ch.knots);
            eval_nurbs_quat(span, ch.degree, frame, &ch.knots, &ch.cpoints)
        } else {
            [0.0, 0.0, 0.0, 1.0]
        };

        let scale = if let Some(s) = self.static_scale {
            s
        } else if let Some(ref ch) = self.spline_scale {
            let span = find_knot_span(ch.degree, frame, ch.cpoints.len(), &ch.knots);
            let mut s = [1.0f32; 3];
            let mut n = [1.0f32, 0.0, 0.0, 0.0, 0.0];
            for i in 1..=ch.degree {
                for j in (0..i).rev() {
                    let k_idx1 = span + i - j;
                    let k_idx0 = span - j;
                    let denom = if k_idx1 < ch.knots.len() && k_idx0 < ch.knots.len() {
                        ch.knots[k_idx1] as f32 - ch.knots[k_idx0] as f32
                    } else {
                        0.0
                    };
                    let a = if denom != 0.0 {
                        (frame - ch.knots[k_idx0] as f32) / denom
                    } else {
                        0.0
                    };
                    let tmp = n[j] * a;
                    n[j + 1] += n[j] - tmp;
                    n[j] = tmp;
                }
            }
            for (i, &factor) in n.iter().enumerate().take(ch.degree + 1) {
                if span >= i && span - i < ch.cpoints.len() {
                    let cp = ch.cpoints[span - i];
                    for c in 0..3 {
                        s[c] += cp[c] * factor;
                    }
                }
            }
            s
        } else {
            [1.0, 1.0, 1.0]
        };

        (pos, rot, scale)
    }
}

pub fn parse_havok_animation(
    data: &[u8],
    clip_name: &str,
    skeleton: Option<&Skeleton>,
) -> Option<AnimationClip> {
    if data.len() < 0x200 || data.get(0..8)? != HAVOK_TAG {
        return None;
    }

    let num_sections = u32::from_be_bytes(data.get(20..24)?.try_into().ok()?) as usize;
    if num_sections < 3 {
        return None;
    }

    let mut cur = 64;
    let mut data_section_offset = None;
    let mut data_section_fixups = None;
    for _ in 0..num_sections {
        if cur + 48 > data.len() {
            return None;
        }
        let tag_slice = &data[cur..cur + 16];
        let abs_off = u32::from_be_bytes(data[cur + 20..cur + 24].try_into().ok()?) as usize;
        let data_off = u32::from_be_bytes(data[cur + 24..cur + 28].try_into().ok()?) as usize;
        let l_fix = u32::from_be_bytes(data[cur + 28..cur + 32].try_into().ok()?) as usize;
        let g_fix = u32::from_be_bytes(data[cur + 32..cur + 36].try_into().ok()?) as usize;
        let v_fix = u32::from_be_bytes(data[cur + 36..cur + 40].try_into().ok()?) as usize;
        if tag_slice.starts_with(b"__data__") {
            data_section_offset = Some(abs_off);
            data_section_fixups = Some((data_off, l_fix, g_fix, v_fix));
            break;
        }
        cur += 48;
    }

    let sec2_off = data_section_offset.unwrap_or(0x190);
    if sec2_off >= data.len() {
        return None;
    }

    let sec2_data = &data[sec2_off..];
    let anim_off = 0xd0;
    if anim_off + 0x84 > sec2_data.len() {
        return None;
    }

    let anim_type = u32::from_be_bytes(
        sec2_data[anim_off + 0x08..anim_off + 0x0c]
            .try_into()
            .ok()?,
    );
    if anim_type != 3 {
        return None;
    }

    let duration = f32::from_be_bytes(
        sec2_data[anim_off + 0x0c..anim_off + 0x10]
            .try_into()
            .ok()?,
    );
    let num_tracks = u32::from_be_bytes(
        sec2_data[anim_off + 0x10..anim_off + 0x14]
            .try_into()
            .ok()?,
    ) as usize;
    let num_frames = u32::from_be_bytes(
        sec2_data[anim_off + 0x28..anim_off + 0x2c]
            .try_into()
            .ok()?,
    ) as usize;
    let frame_duration = f32::from_be_bytes(
        sec2_data[anim_off + 0x40..anim_off + 0x44]
            .try_into()
            .ok()?,
    );
    let data_len = u32::from_be_bytes(
        sec2_data[anim_off + 0x78..anim_off + 0x7c]
            .try_into()
            .ok()?,
    ) as usize;

    let mut binding_off = None;
    let mut block_data_off = 0x180;
    let mut bone_indices_off = None;

    if let Some((data_off, l_fix, g_fix, v_fix)) = data_section_fixups {
        if g_fix <= v_fix && v_fix <= sec2_data.len() {
            let v_data = &sec2_data[g_fix..v_fix];
            for k in (0..v_data.len().saturating_sub(11)).step_by(12) {
                let src = u32::from_be_bytes(v_data[k..k + 4].try_into().ok()?) as usize;
                let cls_off = u32::from_be_bytes(v_data[k + 8..k + 12].try_into().ok()?);
                if cls_off == 0xa2 {
                    binding_off = Some(src);
                    break;
                }
            }
        }
        if data_off <= l_fix && l_fix <= sec2_data.len() {
            let l_data = &sec2_data[data_off..l_fix];
            for k in (0..l_data.len().saturating_sub(7)).step_by(8) {
                let src = u32::from_be_bytes(l_data[k..k + 4].try_into().ok()?) as usize;
                let dst = u32::from_be_bytes(l_data[k + 4..k + 8].try_into().ok()?) as usize;
                if src == anim_off + 0x74 {
                    block_data_off = dst;
                } else if binding_off.is_some_and(|b| src == b + 0x10) {
                    bone_indices_off = Some(dst);
                }
            }
        }
    }

    let mut bone_indices: Vec<usize> = (0..num_tracks).collect();
    if let (Some(b_off), Some(indices_off)) = (binding_off, bone_indices_off) {
        if b_off + 0x18 <= sec2_data.len() {
            let binding_track_count =
                u32::from_be_bytes(sec2_data[b_off + 0x14..b_off + 0x18].try_into().ok()?) as usize;
            if binding_track_count == num_tracks && indices_off + num_tracks * 2 <= sec2_data.len()
            {
                let mut list = Vec::with_capacity(num_tracks);
                for i in 0..num_tracks {
                    let idx = i16::from_be_bytes(
                        sec2_data[indices_off + i * 2..indices_off + (i + 1) * 2]
                            .try_into()
                            .ok()?,
                    );
                    list.push(idx.max(0) as usize);
                }
                bone_indices = list;
            }
        }
    }

    let bone_offset = if let Some(skel) = skeleton {
        if !skel.bones.is_empty()
            && skel.bones[0].name.eq_ignore_ascii_case("unnamed bone")
            && skel.bones.len() > num_tracks
        {
            1
        } else {
            0
        }
    } else {
        1
    };

    if block_data_off + data_len > sec2_data.len() {
        return None;
    }
    let block = &sec2_data[block_data_off..block_data_off + data_len];

    let mut pos = 0;
    let mut masks = Vec::with_capacity(num_tracks);
    for _ in 0..num_tracks {
        if pos + 4 > block.len() {
            return None;
        }
        let q_type = block[pos];
        let pos_type = block[pos + 1];
        let rot_type = block[pos + 2];
        let scale_type = block[pos + 3];
        let pos_q = q_type & 3;
        let rot_q = (q_type >> 2) & 0xf;
        let scale_q = (q_type >> 6) & 3;
        masks.push((pos_q, rot_q, scale_q, pos_type, rot_type, scale_type));
        pos += 4;
    }
    pos = (pos + 3) & !3;

    let mut tracks = Vec::with_capacity(num_tracks);

    for t in 0..num_tracks {
        let (pos_q, rot_q, scale_q, pos_type, rot_type, scale_type) = masks[t];
        let target_bone = bone_indices[t] + bone_offset;

        let has_spline_pos = (pos_type & 0x70) != 0;
        let has_spline_rot = (rot_type & 0xf0) != 0;
        let has_static_rot = (rot_type & 0x0f) != 0;
        let has_spline_scale = (scale_type & 0x70) != 0;

        let mut spline_track = HavokSplineTrack {
            bone_index: target_bone,
            static_pos: None,
            spline_pos: None,
            static_rot: None,
            spline_rot: None,
            static_scale: None,
            spline_scale: None,
        };

        if has_spline_pos {
            if pos + 3 > block.len() {
                return None;
            }
            let num_items = u16::from_le_bytes([block[pos], block[pos + 1]]) as usize;
            let degree = block[pos + 2] as usize;
            let knot_cnt = num_items + degree + 2;
            pos += 3;
            if pos + knot_cnt > block.len() {
                return None;
            }
            let knots = block[pos..pos + knot_cnt].to_vec();
            pos += knot_cnt;
            pos = (pos + 3) & !3;

            let mut bounds = [None, None, None];
            let mut static_vals = [0.0f32; 3];
            for (idx, bit) in [0x10, 0x20, 0x40].iter().enumerate() {
                if (pos_type & bit) != 0 {
                    if pos + 8 > block.len() {
                        return None;
                    }
                    let b_min = f32::from_le_bytes(block[pos..pos + 4].try_into().ok()?);
                    let b_max = f32::from_le_bytes(block[pos + 4..pos + 8].try_into().ok()?);
                    bounds[idx] = Some((b_min, b_max));
                    pos += 8;
                } else if (pos_type & (bit >> 4)) != 0 {
                    if pos + 4 > block.len() {
                        return None;
                    }
                    static_vals[idx] = f32::from_le_bytes(block[pos..pos + 4].try_into().ok()?);
                    pos += 4;
                }
            }

            let _active_cnt = ((pos_type >> 4) & 7).count_ones() as usize;
            let bytes_per_val = if pos_q == 0 { 1 } else { 2 };
            let mut cpoints = Vec::with_capacity(num_items + 1);

            for _ in 0..=num_items {
                let mut cp = static_vals;
                for idx in 0..3 {
                    if let Some((b_min, b_max)) = bounds[idx] {
                        if pos + bytes_per_val > block.len() {
                            return None;
                        }
                        let ratio = if pos_q == 0 {
                            block[pos] as f32 / 255.0
                        } else {
                            u16::from_le_bytes([block[pos], block[pos + 1]]) as f32 / 65535.0
                        };
                        pos += bytes_per_val;
                        cp[idx] = b_min + (b_max - b_min) * ratio;
                    }
                }
                cpoints.push(cp);
            }
            spline_track.spline_pos = Some(SplineVectorChannel {
                degree,
                knots,
                bounds,
                cpoints,
            });
        } else {
            let mut p = [0.0f32; 3];
            for (idx, bit) in [1, 2, 4].iter().enumerate() {
                if (pos_type & bit) != 0 {
                    if pos + 4 > block.len() {
                        return None;
                    }
                    p[idx] = f32::from_le_bytes(block[pos..pos + 4].try_into().ok()?);
                    pos += 4;
                }
            }
            spline_track.static_pos = Some(p);
        }
        pos = (pos + 3) & !3;

        let rot_align = match rot_q {
            0 => 4,
            1 => 1,
            2 => 2,
            3 => 1,
            4 => 2,
            5 => 4,
            _ => 1,
        };

        if has_spline_rot {
            if pos + 3 > block.len() {
                return None;
            }
            let num_items = u16::from_le_bytes([block[pos], block[pos + 1]]) as usize;
            let degree = block[pos + 2] as usize;
            let knot_cnt = num_items + degree + 2;
            pos += 3;
            if pos + knot_cnt > block.len() {
                return None;
            }
            let knots = block[pos..pos + knot_cnt].to_vec();
            pos += knot_cnt;
            pos = (pos + rot_align - 1) & !(rot_align - 1);

            let mut cpoints = Vec::with_capacity(num_items + 1);
            let rot_sz = match rot_q {
                0 => 4,
                1 => 5,
                2 => 6,
                3 => 3,
                4 => 2,
                5 => 16,
                _ => 4,
            };

            for _ in 0..=num_items {
                if pos + rot_sz > block.len() {
                    return None;
                }
                let q = match rot_q {
                    0 => {
                        let val = u32::from_le_bytes(block[pos..pos + 4].try_into().ok()?);
                        read_quat_polar32(val)
                    }
                    1 => read_quat_threecomp40(block[pos..pos + 5].try_into().ok()?),
                    2 => read_quat_threecomp48(block[pos..pos + 6].try_into().ok()?),
                    5 => [
                        f32::from_le_bytes(block[pos..pos + 4].try_into().ok()?),
                        f32::from_le_bytes(block[pos + 4..pos + 8].try_into().ok()?),
                        f32::from_le_bytes(block[pos + 8..pos + 12].try_into().ok()?),
                        f32::from_le_bytes(block[pos + 12..pos + 16].try_into().ok()?),
                    ],
                    _ => [0.0, 0.0, 0.0, 1.0],
                };
                pos += rot_sz;
                cpoints.push(q);
            }
            spline_track.spline_rot = Some(SplineQuatChannel {
                degree,
                knots,
                cpoints,
            });
        } else if has_static_rot {
            pos = (pos + rot_align - 1) & !(rot_align - 1);
            let rot_sz = match rot_q {
                0 => 4,
                1 => 5,
                2 => 6,
                3 => 3,
                4 => 2,
                5 => 16,
                _ => 4,
            };
            if pos + rot_sz > block.len() {
                return None;
            }
            let q = match rot_q {
                0 => {
                    let val = u32::from_le_bytes(block[pos..pos + 4].try_into().ok()?);
                    read_quat_polar32(val)
                }
                1 => read_quat_threecomp40(block[pos..pos + 5].try_into().ok()?),
                2 => read_quat_threecomp48(block[pos..pos + 6].try_into().ok()?),
                5 => [
                    f32::from_le_bytes(block[pos..pos + 4].try_into().ok()?),
                    f32::from_le_bytes(block[pos + 4..pos + 8].try_into().ok()?),
                    f32::from_le_bytes(block[pos + 8..pos + 12].try_into().ok()?),
                    f32::from_le_bytes(block[pos + 12..pos + 16].try_into().ok()?),
                ],
                _ => [0.0, 0.0, 0.0, 1.0],
            };
            pos += rot_sz;
            spline_track.static_rot = Some(q);
        } else {
            spline_track.static_rot = Some([0.0, 0.0, 0.0, 1.0]);
        }
        pos = (pos + 3) & !3;

        if has_spline_scale {
            if pos + 3 > block.len() {
                return None;
            }
            let num_items = u16::from_le_bytes([block[pos], block[pos + 1]]) as usize;
            let degree = block[pos + 2] as usize;
            let knot_cnt = num_items + degree + 2;
            pos += 3;
            if pos + knot_cnt > block.len() {
                return None;
            }
            let knots = block[pos..pos + knot_cnt].to_vec();
            pos += knot_cnt;
            pos = (pos + 3) & !3;

            let mut bounds = [None, None, None];
            let mut static_vals = [1.0f32; 3];
            for (idx, bit) in [0x10, 0x20, 0x40].iter().enumerate() {
                if (scale_type & bit) != 0 {
                    if pos + 8 > block.len() {
                        return None;
                    }
                    let b_min = f32::from_le_bytes(block[pos..pos + 4].try_into().ok()?);
                    let b_max = f32::from_le_bytes(block[pos + 4..pos + 8].try_into().ok()?);
                    bounds[idx] = Some((b_min, b_max));
                    pos += 8;
                } else if (scale_type & (bit >> 4)) != 0 {
                    if pos + 4 > block.len() {
                        return None;
                    }
                    static_vals[idx] = f32::from_le_bytes(block[pos..pos + 4].try_into().ok()?);
                    pos += 4;
                }
            }

            let bytes_per_val = if scale_q == 0 { 1 } else { 2 };
            let mut cpoints = Vec::with_capacity(num_items + 1);

            for _ in 0..=num_items {
                let mut cp = static_vals;
                for idx in 0..3 {
                    if let Some((b_min, b_max)) = bounds[idx] {
                        if pos + bytes_per_val > block.len() {
                            return None;
                        }
                        let ratio = if scale_q == 0 {
                            block[pos] as f32 / 255.0
                        } else {
                            u16::from_le_bytes([block[pos], block[pos + 1]]) as f32 / 65535.0
                        };
                        pos += bytes_per_val;
                        cp[idx] = b_min + (b_max - b_min) * ratio;
                    }
                }
                cpoints.push(cp);
            }
            spline_track.spline_scale = Some(SplineVectorChannel {
                degree,
                knots,
                bounds,
                cpoints,
            });
        } else {
            let mut s = [1.0f32; 3];
            for (idx, bit) in [1, 2, 4].iter().enumerate() {
                if (scale_type & bit) != 0 {
                    if pos + 4 > block.len() {
                        return None;
                    }
                    s[idx] = f32::from_le_bytes(block[pos..pos + 4].try_into().ok()?);
                    pos += 4;
                }
            }
            spline_track.static_scale = Some(s);
        }
        pos = (pos + 3) & !3;

        tracks.push(spline_track);
    }

    let sample_count = num_frames.max(1);
    let dt = if sample_count > 1 {
        duration / (sample_count - 1) as f32
    } else {
        frame_duration
    };

    let mut bone_tracks = Vec::with_capacity(num_tracks);
    for st in &tracks {
        let is_fully_static =
            st.spline_pos.is_none() && st.spline_rot.is_none() && st.spline_scale.is_none();
        if is_fully_static {
            let (p, r, s) = st.sample_frame(0.0);
            bone_tracks.push(BoneTrack {
                bone_index: st.bone_index,
                positions: vec![Keyframe::new(0.0, p)],
                rotations: vec![Keyframe::new(0.0, r)],
                scales: vec![Keyframe::new(0.0, s)],
            });
        } else {
            let mut positions = Vec::with_capacity(sample_count);
            let mut rotations = Vec::with_capacity(sample_count);
            let mut scales = Vec::with_capacity(sample_count);
            for f in 0..sample_count {
                let t = f as f32 * dt;
                let (p, r, s) = st.sample_frame(f as f32);
                positions.push(Keyframe::new(t, p));
                rotations.push(Keyframe::new(t, r));
                scales.push(Keyframe::new(t, s));
            }
            bone_tracks.push(BoneTrack {
                bone_index: st.bone_index,
                positions,
                rotations,
                scales,
            });
        }
    }

    Some(AnimationClip::new(clip_name, duration, bone_tracks))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_polar32_decoding() {
        let q = read_quat_polar32(0);
        let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        assert!((len - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_threecomp40_decoding() {
        let bytes = [0u8; 5];
        let q = read_quat_threecomp40(&bytes);
        let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        assert!((len - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_find_knot_span() {
        let knots = vec![0, 0, 3, 11, 14, 18, 24, 24];
        let span = find_knot_span(1, 12.0, 5, &knots);
        assert!(span >= 1 && span < knots.len());
    }

    #[test]
    fn test_havok_spyro_idle_extraction() {
        use crate::archive::ArchiveMount;
        use std::path::Path;

        let targets = [
            "assets/files/character/016_Spyro.bld",
            "../../assets/files/character/016_Spyro.bld",
        ];
        let p = match targets.iter().map(Path::new).find(|p| p.exists()) {
            Some(p) => p,
            None => return,
        };

        let mut mount = ArchiveMount::new();
        mount.mount(p).expect("mount spyro.bld");
        let bld_data = mount.read_file("level.bld").expect("read level.bld");

        let raw_tag = [0x57, 0xe0, 0xe0, 0x57, 0x10, 0xc0, 0xc0, 0x10];
        let mut clips = Vec::new();
        let mut pos = 0;
        while pos + 8 <= bld_data.len() {
            if bld_data[pos..pos + 8] == raw_tag {
                if let Some(clip) = parse_havok_animation(&bld_data[pos..], "test", None) {
                    if clip.tracks.len() == 49 {
                        clips.push(clip);
                    }
                }
                pos += 8;
            } else {
                pos += 4;
            }
        }

        assert!(
            !clips.is_empty(),
            "found 49-track Havok animations in Spyro"
        );
        let idle = &clips[0];
        assert_eq!(idle.tracks.len(), 49);
        assert!((idle.duration - 0.8).abs() < 1e-2);
        for t in &idle.tracks {
            for k in &t.rotations {
                let q = k.value;
                let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
                assert!((len - 1.0).abs() < 1e-3, "quaternion must be normalized");
            }
        }
    }
}
