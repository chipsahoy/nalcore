//! Codec-neutral NAL unit framing and raw byte sequence payload extraction.

use core::fmt;
use std::borrow::Cow;

/// A borrowing iterator over NAL units in an Annex B byte stream.
///
/// Three-byte (`00 00 01`) and four-byte (`00 00 00 01`) start codes are
/// recognized. Bytes before the first start code are ignored. Zero bytes
/// immediately before a start code or at the end of the stream are treated as
/// leading or trailing zeros rather than as part of a NAL unit.
///
/// Consecutive start codes and a final start code produce empty NAL units. The
/// returned slices borrow directly from the input stream.
#[derive(Clone, Debug)]
pub struct AnnexBNalUnits<'a> {
    input: &'a [u8],
    payload_start: Option<usize>,
}

impl<'a> AnnexBNalUnits<'a> {
    /// Creates an iterator over the NAL units in `input`.
    #[must_use]
    pub fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            payload_start: find_start_code(input, 0).map(|(_, payload_start)| payload_start),
        }
    }
}

impl<'a> Iterator for AnnexBNalUnits<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        let payload_start = self.payload_start?;

        if let Some((start_code_start, next_payload_start)) =
            find_start_code(self.input, payload_start)
        {
            self.payload_start = Some(next_payload_start);
            let payload_end = trim_trailing_zeros(self.input, payload_start, start_code_start);
            Some(&self.input[payload_start..payload_end])
        } else {
            self.payload_start = None;
            let payload_end = trim_trailing_zeros(self.input, payload_start, self.input.len());
            Some(&self.input[payload_start..payload_end])
        }
    }
}

impl std::iter::FusedIterator for AnnexBNalUnits<'_> {}

/// Returns a borrowing iterator over NAL units in an Annex B byte stream.
#[must_use]
pub fn annex_b_nal_units(input: &[u8]) -> AnnexBNalUnits<'_> {
    AnnexBNalUnits::new(input)
}

fn find_start_code(input: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut offset = from;
    while input.len().saturating_sub(offset) >= 3 {
        if input[offset] == 0 && input[offset + 1] == 0 {
            if input.get(offset + 2) == Some(&1) {
                return Some((offset, offset + 3));
            }
            if input.get(offset + 2) == Some(&0) && input.get(offset + 3) == Some(&1) {
                return Some((offset, offset + 4));
            }
        }
        offset += 1;
    }
    None
}

fn trim_trailing_zeros(input: &[u8], start: usize, mut end: usize) -> usize {
    while end > start && input[end - 1] == 0 {
        end -= 1;
    }
    end
}

/// An error found while converting an encoded byte sequence payload to RBSP.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RbspError {
    /// An unescaped `00 00 00`, `00 00 01`, or `00 00 02` sequence occurred.
    ForbiddenSequence {
        /// The offset of the forbidden byte after the two zero bytes.
        offset: usize,
        /// The forbidden byte, in the range `00` through `02`.
        byte: u8,
    },
    /// An emulation-prevention byte was followed by a byte greater than `03`.
    InvalidEmulationPrevention {
        /// The offset of the emulation-prevention byte.
        offset: usize,
        /// The byte following the emulation-prevention byte.
        following: u8,
    },
}

impl fmt::Display for RbspError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ForbiddenSequence { offset, byte } => write!(
                formatter,
                "forbidden unescaped sequence ending in 0x{byte:02x} at byte offset {offset}"
            ),
            Self::InvalidEmulationPrevention { offset, following } => write!(
                formatter,
                "emulation-prevention byte at offset {offset} is followed by invalid byte 0x{following:02x}"
            ),
        }
    }
}

impl std::error::Error for RbspError {}

/// Converts an encoded byte sequence payload (EBSP) to an RBSP.
///
/// Emulation-prevention bytes in `00 00 03` sequences are removed. A terminal
/// `00 00 03` is valid. The input is borrowed when it contains no
/// emulation-prevention bytes; otherwise the returned payload owns a buffer no
/// larger than the input.
///
/// Unescaped `00 00 00`, `00 00 01`, and `00 00 02` sequences are rejected.
/// A `00 00 03` sequence followed by a byte greater than `03` is also rejected.
pub fn rbsp_from_ebsp(ebsp: &[u8]) -> Result<Cow<'_, [u8]>, RbspError> {
    let mut rbsp = None;
    let mut zero_count = 0_u8;
    let mut offset = 0_usize;

    while offset < ebsp.len() {
        let byte = ebsp[offset];

        if zero_count == 2 {
            match byte {
                0..=2 => {
                    return Err(RbspError::ForbiddenSequence { offset, byte });
                }
                3 => {
                    if let Some(&following) = ebsp.get(offset + 1) {
                        if following > 3 {
                            return Err(RbspError::InvalidEmulationPrevention {
                                offset,
                                following,
                            });
                        }
                    }

                    rbsp.get_or_insert_with(|| {
                        let mut output = Vec::with_capacity(ebsp.len().saturating_sub(1));
                        output.extend_from_slice(&ebsp[..offset]);
                        output
                    });
                    zero_count = 0;
                    offset += 1;
                    continue;
                }
                _ => {}
            }
        }

        if let Some(output) = &mut rbsp {
            output.push(byte);
        }
        zero_count = if byte == 0 {
            zero_count.saturating_add(1).min(2)
        } else {
            0
        };
        offset += 1;
    }

    Ok(match rbsp {
        Some(rbsp) => Cow::Owned(rbsp),
        None => Cow::Borrowed(ebsp),
    })
}

#[cfg(test)]
mod tests {
    use super::{AnnexBNalUnits, RbspError, annex_b_nal_units, rbsp_from_ebsp};
    use std::borrow::Cow;

    fn collect_units(input: &[u8]) -> Vec<Vec<u8>> {
        annex_b_nal_units(input).map(<[u8]>::to_vec).collect()
    }

    #[test]
    fn extracts_units_after_three_and_four_byte_start_codes() {
        let input = [0, 0, 1, 0x65, 0xaa, 0, 0, 0, 1, 0x41, 0xbb, 0xcc];

        assert_eq!(
            collect_units(&input),
            vec![vec![0x65, 0xaa], vec![0x41, 0xbb, 0xcc]]
        );
    }

    #[test]
    fn ignores_preamble_and_discards_leading_and_trailing_zeros() {
        let input = [
            0xff, 0x80, 0, 0, 0, 0, 1, 0x67, 0x12, 0, 0, 0, 0, 1, 0x68, 0, 0, 0,
        ];

        assert_eq!(collect_units(&input), vec![vec![0x67, 0x12], vec![0x68]]);
    }

    #[test]
    fn yields_empty_units_for_consecutive_and_terminal_start_codes() {
        let input = [0, 0, 1, 0, 0, 0, 1, 0x09, 0, 0, 1];

        assert_eq!(
            collect_units(&input),
            vec![Vec::<u8>::new(), vec![0x09], Vec::new()]
        );
    }

    #[test]
    fn empty_input_and_input_without_start_codes_have_no_units() {
        assert!(collect_units(&[]).is_empty());
        assert!(collect_units(&[0]).is_empty());
        assert!(collect_units(&[0, 0]).is_empty());
        assert!(collect_units(&[1, 2, 3, 4]).is_empty());
    }

    #[test]
    fn start_code_like_escaped_data_remains_in_the_unit() {
        let input = [0, 0, 1, 0x65, 0, 0, 3, 1, 0x80];

        assert_eq!(collect_units(&input), vec![vec![0x65, 0, 0, 3, 1, 0x80]]);
    }

    #[test]
    fn returned_units_borrow_the_input() {
        let input = [0, 0, 1, 0x65, 0xaa];
        let unit = annex_b_nal_units(&input).next().unwrap();

        assert_eq!(unit.as_ptr(), input[3..].as_ptr());
    }

    #[test]
    fn annex_b_iterator_is_fused() {
        let mut units = AnnexBNalUnits::new(&[0, 0, 1, 0x65]);

        assert_eq!(units.next(), Some([0x65].as_slice()));
        assert_eq!(units.next(), None);
        assert_eq!(units.next(), None);
    }

    #[test]
    fn rbsp_borrows_payloads_without_emulation_prevention_bytes() {
        let input = [0x67, 0x42, 0, 0, 4, 0xff];
        let rbsp = rbsp_from_ebsp(&input).unwrap();

        assert!(matches!(rbsp, Cow::Borrowed(_)));
        assert_eq!(rbsp.as_ref(), input);
    }

    #[test]
    fn removes_all_valid_emulation_prevention_sequences() {
        let cases: &[(&[u8], &[u8])] = &[
            (&[0, 0, 3, 0], &[0, 0, 0]),
            (&[0, 0, 3, 1], &[0, 0, 1]),
            (&[0, 0, 3, 2], &[0, 0, 2]),
            (&[0, 0, 3, 3], &[0, 0, 3]),
            (&[0, 0, 3], &[0, 0]),
            (
                &[0x11, 0, 0, 3, 0, 0x22, 0, 0, 3, 3, 0x33],
                &[0x11, 0, 0, 0, 0x22, 0, 0, 3, 0x33],
            ),
        ];

        for &(ebsp, expected) in cases {
            let rbsp = rbsp_from_ebsp(ebsp).unwrap();
            assert!(matches!(rbsp, Cow::Owned(_)), "ebsp={ebsp:?}");
            assert_eq!(rbsp.as_ref(), expected, "ebsp={ebsp:?}");
        }
    }

    #[test]
    fn rejects_each_forbidden_unescaped_sequence_at_any_offset() {
        for prefix_length in 0..=4 {
            for byte in 0..=2 {
                let mut ebsp = vec![0xaa; prefix_length];
                ebsp.extend_from_slice(&[0, 0, byte]);

                assert_eq!(
                    rbsp_from_ebsp(&ebsp),
                    Err(RbspError::ForbiddenSequence {
                        offset: prefix_length + 2,
                        byte,
                    })
                );
            }
        }
    }

    #[test]
    fn validates_every_byte_after_two_zeros() {
        for following in u8::MIN..=u8::MAX {
            let ebsp = [0, 0, following];
            let result = rbsp_from_ebsp(&ebsp);

            match following {
                0..=2 => assert_eq!(
                    result,
                    Err(RbspError::ForbiddenSequence {
                        offset: 2,
                        byte: following,
                    })
                ),
                3 => assert_eq!(result.unwrap().as_ref(), [0, 0]),
                _ => assert_eq!(result.unwrap().as_ref(), ebsp),
            }
        }
    }

    #[test]
    fn rejects_invalid_bytes_after_emulation_prevention() {
        for following in 4..=u8::MAX {
            let ebsp = [0xaa, 0, 0, 3, following];

            assert_eq!(
                rbsp_from_ebsp(&ebsp),
                Err(RbspError::InvalidEmulationPrevention {
                    offset: 3,
                    following,
                })
            );
        }
    }

    #[test]
    fn errors_have_stable_descriptions() {
        assert_eq!(
            RbspError::ForbiddenSequence { offset: 7, byte: 1 }.to_string(),
            "forbidden unescaped sequence ending in 0x01 at byte offset 7"
        );
        assert_eq!(
            RbspError::InvalidEmulationPrevention {
                offset: 4,
                following: 0xff
            }
            .to_string(),
            "emulation-prevention byte at offset 4 is followed by invalid byte 0xff"
        );
    }

    #[test]
    fn broad_short_input_set_is_panic_free() {
        const ALPHABET: [u8; 7] = [0, 1, 2, 3, 4, 0x80, 0xff];

        for length in 0..=6 {
            let combinations = ALPHABET.len().pow(length as u32);
            for mut value in 0..combinations {
                let mut input = vec![0; length];
                for byte in &mut input {
                    *byte = ALPHABET[value % ALPHABET.len()];
                    value /= ALPHABET.len();
                }

                let units = annex_b_nal_units(&input).collect::<Vec<_>>();
                for unit in units {
                    let input_start = input.as_ptr() as usize;
                    let input_end = input_start + input.len();
                    let unit_start = unit.as_ptr() as usize;
                    assert!(unit_start >= input_start && unit_start <= input_end);
                    assert!(unit.len() <= input_end.saturating_sub(unit_start));
                }
                let _ = rbsp_from_ebsp(&input);
            }
        }
    }
}
