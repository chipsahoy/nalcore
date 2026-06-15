//! H.264/AVC syntax parsing and decoding.

use core::fmt;

pub mod sps;

pub use sps::*;

/// The five-bit `nal_unit_type` field from an H.264 NAL unit header.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NalUnitType {
    /// A type value whose meaning is unspecified by H.264.
    Unspecified(u8),
    /// A coded slice of a non-IDR picture.
    CodedSliceNonIdr,
    /// A coded slice data partition A.
    CodedSliceDataPartitionA,
    /// A coded slice data partition B.
    CodedSliceDataPartitionB,
    /// A coded slice data partition C.
    CodedSliceDataPartitionC,
    /// A coded slice of an IDR picture.
    CodedSliceIdr,
    /// Supplemental enhancement information.
    SupplementalEnhancementInformation,
    /// A sequence parameter set.
    SequenceParameterSet,
    /// A picture parameter set.
    PictureParameterSet,
    /// An access unit delimiter.
    AccessUnitDelimiter,
    /// The end of a sequence.
    EndOfSequence,
    /// The end of the stream.
    EndOfStream,
    /// Filler data.
    FillerData,
    /// A sequence parameter set extension.
    SequenceParameterSetExtension,
    /// A prefix NAL unit.
    Prefix,
    /// A subset sequence parameter set.
    SubsetSequenceParameterSet,
    /// A depth parameter set.
    DepthParameterSet,
    /// A type value reserved by H.264.
    Reserved(u8),
    /// A coded slice of an auxiliary coded picture.
    CodedSliceAuxiliary,
    /// A coded slice extension.
    CodedSliceExtension,
    /// A coded slice extension for a depth view component.
    CodedSliceExtensionDepthView,
}

impl NalUnitType {
    /// Converts a raw five-bit value to its H.264 NAL unit type.
    ///
    /// Values greater than 31 do not fit in the header field and return `None`.
    #[must_use]
    pub const fn from_raw(value: u8) -> Option<Self> {
        Some(match value {
            0 | 24..=31 => Self::Unspecified(value),
            1 => Self::CodedSliceNonIdr,
            2 => Self::CodedSliceDataPartitionA,
            3 => Self::CodedSliceDataPartitionB,
            4 => Self::CodedSliceDataPartitionC,
            5 => Self::CodedSliceIdr,
            6 => Self::SupplementalEnhancementInformation,
            7 => Self::SequenceParameterSet,
            8 => Self::PictureParameterSet,
            9 => Self::AccessUnitDelimiter,
            10 => Self::EndOfSequence,
            11 => Self::EndOfStream,
            12 => Self::FillerData,
            13 => Self::SequenceParameterSetExtension,
            14 => Self::Prefix,
            15 => Self::SubsetSequenceParameterSet,
            16 => Self::DepthParameterSet,
            17..=18 | 22..=23 => Self::Reserved(value),
            19 => Self::CodedSliceAuxiliary,
            20 => Self::CodedSliceExtension,
            21 => Self::CodedSliceExtensionDepthView,
            _ => return None,
        })
    }

    /// Returns the raw five-bit value stored in the NAL unit header.
    #[must_use]
    pub const fn raw_value(self) -> u8 {
        match self {
            Self::Unspecified(value) | Self::Reserved(value) => value,
            Self::CodedSliceNonIdr => 1,
            Self::CodedSliceDataPartitionA => 2,
            Self::CodedSliceDataPartitionB => 3,
            Self::CodedSliceDataPartitionC => 4,
            Self::CodedSliceIdr => 5,
            Self::SupplementalEnhancementInformation => 6,
            Self::SequenceParameterSet => 7,
            Self::PictureParameterSet => 8,
            Self::AccessUnitDelimiter => 9,
            Self::EndOfSequence => 10,
            Self::EndOfStream => 11,
            Self::FillerData => 12,
            Self::SequenceParameterSetExtension => 13,
            Self::Prefix => 14,
            Self::SubsetSequenceParameterSet => 15,
            Self::DepthParameterSet => 16,
            Self::CodedSliceAuxiliary => 19,
            Self::CodedSliceExtension => 20,
            Self::CodedSliceExtensionDepthView => 21,
        }
    }
}

/// A validated one-byte H.264 NAL unit header.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NalHeader {
    nal_ref_idc: u8,
    nal_unit_type: NalUnitType,
}

impl NalHeader {
    /// Returns the two-bit reference priority field.
    #[must_use]
    pub const fn nal_ref_idc(self) -> u8 {
        self.nal_ref_idc
    }

    /// Returns the NAL unit type.
    #[must_use]
    pub const fn nal_unit_type(self) -> NalUnitType {
        self.nal_unit_type
    }
}

/// A parsed H.264 NAL unit that borrows its encoded payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NalUnit<'a> {
    header: NalHeader,
    payload: &'a [u8],
}

impl<'a> NalUnit<'a> {
    /// Returns the validated NAL unit header.
    #[must_use]
    pub const fn header(self) -> NalHeader {
        self.header
    }

    /// Returns the encoded byte sequence payload after the one-byte header.
    ///
    /// Extension headers, when present, remain in this payload.
    #[must_use]
    pub const fn payload(self) -> &'a [u8] {
        self.payload
    }
}

/// An error returned while parsing an H.264 NAL unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NalUnitError {
    /// The framed NAL unit did not contain its one-byte header.
    MissingHeader,
    /// The header's required-zero `forbidden_zero_bit` was asserted.
    ForbiddenZeroBit {
        /// The invalid header byte.
        header: u8,
    },
}

impl fmt::Display for NalUnitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHeader => formatter.write_str("H.264 NAL unit is missing its header"),
            Self::ForbiddenZeroBit { header } => write!(
                formatter,
                "H.264 NAL header 0x{header:02x} has forbidden_zero_bit set"
            ),
        }
    }
}

impl std::error::Error for NalUnitError {}

/// Parses an H.264 NAL unit from the bytes returned by Annex B framing.
///
/// The one-byte header is validated and separated from the borrowed EBSP
/// payload. Extension NAL headers are not parsed.
pub fn parse_nal_unit(input: &[u8]) -> Result<NalUnit<'_>, NalUnitError> {
    let (&header, payload) = input.split_first().ok_or(NalUnitError::MissingHeader)?;
    if header & 0x80 != 0 {
        return Err(NalUnitError::ForbiddenZeroBit { header });
    }

    let type_value = header & 0x1f;
    let nal_unit_type = match NalUnitType::from_raw(type_value) {
        Some(nal_unit_type) => nal_unit_type,
        None => NalUnitType::Unspecified(type_value),
    };

    Ok(NalUnit {
        header: NalHeader {
            nal_ref_idc: (header >> 5) & 0x03,
            nal_unit_type,
        },
        payload,
    })
}

#[cfg(test)]
mod tests {
    use super::{NalUnitError, NalUnitType, parse_nal_unit};
    use crate::nal::{annex_b_nal_units, rbsp_from_ebsp};
    use std::borrow::Cow;

    #[test]
    fn parses_common_vcl_and_non_vcl_types() {
        let cases = [
            (0x01, NalUnitType::CodedSliceNonIdr),
            (0x05, NalUnitType::CodedSliceIdr),
            (0x06, NalUnitType::SupplementalEnhancementInformation),
            (0x07, NalUnitType::SequenceParameterSet),
            (0x08, NalUnitType::PictureParameterSet),
            (0x09, NalUnitType::AccessUnitDelimiter),
        ];

        for (type_value, expected) in cases {
            for nal_ref_idc in 0..=3 {
                let input = [(nal_ref_idc << 5) | type_value, 0xaa, 0xbb];
                let unit = parse_nal_unit(&input).unwrap();

                assert_eq!(unit.header().nal_ref_idc(), nal_ref_idc);
                assert_eq!(unit.header().nal_unit_type(), expected);
                assert_eq!(unit.payload(), [0xaa, 0xbb]);
            }
        }
    }

    #[test]
    fn preserves_every_reserved_and_unspecified_type_value() {
        for value in [0, 24, 25, 26, 27, 28, 29, 30, 31] {
            let input = [value];
            let unit = parse_nal_unit(&input).unwrap();
            assert_eq!(
                unit.header().nal_unit_type(),
                NalUnitType::Unspecified(value)
            );
        }

        for value in [17, 18, 22, 23] {
            let input = [value];
            let unit = parse_nal_unit(&input).unwrap();
            assert_eq!(unit.header().nal_unit_type(), NalUnitType::Reserved(value));
        }
    }

    #[test]
    fn maps_all_defined_type_values() {
        let expected = [
            NalUnitType::Unspecified(0),
            NalUnitType::CodedSliceNonIdr,
            NalUnitType::CodedSliceDataPartitionA,
            NalUnitType::CodedSliceDataPartitionB,
            NalUnitType::CodedSliceDataPartitionC,
            NalUnitType::CodedSliceIdr,
            NalUnitType::SupplementalEnhancementInformation,
            NalUnitType::SequenceParameterSet,
            NalUnitType::PictureParameterSet,
            NalUnitType::AccessUnitDelimiter,
            NalUnitType::EndOfSequence,
            NalUnitType::EndOfStream,
            NalUnitType::FillerData,
            NalUnitType::SequenceParameterSetExtension,
            NalUnitType::Prefix,
            NalUnitType::SubsetSequenceParameterSet,
            NalUnitType::DepthParameterSet,
            NalUnitType::Reserved(17),
            NalUnitType::Reserved(18),
            NalUnitType::CodedSliceAuxiliary,
            NalUnitType::CodedSliceExtension,
            NalUnitType::CodedSliceExtensionDepthView,
            NalUnitType::Reserved(22),
            NalUnitType::Reserved(23),
            NalUnitType::Unspecified(24),
            NalUnitType::Unspecified(25),
            NalUnitType::Unspecified(26),
            NalUnitType::Unspecified(27),
            NalUnitType::Unspecified(28),
            NalUnitType::Unspecified(29),
            NalUnitType::Unspecified(30),
            NalUnitType::Unspecified(31),
        ];

        for (value, expected) in expected.into_iter().enumerate() {
            let value = value as u8;
            assert_eq!(NalUnitType::from_raw(value), Some(expected));
            assert_eq!(expected.raw_value(), value);
        }
        for value in 32..=u8::MAX {
            assert_eq!(NalUnitType::from_raw(value), None);
        }
    }

    #[test]
    fn rejects_missing_and_forbidden_headers() {
        assert_eq!(parse_nal_unit(&[]), Err(NalUnitError::MissingHeader));

        for header in 0x80..=u8::MAX {
            assert_eq!(
                parse_nal_unit(&[header, 0xaa]),
                Err(NalUnitError::ForbiddenZeroBit { header })
            );
        }
    }

    #[test]
    fn all_header_bytes_are_parsed_without_panicking() {
        for header in u8::MIN..=u8::MAX {
            let input = [header, 0xaa, 0xbb, 0xcc];
            let result = parse_nal_unit(&input);

            if header & 0x80 != 0 {
                assert_eq!(result, Err(NalUnitError::ForbiddenZeroBit { header }));
                continue;
            }

            let unit = result.unwrap();
            assert_eq!(unit.header().nal_ref_idc(), (header >> 5) & 0x03);
            assert_eq!(unit.header().nal_unit_type().raw_value(), header & 0x1f);
            assert_eq!(unit.payload(), &input[1..]);
            assert_eq!(unit.payload().as_ptr(), input[1..].as_ptr());
        }
    }

    #[test]
    fn accepts_an_empty_payload() {
        let unit = parse_nal_unit(&[0x65]).unwrap();

        assert_eq!(unit.header().nal_ref_idc(), 3);
        assert_eq!(unit.header().nal_unit_type(), NalUnitType::CodedSliceIdr);
        assert!(unit.payload().is_empty());
    }

    #[test]
    fn composes_with_annex_b_framing_and_rbsp_extraction() {
        let stream = [0, 0, 1, 0x67, 0x42, 0, 0, 3, 1, 0x80];
        let framed = annex_b_nal_units(&stream).next().unwrap();
        let unit = parse_nal_unit(framed).unwrap();
        let rbsp = rbsp_from_ebsp(unit.payload()).unwrap();

        assert_eq!(
            unit.header().nal_unit_type(),
            NalUnitType::SequenceParameterSet
        );
        assert!(matches!(rbsp, Cow::Owned(_)));
        assert_eq!(rbsp.as_ref(), [0x42, 0, 0, 1, 0x80]);
    }

    #[test]
    fn extension_headers_remain_in_the_payload() {
        for type_value in [14, 20, 21] {
            let input = [type_value, 0xde, 0xad, 0xbe, 0xef];
            let unit = parse_nal_unit(&input).unwrap();

            assert_eq!(unit.header().nal_unit_type().raw_value(), type_value);
            assert_eq!(unit.payload(), [0xde, 0xad, 0xbe, 0xef]);
        }
    }

    #[test]
    fn errors_have_stable_descriptions() {
        assert_eq!(
            NalUnitError::MissingHeader.to_string(),
            "H.264 NAL unit is missing its header"
        );
        assert_eq!(
            NalUnitError::ForbiddenZeroBit { header: 0xe5 }.to_string(),
            "H.264 NAL header 0xe5 has forbidden_zero_bit set"
        );
    }
}
