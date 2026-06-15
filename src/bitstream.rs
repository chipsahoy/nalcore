//! Bitstream parsing primitives shared by the supported codecs.
//!
//! Bits are read most-significant bit first, matching the bit ordering used by
//! H.264 and H.265 syntax elements.

use core::fmt;

/// An error returned while reading a bitstream syntax element.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BitReaderError {
    /// The input ended before the requested number of bits were available.
    UnexpectedEof {
        /// The number of bits required by the operation.
        requested: usize,
        /// The number of bits remaining, saturated at [`usize::MAX`].
        remaining: usize,
    },
    /// A fixed-width read requested an unsupported width.
    InvalidWidth {
        /// The requested width in bits.
        width: usize,
        /// The largest supported width in bits.
        maximum: usize,
    },
    /// A variable-length value cannot be represented by its return type.
    Overflow,
}

impl fmt::Display for BitReaderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof {
                requested,
                remaining,
            } => write!(
                formatter,
                "unexpected end of input: requested {requested} bits, {remaining} remain"
            ),
            Self::InvalidWidth { width, maximum } => {
                write!(formatter, "invalid bit width {width}; maximum is {maximum}")
            }
            Self::Overflow => formatter.write_str("decoded value overflows its return type"),
        }
    }
}

impl std::error::Error for BitReaderError {}

/// A non-allocating, most-significant-bit-first reader over a byte slice.
///
/// Failed reads do not advance the reader. An unsigned fixed-width read may
/// have a width from 0 through 64 bits; a zero-width read returns zero. Signed
/// reads require a width from 1 through 64 bits and use two's-complement
/// representation.
#[derive(Clone, Debug)]
pub struct BitReader<'a> {
    input: &'a [u8],
    byte_offset: usize,
    bit_offset: u8,
}

impl<'a> BitReader<'a> {
    /// Creates a reader positioned at the first bit of `input`.
    #[must_use]
    pub const fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            byte_offset: 0,
            bit_offset: 0,
        }
    }

    /// Returns the number of unread bits.
    ///
    /// The result saturates at [`usize::MAX`] if the byte slice is
    /// theoretically large enough for its bit count not to fit in `usize`.
    #[must_use]
    pub fn remaining_bits(&self) -> usize {
        usize::try_from(self.remaining_bits_u128()).unwrap_or(usize::MAX)
    }

    /// Reads one bit.
    pub fn read_bit(&mut self) -> Result<bool, BitReaderError> {
        let byte = self
            .input
            .get(self.byte_offset)
            .copied()
            .ok_or_else(|| self.unexpected_eof(1))?;
        let value = byte & (0x80 >> self.bit_offset) != 0;
        self.advance_unchecked(1);
        Ok(value)
    }

    /// Reads an unsigned field with `width` bits.
    ///
    /// Widths greater than 64 return [`BitReaderError::InvalidWidth`].
    pub fn read_bits(&mut self, width: usize) -> Result<u64, BitReaderError> {
        if width > u64::BITS as usize {
            return Err(BitReaderError::InvalidWidth {
                width,
                maximum: u64::BITS as usize,
            });
        }
        self.require_bits(width)?;

        let mut value = 0_u64;
        for _ in 0..width {
            value = (value << 1) | u64::from(self.read_bit_unchecked());
        }
        Ok(value)
    }

    /// Reads a signed two's-complement field with `width` bits.
    ///
    /// Valid widths are 1 through 64 bits.
    pub fn read_signed_bits(&mut self, width: usize) -> Result<i64, BitReaderError> {
        if !(1..=i64::BITS as usize).contains(&width) {
            return Err(BitReaderError::InvalidWidth {
                width,
                maximum: i64::BITS as usize,
            });
        }

        let value = self.read_bits(width)?;
        if width == i64::BITS as usize {
            return Ok(value as i64);
        }

        let sign_bit = 1_u64 << (width - 1);
        if value & sign_bit == 0 {
            Ok(value as i64)
        } else {
            Ok((value | (!0_u64 << width)) as i64)
        }
    }

    /// Advances by `count` bits without reading their values.
    ///
    /// If insufficient input remains, the reader is not advanced.
    pub fn skip_bits(&mut self, count: usize) -> Result<(), BitReaderError> {
        self.require_bits(count)?;
        self.advance_unchecked(count);
        Ok(())
    }

    /// Decodes an unsigned Exp-Golomb (`ue(v)`) value.
    ///
    /// Prefixes or values that cannot be represented by `u64` return
    /// [`BitReaderError::Overflow`]. A truncated code returns
    /// [`BitReaderError::UnexpectedEof`].
    pub fn read_ue(&mut self) -> Result<u64, BitReaderError> {
        let start = (self.byte_offset, self.bit_offset);
        let result = self.read_ue_inner();
        if result.is_err() {
            (self.byte_offset, self.bit_offset) = start;
        }
        result
    }

    /// Decodes a signed Exp-Golomb (`se(v)`) value.
    ///
    /// Values outside the `i64` range return [`BitReaderError::Overflow`].
    /// On any error, the reader is not advanced.
    pub fn read_se(&mut self) -> Result<i64, BitReaderError> {
        let start = (self.byte_offset, self.bit_offset);
        let result = self.read_ue().and_then(|code_num| {
            if code_num & 1 == 0 {
                let magnitude = code_num / 2;
                i64::try_from(magnitude)
                    .map(|value| -value)
                    .map_err(|_| BitReaderError::Overflow)
            } else {
                let magnitude = code_num / 2 + 1;
                i64::try_from(magnitude).map_err(|_| BitReaderError::Overflow)
            }
        });
        if result.is_err() {
            (self.byte_offset, self.bit_offset) = start;
        }
        result
    }

    fn read_ue_inner(&mut self) -> Result<u64, BitReaderError> {
        let mut leading_zero_bits = 0_usize;
        while !self.read_bit()? {
            leading_zero_bits += 1;
            if leading_zero_bits > u64::BITS as usize {
                return Err(BitReaderError::Overflow);
            }
        }

        if leading_zero_bits == 0 {
            return Ok(0);
        }

        let suffix = self.read_bits(leading_zero_bits)?;
        if leading_zero_bits == u64::BITS as usize {
            return if suffix == 0 {
                Ok(u64::MAX)
            } else {
                Err(BitReaderError::Overflow)
            };
        }

        Ok((1_u64 << leading_zero_bits) - 1 + suffix)
    }

    fn require_bits(&self, requested: usize) -> Result<(), BitReaderError> {
        if requested as u128 <= self.remaining_bits_u128() {
            Ok(())
        } else {
            Err(self.unexpected_eof(requested))
        }
    }

    fn unexpected_eof(&self, requested: usize) -> BitReaderError {
        BitReaderError::UnexpectedEof {
            requested,
            remaining: self.remaining_bits(),
        }
    }

    fn remaining_bits_u128(&self) -> u128 {
        let remaining_bytes = self.input.len().saturating_sub(self.byte_offset) as u128;
        remaining_bytes * u128::from(u8::BITS) - u128::from(self.bit_offset)
    }

    fn read_bit_unchecked(&mut self) -> bool {
        let value = self.input[self.byte_offset] & (0x80 >> self.bit_offset) != 0;
        self.advance_unchecked(1);
        value
    }

    fn advance_unchecked(&mut self, count: usize) {
        let byte_advance = count / u8::BITS as usize;
        let bit_advance = (count % u8::BITS as usize) as u8;

        self.byte_offset = self
            .byte_offset
            .checked_add(byte_advance)
            .unwrap_or(self.input.len());
        self.bit_offset += bit_advance;
        if self.bit_offset >= u8::BITS as u8 {
            self.byte_offset = self.byte_offset.checked_add(1).unwrap_or(self.input.len());
            self.bit_offset -= u8::BITS as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BitReader, BitReaderError};

    fn bits_to_bytes(bits: &str) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(bits.len().div_ceil(8));
        for chunk in bits.as_bytes().chunks(8) {
            let mut byte = 0_u8;
            for (index, bit) in chunk.iter().enumerate() {
                assert!(matches!(bit, b'0' | b'1'));
                if *bit == b'1' {
                    byte |= 0x80 >> index;
                }
            }
            bytes.push(byte);
        }
        bytes
    }

    fn reference_bits(input: &[u8], start: usize, width: usize) -> u64 {
        (0..width).fold(0_u64, |value, offset| {
            let position = start + offset;
            let bit = (input[position / 8] >> (7 - position % 8)) & 1;
            (value << 1) | u64::from(bit)
        })
    }

    fn assert_failed_read_preserves_position<'a, T>(
        input: &'a [u8],
        start: usize,
        operation: impl FnOnce(&mut BitReader<'a>) -> Result<T, BitReaderError>,
    ) {
        let mut reader = BitReader::new(input);
        reader.skip_bits(start).unwrap();
        let before = reader.remaining_bits();
        if operation(&mut reader).is_err() {
            assert_eq!(reader.remaining_bits(), before);
        }
    }

    #[test]
    fn reads_bits_most_significant_bit_first() {
        let mut reader = BitReader::new(&[0b1011_0010, 0b0110_1101]);

        assert_eq!(reader.read_bit(), Ok(true));
        assert_eq!(reader.read_bits(3), Ok(0b011));
        assert_eq!(reader.read_bits(6), Ok(0b001001));
        assert_eq!(reader.read_bits(6), Ok(0b101101));
        assert_eq!(reader.remaining_bits(), 0);
    }

    #[test]
    fn reads_all_widths_at_all_bit_alignments() {
        let input = [0x00, 0xff, 0xa5, 0x3c, 0x81, 0x7e, 0x42, 0x99, 0x13, 0x57];

        for start in 0..=16 {
            for width in 0..=64 {
                let mut reader = BitReader::new(&input);
                reader.skip_bits(start).unwrap();
                assert_eq!(
                    reader.read_bits(width),
                    Ok(reference_bits(&input, start, width)),
                    "start={start}, width={width}"
                );
                assert_eq!(reader.remaining_bits(), input.len() * 8 - start - width);
            }
        }
    }

    #[test]
    fn reads_signed_twos_complement_fields() {
        let cases = [
            ("0", 1, 0),
            ("1", 1, -1),
            ("0111", 4, 7),
            ("1000", 4, -8),
            ("1111", 4, -1),
            ("10000000", 8, -128),
            ("01111111", 8, 127),
        ];

        for (bits, width, expected) in cases {
            let bytes = bits_to_bytes(bits);
            assert_eq!(
                BitReader::new(&bytes).read_signed_bits(width),
                Ok(expected),
                "bits={bits}"
            );
        }

        assert_eq!(
            BitReader::new(&i64::MIN.to_be_bytes()).read_signed_bits(64),
            Ok(i64::MIN)
        );
        assert_eq!(
            BitReader::new(&i64::MAX.to_be_bytes()).read_signed_bits(64),
            Ok(i64::MAX)
        );
    }

    #[test]
    fn rejects_invalid_widths_without_advancing() {
        let mut reader = BitReader::new(&[0xff]);

        assert_eq!(
            reader.read_bits(65),
            Err(BitReaderError::InvalidWidth {
                width: 65,
                maximum: 64,
            })
        );
        assert_eq!(
            reader.read_signed_bits(0),
            Err(BitReaderError::InvalidWidth {
                width: 0,
                maximum: 64,
            })
        );
        assert_eq!(
            reader.read_signed_bits(65),
            Err(BitReaderError::InvalidWidth {
                width: 65,
                maximum: 64,
            })
        );
        assert_eq!(reader.remaining_bits(), 8);
        assert_eq!(reader.read_bits(0), Ok(0));
        assert_eq!(reader.remaining_bits(), 8);
    }

    #[test]
    fn reports_eof_and_keeps_position_for_fixed_width_operations() {
        let mut reader = BitReader::new(&[0b1010_0000]);
        reader.skip_bits(5).unwrap();

        assert_eq!(
            reader.read_bits(4),
            Err(BitReaderError::UnexpectedEof {
                requested: 4,
                remaining: 3,
            })
        );
        assert_eq!(reader.remaining_bits(), 3);
        assert_eq!(
            reader.skip_bits(4),
            Err(BitReaderError::UnexpectedEof {
                requested: 4,
                remaining: 3,
            })
        );
        assert_eq!(reader.read_bits(3), Ok(0));
        assert_eq!(
            reader.read_bit(),
            Err(BitReaderError::UnexpectedEof {
                requested: 1,
                remaining: 0,
            })
        );
    }

    #[test]
    fn skips_across_byte_boundaries() {
        let mut reader = BitReader::new(&[0xaa, 0x55, 0xf0]);

        reader.skip_bits(0).unwrap();
        reader.skip_bits(7).unwrap();
        assert_eq!(reader.read_bits(2), Ok(0));
        reader.skip_bits(8).unwrap();
        assert_eq!(reader.remaining_bits(), 7);
        assert_eq!(reader.read_bits(7), Ok(0x70));
    }

    #[test]
    fn decodes_unsigned_exp_golomb_values() {
        let codes = [
            ("1", 0),
            ("010", 1),
            ("011", 2),
            ("00100", 3),
            ("00101", 4),
            ("00110", 5),
            ("00111", 6),
            ("0001000", 7),
            ("0001011", 10),
            ("000010001", 16),
        ];

        for (bits, expected) in codes {
            let bytes = bits_to_bytes(bits);
            let mut reader = BitReader::new(&bytes);
            assert_eq!(reader.read_ue(), Ok(expected), "bits={bits}");
            assert_eq!(reader.remaining_bits(), bytes.len() * 8 - bits.len());
        }
    }

    #[test]
    fn decodes_signed_exp_golomb_values() {
        let codes = [
            ("1", 0),
            ("010", 1),
            ("011", -1),
            ("00100", 2),
            ("00101", -2),
            ("00110", 3),
            ("00111", -3),
        ];

        for (bits, expected) in codes {
            let bytes = bits_to_bytes(bits);
            assert_eq!(
                BitReader::new(&bytes).read_se(),
                Ok(expected),
                "bits={bits}"
            );
        }
    }

    #[test]
    fn decodes_exp_golomb_values_at_every_bit_alignment() {
        for alignment in 0..8 {
            let prefix = "1".repeat(alignment);
            let encoded = format!("{prefix}0001011");
            let bytes = bits_to_bytes(&encoded);
            let mut reader = BitReader::new(&bytes);
            reader.skip_bits(alignment).unwrap();

            assert_eq!(reader.read_ue(), Ok(10), "alignment={alignment}");
        }
    }

    #[test]
    fn handles_exp_golomb_boundaries_and_overflow() {
        let maximum = format!("{}1{}", "0".repeat(64), "0".repeat(64));
        let bytes = bits_to_bytes(&maximum);
        assert_eq!(BitReader::new(&bytes).read_ue(), Ok(u64::MAX));

        let unsigned_overflow = format!("{}1{}1", "0".repeat(64), "0".repeat(63));
        let bytes = bits_to_bytes(&unsigned_overflow);
        let mut reader = BitReader::new(&bytes);
        assert_eq!(reader.read_ue(), Err(BitReaderError::Overflow));
        assert_eq!(reader.remaining_bits(), bytes.len() * 8);

        let prefix_overflow = format!("{}1", "0".repeat(65));
        let bytes = bits_to_bytes(&prefix_overflow);
        assert_eq!(
            BitReader::new(&bytes).read_ue(),
            Err(BitReaderError::Overflow)
        );

        let signed_overflow = maximum;
        let bytes = bits_to_bytes(&signed_overflow);
        let mut reader = BitReader::new(&bytes);
        assert_eq!(reader.read_se(), Err(BitReaderError::Overflow));
        assert_eq!(reader.remaining_bits(), bytes.len() * 8);
    }

    #[test]
    fn reports_truncated_exp_golomb_codes_without_advancing() {
        for bits in ["", "0", "00", "00000000", "001", "00010"] {
            let padding_bits = (8 - bits.len() % 8) % 8;
            let encoded = format!("{}{bits}", "0".repeat(padding_bits));
            let bytes = bits_to_bytes(&encoded);
            let mut reader = BitReader::new(&bytes);
            reader.skip_bits(padding_bits).unwrap();
            let remaining = reader.remaining_bits();

            assert!(
                matches!(reader.read_ue(), Err(BitReaderError::UnexpectedEof { .. })),
                "bits={bits}"
            );
            assert_eq!(reader.remaining_bits(), remaining, "bits={bits}");
        }
    }

    #[test]
    fn all_short_inputs_and_operation_widths_are_panic_free() {
        for value in 0_u16..=u16::MAX {
            let input = value.to_be_bytes();
            for start in 0..=input.len() * 8 {
                for width in [0, 1, 7, 8, 9, 15, 16, 17, 63, 64, 65, usize::MAX] {
                    let mut reader = BitReader::new(&input);
                    reader.skip_bits(start).unwrap();
                    let before = reader.remaining_bits();
                    let result = reader.read_bits(width);
                    if result.is_err() {
                        assert_eq!(reader.remaining_bits(), before);
                    }
                }

                assert_failed_read_preserves_position(&input, start, BitReader::read_bit);
                assert_failed_read_preserves_position(&input, start, BitReader::read_ue);
                assert_failed_read_preserves_position(&input, start, BitReader::read_se);

                for count in [0, 1, 8, 16, 17, usize::MAX] {
                    let mut reader = BitReader::new(&input);
                    reader.skip_bits(start).unwrap();
                    let before = reader.remaining_bits();
                    if reader.skip_bits(count).is_err() {
                        assert_eq!(reader.remaining_bits(), before);
                    }
                }
            }
        }
    }
}
