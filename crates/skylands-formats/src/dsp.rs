use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum DspError {
    #[error("Unexpected end of data: needed {needed} bytes, got {available}")]
    UnexpectedEof { needed: usize, available: usize },
    #[error("Invalid channel count: {0}")]
    InvalidChannelCount(u16),
}

pub const SAMPLES_PER_FRAME: usize = 14;
pub const BYTES_PER_FRAME: usize = 8;

pub struct DspChannelState {
    pub coefs: [i16; 16],
    pub hist1: i16,
    pub hist2: i16,
}

impl DspChannelState {
    pub fn new(coefs: [i16; 16]) -> Self {
        Self {
            coefs,
            hist1: 0,
            hist2: 0,
        }
    }

    pub fn decode_frame(&mut self, frame: &[u8; 8], out_samples: &mut [i16; 14]) {
        let header = frame[0];
        let scale = 1i32 << (header & 0x0F);
        let coef_index = ((header >> 4) & 0x07) as usize;

        let coef1 = self.coefs[coef_index * 2] as i32;
        let coef2 = self.coefs[coef_index * 2 + 1] as i32;

        let mut sample_idx = 0;
        for &byte in &frame[1..8] {
            let n0 = ((byte as i8) >> 4) as i32;
            let n1 = ((byte << 4) as i8 >> 4) as i32;

            for nibble in [n0, n1] {
                let sample_raw = ((nibble * scale) << 11)
                    + 1024
                    + coef1 * (self.hist1 as i32)
                    + coef2 * (self.hist2 as i32);
                let sample = (sample_raw >> 11).clamp(-32768, 32767) as i16;

                self.hist2 = self.hist1;
                self.hist1 = sample;
                out_samples[sample_idx] = sample;
                sample_idx += 1;
            }
        }
    }
}

pub fn decode_dsp_adpcm(
    data: &[u8],
    channels: u16,
    total_samples: usize,
    coefs: &[[i16; 16]],
) -> Result<Vec<i16>, DspError> {
    if channels == 0 || channels > 2 {
        return Err(DspError::InvalidChannelCount(channels));
    }
    if coefs.len() < channels as usize {
        return Err(DspError::InvalidChannelCount(channels));
    }

    if channels == 1 {
        let mut state = DspChannelState::new(coefs[0]);
        let mut decoded = Vec::with_capacity(total_samples);
        let mut frame_buf = [0i16; SAMPLES_PER_FRAME];
        let mut offset = 0;

        while decoded.len() < total_samples && offset + BYTES_PER_FRAME <= data.len() {
            let frame: &[u8; 8] = data[offset..offset + 8].try_into().unwrap();
            state.decode_frame(frame, &mut frame_buf);
            let remaining = total_samples - decoded.len();
            let take = remaining.min(SAMPLES_PER_FRAME);
            decoded.extend_from_slice(&frame_buf[..take]);
            offset += BYTES_PER_FRAME;
        }

        decoded.resize(total_samples, 0);
        Ok(decoded)
    } else {
        let mut state_l = DspChannelState::new(coefs[0]);
        let mut state_r = DspChannelState::new(coefs[1]);
        let mut decoded = Vec::with_capacity(total_samples * 2);

        let mut samples_left = Vec::with_capacity(total_samples);
        let mut samples_right = Vec::with_capacity(total_samples);

        let mut frame_buf = [0i16; SAMPLES_PER_FRAME];
        let mut offset = 0;

        while (samples_left.len() < total_samples || samples_right.len() < total_samples)
            && offset + BYTES_PER_FRAME <= data.len()
        {
            let mut left_frame = [0u8; 8];
            let mut right_frame = [0u8; 8];

            if offset + 16 <= data.len() {
                for i in 0..4 {
                    left_frame[i * 2] = data[offset + i * 4];
                    left_frame[i * 2 + 1] = data[offset + i * 4 + 1];
                    right_frame[i * 2] = data[offset + i * 4 + 2];
                    right_frame[i * 2 + 1] = data[offset + i * 4 + 3];
                }
                offset += 16;
            } else {
                break;
            }

            state_l.decode_frame(&left_frame, &mut frame_buf);
            let rem_l = total_samples.saturating_sub(samples_left.len());
            samples_left.extend_from_slice(&frame_buf[..rem_l.min(SAMPLES_PER_FRAME)]);

            state_r.decode_frame(&right_frame, &mut frame_buf);
            let rem_r = total_samples.saturating_sub(samples_right.len());
            samples_right.extend_from_slice(&frame_buf[..rem_r.min(SAMPLES_PER_FRAME)]);
        }

        samples_left.resize(total_samples, 0);
        samples_right.resize(total_samples, 0);

        for i in 0..total_samples {
            decoded.push(samples_left[i]);
            decoded.push(samples_right[i]);
        }

        Ok(decoded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dsp_silence_decode() {
        let coefs = [[0i16; 16]];
        let frame = [0u8; 8];
        let mut state = DspChannelState::new(coefs[0]);
        let mut out = [0i16; 14];
        state.decode_frame(&frame, &mut out);
        assert_eq!(out, [0i16; 14]);
    }

    #[test]
    fn test_dsp_adpcm_known_step() {
        let mut coefs = [0i16; 16];
        coefs[0] = 2048;
        coefs[1] = 0;
        let mut state = DspChannelState::new(coefs);

        let mut frame = [0u8; 8];
        frame[0] = 0x00;
        frame[1] = 0x12;

        let mut out = [0i16; 14];
        state.decode_frame(&frame, &mut out);

        assert_eq!(out[0], 1);
        assert_eq!(out[1], 3);
        assert_eq!(out[2], 3);
        assert_eq!(out[3], 3);
        assert_eq!(out[4], 3);
        assert_eq!(state.hist1, 3);
        assert_eq!(state.hist2, 3);
    }
}
