use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum TextureError {
    #[error("Buffer too small: expected {expected} bytes, got {actual}")]
    BufferTooSmall { expected: usize, actual: usize },
    #[error("Invalid dimensions: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },
}

#[inline]
pub fn decode_rgb565(c: u16) -> [u8; 4] {
    let r = ((c >> 11) & 0x1f) as u8;
    let g = ((c >> 5) & 0x3f) as u8;
    let b = (c & 0x1f) as u8;
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
        255,
    ]
}

#[inline]
pub fn decode_dxt1_block(bytes: &[u8; 8], is_cmpr: bool) -> [[u8; 4]; 16] {
    let (c0_raw, c1_raw) = if is_cmpr {
        (
            u16::from_be_bytes([bytes[0], bytes[1]]),
            u16::from_be_bytes([bytes[2], bytes[3]]),
        )
    } else {
        (
            u16::from_le_bytes([bytes[0], bytes[1]]),
            u16::from_le_bytes([bytes[2], bytes[3]]),
        )
    };

    let c0 = decode_rgb565(c0_raw);
    let c1 = decode_rgb565(c1_raw);

    let mut palette = [[0u8; 4]; 4];
    palette[0] = c0;
    palette[1] = c1;

    if c0_raw > c1_raw {
        palette[2] = [
            ((2 * c0[0] as u16 + c1[0] as u16) / 3) as u8,
            ((2 * c0[1] as u16 + c1[1] as u16) / 3) as u8,
            ((2 * c0[2] as u16 + c1[2] as u16) / 3) as u8,
            255,
        ];
        palette[3] = [
            ((c0[0] as u16 + 2 * c1[0] as u16) / 3) as u8,
            ((c0[1] as u16 + 2 * c1[1] as u16) / 3) as u8,
            ((c0[2] as u16 + 2 * c1[2] as u16) / 3) as u8,
            255,
        ];
    } else {
        palette[2] = [
            ((c0[0] as u16 + c1[0] as u16) / 2) as u8,
            ((c0[1] as u16 + c1[1] as u16) / 2) as u8,
            ((c0[2] as u16 + c1[2] as u16) / 2) as u8,
            255,
        ];
        palette[3] = [0, 0, 0, 0];
    }

    let mut pixels = [[0u8; 4]; 16];
    for y in 0..4 {
        let row_byte = bytes[4 + y];
        for x in 0..4 {
            let idx = if is_cmpr {
                ((row_byte >> (6 - 2 * x)) & 0x03) as usize
            } else {
                ((row_byte >> (2 * x)) & 0x03) as usize
            };
            pixels[y * 4 + x] = palette[idx];
        }
    }

    pixels
}

// ponytail: pad unaligned dimensions to 4-pixel blocks; upscale/resample deferred until asset loader
pub fn decode_dxt1(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, TextureError> {
    if width == 0 || height == 0 {
        return Err(TextureError::InvalidDimensions { width, height });
    }

    let blocks_x = width.div_ceil(4);
    let blocks_y = height.div_ceil(4);
    let expected = (blocks_x * blocks_y * 8) as usize;

    if data.len() < expected {
        return Err(TextureError::BufferTooSmall {
            expected,
            actual: data.len(),
        });
    }

    let mut rgba = vec![0u8; (width * height * 4) as usize];
    let mut offset = 0;

    for by in 0..blocks_y {
        for bx in 0..blocks_x {
            let block: &[u8; 8] = data[offset..offset + 8].try_into().unwrap();
            offset += 8;

            let block_pixels = decode_dxt1_block(block, false);
            for py in 0..4 {
                let y = by * 4 + py;
                if y >= height {
                    continue;
                }
                for px in 0..4 {
                    let x = bx * 4 + px;
                    if x >= width {
                        continue;
                    }
                    let dst_idx = ((y * width + x) * 4) as usize;
                    rgba[dst_idx..dst_idx + 4].copy_from_slice(&block_pixels[(py * 4 + px) as usize]);
                }
            }
        }
    }

    Ok(rgba)
}

// ponytail: pad unaligned dimensions to 8x8 GX tiles; clamp sampling deferred until mipmap pipeline
pub fn decode_cmpr(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, TextureError> {
    if width == 0 || height == 0 {
        return Err(TextureError::InvalidDimensions { width, height });
    }

    let tiles_x = width.div_ceil(8);
    let tiles_y = height.div_ceil(8);
    let expected = (tiles_x * tiles_y * 32) as usize;

    if data.len() < expected {
        return Err(TextureError::BufferTooSmall {
            expected,
            actual: data.len(),
        });
    }

    let mut rgba = vec![0u8; (width * height * 4) as usize];
    let mut offset = 0;

    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            let tile_base_x = tx * 8;
            let tile_base_y = ty * 8;

            // 4 sub-blocks per 8x8 tile: (0,0), (4,0), (0,4), (4,4)
            const SUB_BLOCK_OFFSETS: [(u32, u32); 4] = [(0, 0), (4, 0), (0, 4), (4, 4)];

            for (sbx, sby) in SUB_BLOCK_OFFSETS {
                let block: &[u8; 8] = data[offset..offset + 8].try_into().unwrap();
                offset += 8;

                let block_pixels = decode_dxt1_block(block, true);
                for py in 0..4 {
                    let y = tile_base_y + sby + py;
                    if y >= height {
                        continue;
                    }
                    for px in 0..4 {
                        let x = tile_base_x + sbx + px;
                        if x >= width {
                            continue;
                        }
                        let dst_idx = ((y * width + x) * 4) as usize;
                        rgba[dst_idx..dst_idx + 4]
                            .copy_from_slice(&block_pixels[(py * 4 + px) as usize]);
                    }
                }
            }
        }
    }

    Ok(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_rgb565() {
        assert_eq!(decode_rgb565(0x0000), [0, 0, 0, 255]);
        assert_eq!(decode_rgb565(0xFFFF), [255, 255, 255, 255]);
        assert_eq!(decode_rgb565(0xF800), [255, 0, 0, 255]);
        assert_eq!(decode_rgb565(0x07E0), [0, 255, 0, 255]);
        assert_eq!(decode_rgb565(0x001F), [0, 0, 255, 255]);
    }

    #[test]
    fn test_dxt1_block_4color() {
        let mut block = [0u8; 8];
        block[0..2].copy_from_slice(&0xF800u16.to_le_bytes()); // Red
        block[2..4].copy_from_slice(&0x001Fu16.to_le_bytes()); // Blue
        block[4] = 0b00_00_01_00; // px0=0 (idx 0), px1=1 (idx 1) in LE bit order
        block[5] = 0x00;
        block[6] = 0x00;
        block[7] = 0x00;

        let pixels = decode_dxt1_block(&block, false);
        assert_eq!(pixels[0], [255, 0, 0, 255]); // idx 0 = Red
        assert_eq!(pixels[1], [0, 0, 255, 255]); // idx 1 = Blue
    }

    #[test]
    fn test_cmpr_block_transparent() {
        let mut block = [0u8; 8];
        block[0..2].copy_from_slice(&0x001Fu16.to_be_bytes()); // Blue
        block[2..4].copy_from_slice(&0xF800u16.to_be_bytes()); // Red (c0 <= c1 -> 3 color + transparent)
        block[4] = 0b11_00_00_00; // px0 = idx 3 (transparent) in CMPR MSB bit order
        block[5] = 0x00;
        block[6] = 0x00;
        block[7] = 0x00;

        let pixels = decode_dxt1_block(&block, true);
        assert_eq!(pixels[0], [0, 0, 0, 0]); // Transparent
    }

    #[test]
    fn test_decode_dxt1_full() {
        let mut data = vec![0u8; 8];
        data[0..2].copy_from_slice(&0xFFFFu16.to_le_bytes()); // White
        data[2..4].copy_from_slice(&0x0000u16.to_le_bytes()); // Black
        data[4..8].fill(0x00); // All pixels idx 0 (White)

        let rgba = decode_dxt1(&data, 4, 4).expect("Decode DXT1");
        assert_eq!(rgba.len(), 4 * 4 * 4);
        assert_eq!(&rgba[0..4], &[255, 255, 255, 255]);
    }

    #[test]
    fn test_decode_cmpr_full() {
        // 8x8 tile = 32 bytes
        let mut data = vec![0u8; 32];
        for b in 0..4 {
            let base = b * 8;
            data[base..base + 2].copy_from_slice(&0xF800u16.to_be_bytes()); // Red
            data[base + 2..base + 4].copy_from_slice(&0x0000u16.to_be_bytes());
            data[base + 4..base + 8].fill(0x00); // idx 0 = Red
        }

        let rgba = decode_cmpr(&data, 8, 8).expect("Decode CMPR");
        assert_eq!(rgba.len(), 8 * 8 * 4);
        assert_eq!(&rgba[0..4], &[255, 0, 0, 255]);
        assert_eq!(&rgba[(63 * 4)..(64 * 4)], &[255, 0, 0, 255]);
    }

    #[test]
    fn test_bounds_errors() {
        assert_eq!(
            decode_cmpr(&[0u8; 10], 8, 8),
            Err(TextureError::BufferTooSmall {
                expected: 32,
                actual: 10
            })
        );
        assert_eq!(
            decode_dxt1(&[], 0, 4),
            Err(TextureError::InvalidDimensions {
                width: 0,
                height: 4
            })
        );
    }
}
