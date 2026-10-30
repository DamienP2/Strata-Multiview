//! Minimal QuickTime (.mov) writer: one video track of PNG frames with
//! alpha ("png " codec, 32-bit). Premiere Pro, After Effects, Avid, DaVinci
//! Resolve and Final Cut import it with transparency, which makes rendered
//! graphics usable in post-production.

/// Writes a .mov from PNG-encoded frames (all `width` × `height`), at
/// `rate` = num / den frames per second.
pub fn write_png_mov(frames: &[Vec<u8>], width: u32, height: u32, rate: (u32, u32)) -> Vec<u8> {
    let (num, den) = (rate.0.max(1), rate.1.max(1));
    // QuickTime prefers fine time scales (25 fps -> 2500).
    let k = if num < 1000 { 100 } else { 1 };
    let timescale = num * k;
    let delta = den * k;
    let duration = delta as u64 * frames.len() as u64;

    let mut out = Vec::new();
    // ftyp
    atom(&mut out, b"ftyp", |b| {
        b.extend_from_slice(b"qt  ");
        b.extend_from_slice(&0x0000_0200u32.to_be_bytes());
        b.extend_from_slice(b"qt  ");
    });
    // mdat (64-bit size when it does not fit in 32 bits)
    let payload: u64 = frames.iter().map(|f| f.len() as u64).sum();
    let mut offsets = Vec::with_capacity(frames.len());
    if payload + 8 > u32::MAX as u64 {
        out.extend_from_slice(&1u32.to_be_bytes());
        out.extend_from_slice(b"mdat");
        out.extend_from_slice(&(payload + 16).to_be_bytes());
    } else {
        out.extend_from_slice(&((payload + 8) as u32).to_be_bytes());
        out.extend_from_slice(b"mdat");
    }
    for f in frames {
        offsets.push(out.len() as u64);
        out.extend_from_slice(f);
    }
    let wide = offsets.last().is_some_and(|&o| o > u32::MAX as u64);

    atom(&mut out, b"moov", |moov| {
        atom(moov, b"mvhd", |b| {
            b.extend_from_slice(&[0; 4]); // version, flags
            b.extend_from_slice(&[0; 8]); // creation, modification
            b.extend_from_slice(&timescale.to_be_bytes());
            b.extend_from_slice(&(duration.min(u32::MAX as u64) as u32).to_be_bytes());
            b.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // rate 1.0
            b.extend_from_slice(&0x0100u16.to_be_bytes()); // volume 1.0
            b.extend_from_slice(&[0; 10]);
            matrix(b);
            b.extend_from_slice(&[0; 24]); // preview, poster, selection, current time
            b.extend_from_slice(&2u32.to_be_bytes()); // next track id
        });
        atom(moov, b"trak", |trak| {
            atom(trak, b"tkhd", |b| {
                b.extend_from_slice(&[0, 0, 0, 0x0f]); // enabled, in movie / preview / poster
                b.extend_from_slice(&[0; 8]);
                b.extend_from_slice(&1u32.to_be_bytes()); // track id
                b.extend_from_slice(&[0; 4]);
                b.extend_from_slice(&(duration.min(u32::MAX as u64) as u32).to_be_bytes());
                b.extend_from_slice(&[0; 8]);
                b.extend_from_slice(&[0; 4]); // layer, alternate group
                b.extend_from_slice(&[0; 4]); // volume, reserved
                matrix(b);
                b.extend_from_slice(&(width << 16).to_be_bytes());
                b.extend_from_slice(&(height << 16).to_be_bytes());
            });
            atom(trak, b"mdia", |mdia| {
                atom(mdia, b"mdhd", |b| {
                    b.extend_from_slice(&[0; 4]);
                    b.extend_from_slice(&[0; 8]);
                    b.extend_from_slice(&timescale.to_be_bytes());
                    b.extend_from_slice(&(duration.min(u32::MAX as u64) as u32).to_be_bytes());
                    b.extend_from_slice(&0x7fffu16.to_be_bytes()); // language: unspecified
                    b.extend_from_slice(&0u16.to_be_bytes()); // quality
                });
                handler(mdia, b"mhlr", b"vide", "Video");
                atom(mdia, b"minf", |minf| {
                    atom(minf, b"vmhd", |b| {
                        b.extend_from_slice(&[0, 0, 0, 1]);
                        b.extend_from_slice(&0x0040u16.to_be_bytes()); // graphics mode: dither copy
                        b.extend_from_slice(&[0x80, 0, 0x80, 0, 0x80, 0]); // opcolor
                    });
                    handler(minf, b"dhlr", b"alis", "Data");
                    atom(minf, b"dinf", |dinf| {
                        atom(dinf, b"dref", |b| {
                            b.extend_from_slice(&[0; 4]);
                            b.extend_from_slice(&1u32.to_be_bytes());
                            // Self-contained: the data is in this file.
                            atom(b, b"alis", |a| a.extend_from_slice(&[0, 0, 0, 1]));
                        });
                    });
                    atom(minf, b"stbl", |stbl| {
                        atom(stbl, b"stsd", |b| {
                            b.extend_from_slice(&[0; 4]);
                            b.extend_from_slice(&1u32.to_be_bytes());
                            atom(b, b"png ", |d| {
                                d.extend_from_slice(&[0; 6]);
                                d.extend_from_slice(&1u16.to_be_bytes()); // data reference index
                                d.extend_from_slice(&[0; 4]); // version, revision
                                d.extend_from_slice(b"appl");
                                d.extend_from_slice(&0u32.to_be_bytes()); // temporal quality
                                d.extend_from_slice(&512u32.to_be_bytes()); // spatial quality
                                d.extend_from_slice(&(width as u16).to_be_bytes());
                                d.extend_from_slice(&(height as u16).to_be_bytes());
                                d.extend_from_slice(&0x0048_0000u32.to_be_bytes()); // 72 dpi
                                d.extend_from_slice(&0x0048_0000u32.to_be_bytes());
                                d.extend_from_slice(&0u32.to_be_bytes()); // data size
                                d.extend_from_slice(&1u16.to_be_bytes()); // frames per sample
                                let mut name = [0u8; 32];
                                name[0] = 3;
                                name[1..4].copy_from_slice(b"PNG");
                                d.extend_from_slice(&name);
                                d.extend_from_slice(&32u16.to_be_bytes()); // depth: RGB + alpha
                                d.extend_from_slice(&0xffffu16.to_be_bytes()); // no color table
                            });
                        });
                        atom(stbl, b"stts", |b| {
                            b.extend_from_slice(&[0; 4]);
                            b.extend_from_slice(&1u32.to_be_bytes());
                            b.extend_from_slice(&(frames.len() as u32).to_be_bytes());
                            b.extend_from_slice(&delta.to_be_bytes());
                        });
                        atom(stbl, b"stsc", |b| {
                            b.extend_from_slice(&[0; 4]);
                            b.extend_from_slice(&1u32.to_be_bytes());
                            b.extend_from_slice(&1u32.to_be_bytes()); // first chunk
                            b.extend_from_slice(&1u32.to_be_bytes()); // samples per chunk
                            b.extend_from_slice(&1u32.to_be_bytes()); // description
                        });
                        atom(stbl, b"stsz", |b| {
                            b.extend_from_slice(&[0; 4]);
                            b.extend_from_slice(&0u32.to_be_bytes());
                            b.extend_from_slice(&(frames.len() as u32).to_be_bytes());
                            for f in frames {
                                b.extend_from_slice(&(f.len() as u32).to_be_bytes());
                            }
                        });
                        if wide {
                            atom(stbl, b"co64", |b| {
                                b.extend_from_slice(&[0; 4]);
                                b.extend_from_slice(&(offsets.len() as u32).to_be_bytes());
                                for o in &offsets {
                                    b.extend_from_slice(&o.to_be_bytes());
                                }
                            });
                        } else {
                            atom(stbl, b"stco", |b| {
                                b.extend_from_slice(&[0; 4]);
                                b.extend_from_slice(&(offsets.len() as u32).to_be_bytes());
                                for o in &offsets {
                                    b.extend_from_slice(&(*o as u32).to_be_bytes());
                                }
                            });
                        }
                    });
                });
            });
        });
    });
    out
}

fn atom(out: &mut Vec<u8>, kind: &[u8; 4], body: impl FnOnce(&mut Vec<u8>)) {
    let start = out.len();
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(kind);
    body(out);
    let size = (out.len() - start) as u32;
    out[start..start + 4].copy_from_slice(&size.to_be_bytes());
}

fn matrix(b: &mut Vec<u8>) {
    for v in [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
        b.extend_from_slice(&v.to_be_bytes());
    }
}

fn handler(out: &mut Vec<u8>, component: &[u8; 4], subtype: &[u8; 4], name: &str) {
    atom(out, b"hdlr", |b| {
        b.extend_from_slice(&[0; 4]);
        b.extend_from_slice(component);
        b.extend_from_slice(subtype);
        b.extend_from_slice(&[0; 12]); // manufacturer, flags, mask
        b.push(name.len() as u8);
        b.extend_from_slice(name.as_bytes());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_valid_container() {
        let frame = cg_render::Frame::blank(64, 36).to_png();
        let mov = write_png_mov(&[frame.clone(), frame.clone(), frame], 64, 36, (25, 1));
        assert_eq!(&mov[4..8], b"ftyp");
        // Every top-level atom size adds up to the file size.
        let mut i = 0;
        let mut kinds = Vec::new();
        while i < mov.len() {
            let size = u32::from_be_bytes(mov[i..i + 4].try_into().unwrap()) as usize;
            kinds.push(String::from_utf8_lossy(&mov[i + 4..i + 8]).into_owned());
            i += size;
        }
        assert_eq!(i, mov.len());
        assert_eq!(kinds, ["ftyp", "mdat", "moov"]);
        if let Ok(path) = std::env::var("CG_DUMP_MOV") {
            std::fs::write(path, &mov).unwrap();
        }
    }
}
