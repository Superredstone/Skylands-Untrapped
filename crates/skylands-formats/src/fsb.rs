use byteorder::{BigEndian, ByteOrder, ReadBytesExt};
use std::io::Cursor;
use thiserror::Error;

use crate::dsp::{decode_dsp_adpcm, DspError};

#[derive(Error, Debug)]
pub enum FsbError {
    #[error("Invalid magic: expected FSB5")]
    InvalidMagic,
    #[error("Unsupported codec: {0}")]
    UnsupportedCodec(u32),
    #[error("Missing DSP ADPCM coefficients")]
    MissingCoefficients,
    #[error("DSP decode error: {0}")]
    Dsp(#[from] DspError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Format error: {0}")]
    Format(String),
}

pub const CODEC_GCADPCM: u32 = 0x06;

#[derive(Debug, Clone, PartialEq)]
pub struct AudioClip {
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: Vec<i16>,
}

#[derive(Debug, Clone)]
pub struct FsbHeader {
    pub version: u32,
    pub num_samples: u32,
    pub sample_header_size: u32,
    pub name_table_size: u32,
    pub data_size: u32,
    pub codec: u32,
}

pub fn parse_fsb5(data: &[u8]) -> Result<AudioClip, FsbError> {
    if data.len() < 28 {
        return Err(FsbError::Format("FSB5 data too short".into()));
    }
    if &data[0..4] != b"FSB5" {
        return Err(FsbError::InvalidMagic);
    }

    let mut cur = Cursor::new(&data[4..]);
    let version = cur.read_u32::<byteorder::LittleEndian>()?;
    let num_samples = cur.read_u32::<byteorder::LittleEndian>()?;
    let sample_header_size = cur.read_u32::<byteorder::LittleEndian>()?;
    let name_table_size = cur.read_u32::<byteorder::LittleEndian>()?;
    let data_size = cur.read_u32::<byteorder::LittleEndian>()?;
    let codec = cur.read_u32::<byteorder::LittleEndian>()?;

    if codec != CODEC_GCADPCM {
        return Err(FsbError::UnsupportedCodec(codec));
    }

    let header = FsbHeader {
        version,
        num_samples,
        sample_header_size,
        name_table_size,
        data_size,
        codec,
    };

    let sample_headers_offset = 28;
    let names_offset = sample_headers_offset + header.sample_header_size as usize;
    let data_offset = names_offset + header.name_table_size as usize;

    if data.len() < data_offset + header.data_size as usize {
        return Err(FsbError::Format("FSB5 payload truncated".into()));
    }

    let sample_hdr_data =
        &data[sample_headers_offset..sample_headers_offset + header.sample_header_size as usize];
    if sample_hdr_data.len() < 8 {
        return Err(FsbError::Format("Sample header too short".into()));
    }

    let val = byteorder::LittleEndian::read_u64(&sample_hdr_data[0..8]);
    let has_extra = (val & 1) != 0;
    let base_freq_idx = ((val >> 1) & 0x0F) as usize;
    let channels = (((val >> 5) & 0x07) + 1) as u16;
    let data_sub_offset = ((val >> 8) & 0x01FFFFFF) as usize * 32;
    let num_pcm_samples = (val >> 33) as usize;

    let standard_freqs = [
        4000, 8000, 11000, 11025, 16000, 22050, 24000, 32000, 44100, 48000, 96000,
    ];
    let mut sample_rate = if base_freq_idx < standard_freqs.len() {
        standard_freqs[base_freq_idx]
    } else {
        44100
    };

    let mut coefs = Vec::new();
    let mut offset = 8;

    if has_extra {
        while offset + 4 <= sample_hdr_data.len() {
            let chunk_hdr = byteorder::LittleEndian::read_u32(&sample_hdr_data[offset..offset + 4]);
            offset += 4;
            let more_chunks = (chunk_hdr & 1) != 0;
            let chunk_type = (chunk_hdr >> 1) & 0x7F;
            let chunk_size = (chunk_hdr >> 8) as usize;

            if offset + chunk_size > sample_hdr_data.len() {
                return Err(FsbError::Format("Extra chunk exceeds header".into()));
            }

            let chunk_data = &sample_hdr_data[offset..offset + chunk_size];
            offset += chunk_size;

            match chunk_type {
                0x02 => {
                    if chunk_size >= 4 {
                        sample_rate = byteorder::LittleEndian::read_u32(&chunk_data[0..4]);
                    }
                }
                0x07 => {
                    for ch_idx in 0..(channels as usize) {
                        let ch_offset = ch_idx * 46;
                        if chunk_data.len() >= ch_offset + 32 {
                            let mut ch_coefs = [0i16; 16];
                            for i in 0..16 {
                                ch_coefs[i] = BigEndian::read_i16(
                                    &chunk_data[ch_offset + i * 2..ch_offset + i * 2 + 2],
                                );
                            }
                            coefs.push(ch_coefs);
                        }
                    }
                }
                _ => {}
            }

            if !more_chunks {
                break;
            }
        }
    }

    if coefs.is_empty() {
        return Err(FsbError::MissingCoefficients);
    }

    let audio_payload = &data[data_offset + data_sub_offset..];
    let samples = decode_dsp_adpcm(audio_payload, channels, num_pcm_samples, &coefs)?;

    Ok(AudioClip {
        channels,
        sample_rate,
        samples,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fsb5_parse_synthetic() {
        let mut data = Vec::new();
        data.extend_from_slice(b"FSB5");
        data.extend_from_slice(&1u32.to_le_bytes()); // version
        data.extend_from_slice(&1u32.to_le_bytes()); // num_samples

        let mut sample_hdr = Vec::new();
        let base_bits: u64 = 1 | (4 << 1) | (14 << 33);
        sample_hdr.extend_from_slice(&base_bits.to_le_bytes());

        let chunk_type: u32 = (0x07 << 1) | (46 << 8); // chunk 0x07, size 46, no more
        sample_hdr.extend_from_slice(&chunk_type.to_le_bytes());
        let mut coef_bytes = [0u8; 46];
        BigEndian::write_i16(&mut coef_bytes[0..2], 2048);
        sample_hdr.extend_from_slice(&coef_bytes);

        let sample_hdr_size = sample_hdr.len() as u32;
        data.extend_from_slice(&sample_hdr_size.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes()); // name table size

        let audio_data = [0x00, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        data.extend_from_slice(&(audio_data.len() as u32).to_le_bytes()); // data_size
        data.extend_from_slice(&CODEC_GCADPCM.to_le_bytes()); // codec

        data.extend_from_slice(&sample_hdr);
        data.extend_from_slice(&audio_data);

        let clip = parse_fsb5(&data).expect("FSB5 decode");
        assert_eq!(clip.channels, 1);
        assert_eq!(clip.sample_rate, 16000);
        assert_eq!(clip.samples.len(), 14);
        assert_eq!(clip.samples[0], 1);
        assert_eq!(clip.samples[1], 3);
    }
}
