//! H.264 sequence parameter set syntax.

use core::fmt;

use crate::bitstream::{BitReader, BitReaderError};

const MAX_SEQUENCE_PARAMETER_SET_ID: u64 = 31;
const MAX_BIT_DEPTH_MINUS_8: u64 = 6;
const MAX_LOG2_MINUS_4: u64 = 12;
const MAX_POC_CYCLE_LENGTH: u64 = 255;
const MAX_CPB_COUNT_MINUS_1: u64 = 31;
const MAX_DPB_FRAMES: u64 = 16;

/// The chroma sampling format signaled by an H.264 sequence parameter set.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ChromaFormat {
    /// Monochrome samples only.
    Monochrome,
    /// 4:2:0 chroma sampling.
    Yuv420,
    /// 4:2:2 chroma sampling.
    Yuv422,
    /// 4:4:4 chroma sampling.
    Yuv444,
}

impl ChromaFormat {
    fn from_idc(value: u64) -> Result<Self, SequenceParameterSetError> {
        match value {
            0 => Ok(Self::Monochrome),
            1 => Ok(Self::Yuv420),
            2 => Ok(Self::Yuv422),
            3 => Ok(Self::Yuv444),
            _ => Err(out_of_range("chroma_format_idc", value, 0, 3)),
        }
    }

    /// Returns the raw `chroma_format_idc` value.
    #[must_use]
    pub const fn idc(self) -> u8 {
        match self {
            Self::Monochrome => 0,
            Self::Yuv420 => 1,
            Self::Yuv422 => 2,
            Self::Yuv444 => 3,
        }
    }
}

/// A parsed 4x4 sequence scaling list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScalingList4x4 {
    /// The decoded scaling-list values.
    pub values: [u8; 16],
    /// Whether the syntax selects the standard default matrix.
    pub use_default: bool,
}

/// A parsed 8x8 sequence scaling list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScalingList8x8 {
    /// The decoded scaling-list values.
    pub values: [u8; 64],
    /// Whether the syntax selects the standard default matrix.
    pub use_default: bool,
}

/// Scaling matrices explicitly present in a sequence parameter set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SequenceScalingMatrices {
    /// The six possible 4x4 scaling lists.
    pub lists_4x4: [Option<ScalingList4x4>; 6],
    /// The six possible 8x8 scaling lists. Non-4:4:4 streams use only the
    /// first two entries.
    pub lists_8x8: [Option<ScalingList8x8>; 6],
}

/// Picture-order-count syntax retained for later slice-header parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PictureOrderCount {
    /// `pic_order_cnt_type` 0.
    TypeZero {
        /// The number of bits used by `pic_order_cnt_lsb`.
        log2_max_pic_order_cnt_lsb: u8,
    },
    /// `pic_order_cnt_type` 1.
    TypeOne {
        /// Whether delta picture-order-count fields are omitted from slices.
        delta_pic_order_always_zero: bool,
        /// The non-reference-picture offset.
        offset_for_non_ref_pic: i32,
        /// The top-to-bottom field offset.
        offset_for_top_to_bottom_field: i32,
        /// The offsets in one picture-order-count cycle.
        offsets_for_ref_frame: Vec<i32>,
    },
    /// `pic_order_cnt_type` 2.
    TypeTwo,
}

/// Frame-cropping offsets in syntax units.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameCrop {
    /// Left crop offset.
    pub left: u32,
    /// Right crop offset.
    pub right: u32,
    /// Top crop offset.
    pub top: u32,
    /// Bottom crop offset.
    pub bottom: u32,
    /// Horizontal pixels represented by one left or right offset.
    pub unit_x: u8,
    /// Vertical pixels represented by one top or bottom offset.
    pub unit_y: u8,
}

/// An aspect ratio signaled in VUI parameters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AspectRatio {
    /// The H.264 `aspect_ratio_idc` value.
    pub idc: u8,
    /// Extended sample-aspect-ratio width when `idc` is 255.
    pub sar_width: Option<u16>,
    /// Extended sample-aspect-ratio height when `idc` is 255.
    pub sar_height: Option<u16>,
}

/// Video signal and optional color-description syntax.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VideoSignalType {
    /// The three-bit `video_format` value.
    pub video_format: u8,
    /// Whether samples use full-range quantization.
    pub full_range: bool,
    /// Optional color primaries.
    pub colour_primaries: Option<u8>,
    /// Optional transfer characteristics.
    pub transfer_characteristics: Option<u8>,
    /// Optional matrix coefficients.
    pub matrix_coefficients: Option<u8>,
}

/// Chroma sample locations for top and bottom fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChromaLocation {
    /// Top-field chroma sample location type.
    pub top_field: u8,
    /// Bottom-field chroma sample location type.
    pub bottom_field: u8,
}

/// VUI timing information.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimingInfo {
    /// Number of clock ticks per time-scale unit.
    pub num_units_in_tick: u32,
    /// Number of time-scale units per second.
    pub time_scale: u32,
    /// Whether the stream has a fixed frame rate under H.264 timing rules.
    pub fixed_frame_rate: bool,
}

/// One coded-picture-buffer schedule entry from HRD syntax.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpbEntry {
    /// Raw `bit_rate_value_minus1`.
    pub bit_rate_value_minus1: u32,
    /// Raw `cpb_size_value_minus1`.
    pub cpb_size_value_minus1: u32,
    /// Whether the entry describes constant-bit-rate operation.
    pub cbr: bool,
}

/// Hypothetical reference decoder parameters.
///
/// The syntax is retained for inspection. Decoder HRD behavior is outside the
/// current scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HrdParameters {
    /// Bit-rate scaling exponent.
    pub bit_rate_scale: u8,
    /// CPB-size scaling exponent.
    pub cpb_size_scale: u8,
    /// CPB schedule entries.
    pub cpb_entries: Vec<CpbEntry>,
    /// Initial removal delay field width in bits.
    pub initial_cpb_removal_delay_length: u8,
    /// Removal delay field width in bits.
    pub cpb_removal_delay_length: u8,
    /// Output delay field width in bits.
    pub dpb_output_delay_length: u8,
    /// Time-offset field width in bits.
    pub time_offset_length: u8,
}

/// VUI bitstream restriction parameters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BitstreamRestrictions {
    /// Whether motion vectors may point outside picture boundaries.
    pub motion_vectors_over_pic_boundaries: bool,
    /// Raw maximum bytes-per-picture denominator.
    pub max_bytes_per_pic_denom: u8,
    /// Raw maximum bits-per-macroblock denominator.
    pub max_bits_per_mb_denom: u8,
    /// Maximum horizontal motion-vector length exponent.
    pub log2_max_mv_length_horizontal: u8,
    /// Maximum vertical motion-vector length exponent.
    pub log2_max_mv_length_vertical: u8,
    /// Maximum number of reordered frames.
    pub max_num_reorder_frames: u32,
    /// Maximum decoded-frame-buffer occupancy.
    pub max_dec_frame_buffering: u32,
}

/// Parsed H.264 video usability information.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VuiParameters {
    /// Optional sample aspect ratio.
    pub aspect_ratio: Option<AspectRatio>,
    /// Optional overscan-appropriate flag.
    pub overscan_appropriate: Option<bool>,
    /// Optional video signal and color description.
    pub video_signal_type: Option<VideoSignalType>,
    /// Optional chroma sample locations.
    pub chroma_location: Option<ChromaLocation>,
    /// Optional timing information.
    pub timing_info: Option<TimingInfo>,
    /// Optional NAL HRD parameters.
    pub nal_hrd_parameters: Option<HrdParameters>,
    /// Optional VCL HRD parameters.
    pub vcl_hrd_parameters: Option<HrdParameters>,
    /// Optional low-delay HRD flag, present when either HRD block is present.
    pub low_delay_hrd: Option<bool>,
    /// Whether picture timing may carry `pic_struct`.
    pub pic_struct_present: bool,
    /// Optional bitstream restrictions.
    pub bitstream_restrictions: Option<BitstreamRestrictions>,
}

/// A parsed H.264 sequence parameter set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SequenceParameterSet {
    /// Profile identifier.
    pub profile_idc: u8,
    /// Constraint-set flags zero through five.
    pub constraint_set_flags: [bool; 6],
    /// Level identifier.
    pub level_idc: u8,
    /// Sequence parameter set identifier.
    pub seq_parameter_set_id: u8,
    /// Chroma sampling format.
    pub chroma_format: ChromaFormat,
    /// Whether 4:4:4 color planes are coded separately.
    pub separate_colour_plane: bool,
    /// Luma sample bit depth.
    pub bit_depth_luma: u8,
    /// Chroma sample bit depth.
    pub bit_depth_chroma: u8,
    /// Whether the transform bypass mode is enabled for QP prime zero.
    pub qpprime_y_zero_transform_bypass: bool,
    /// Optional sequence scaling matrices.
    pub scaling_matrices: Option<SequenceScalingMatrices>,
    /// Number of bits used by `frame_num`.
    pub log2_max_frame_num: u8,
    /// Picture-order-count syntax.
    pub picture_order_count: PictureOrderCount,
    /// Maximum number of short- and long-term reference frames.
    pub max_num_ref_frames: u32,
    /// Whether gaps in `frame_num` are allowed.
    pub gaps_in_frame_num_value_allowed: bool,
    /// Picture width in macroblocks.
    pub pic_width_in_mbs: u32,
    /// Picture height in map units.
    pub pic_height_in_map_units: u32,
    /// Whether every picture is coded as a frame.
    pub frame_mbs_only: bool,
    /// Whether macroblock-adaptive frame/field coding may be used.
    pub mb_adaptive_frame_field: bool,
    /// Direct-mode 8x8 inference flag.
    pub direct_8x8_inference: bool,
    /// Optional frame cropping.
    pub frame_crop: Option<FrameCrop>,
    /// Coded width in luma samples.
    pub coded_width: u32,
    /// Coded height in luma samples.
    pub coded_height: u32,
    /// Display width after cropping.
    pub display_width: u32,
    /// Display height after cropping.
    pub display_height: u32,
    /// Optional video usability information.
    pub vui_parameters: Option<VuiParameters>,
}

/// An error returned while parsing an H.264 sequence parameter set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SequenceParameterSetError {
    /// A bitstream primitive could not be read.
    Bitstream(BitReaderError),
    /// The profile uses SPS syntax not supported by this parser.
    UnsupportedProfile {
        /// Unsupported `profile_idc`.
        profile_idc: u8,
    },
    /// Reserved SPS header bits were nonzero.
    ReservedBitsNonZero {
        /// The invalid two-bit value.
        value: u8,
    },
    /// An unsigned syntax value was outside its allowed range.
    ValueOutOfRange {
        /// Syntax field name.
        field: &'static str,
        /// Invalid value.
        value: u64,
        /// Smallest accepted value.
        minimum: u64,
        /// Largest accepted value.
        maximum: u64,
    },
    /// A signed syntax value was outside its allowed range.
    SignedValueOutOfRange {
        /// Syntax field name.
        field: &'static str,
        /// Invalid value.
        value: i64,
        /// Smallest accepted value.
        minimum: i64,
        /// Largest accepted value.
        maximum: i64,
    },
    /// A derived value did not fit its public representation.
    ArithmeticOverflow {
        /// Value being derived.
        field: &'static str,
    },
    /// Frame crop offsets remove the entire coded width or height.
    InvalidFrameCrop,
    /// A field required by the specification to be nonzero was zero.
    ZeroValue {
        /// Syntax field name.
        field: &'static str,
    },
    /// A reserved VUI aspect-ratio identifier was used.
    ReservedAspectRatio {
        /// Reserved `aspect_ratio_idc`.
        aspect_ratio_idc: u8,
    },
    /// The RBSP stop bit or its alignment bits were malformed.
    InvalidRbspTrailingBits,
}

impl fmt::Display for SequenceParameterSetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bitstream(error) => write!(formatter, "invalid SPS bitstream: {error}"),
            Self::UnsupportedProfile { profile_idc } => {
                write!(formatter, "unsupported H.264 profile_idc {profile_idc}")
            }
            Self::ReservedBitsNonZero { value } => {
                write!(
                    formatter,
                    "SPS reserved_zero_2bits is {value}, expected zero"
                )
            }
            Self::ValueOutOfRange {
                field,
                value,
                minimum,
                maximum,
            } => write!(
                formatter,
                "SPS field {field} has value {value}, expected {minimum}..={maximum}"
            ),
            Self::SignedValueOutOfRange {
                field,
                value,
                minimum,
                maximum,
            } => write!(
                formatter,
                "SPS field {field} has value {value}, expected {minimum}..={maximum}"
            ),
            Self::ArithmeticOverflow { field } => {
                write!(formatter, "SPS {field} arithmetic overflow")
            }
            Self::InvalidFrameCrop => {
                formatter.write_str("SPS frame crop removes the entire coded picture")
            }
            Self::ZeroValue { field } => {
                write!(formatter, "SPS field {field} must not be zero")
            }
            Self::ReservedAspectRatio { aspect_ratio_idc } => write!(
                formatter,
                "SPS uses reserved aspect_ratio_idc {aspect_ratio_idc}"
            ),
            Self::InvalidRbspTrailingBits => {
                formatter.write_str("SPS has invalid rbsp_trailing_bits")
            }
        }
    }
}

impl std::error::Error for SequenceParameterSetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bitstream(error) => Some(error),
            _ => None,
        }
    }
}

impl From<BitReaderError> for SequenceParameterSetError {
    fn from(error: BitReaderError) -> Self {
        Self::Bitstream(error)
    }
}

/// Parses an H.264 sequence parameter set from its RBSP bytes.
///
/// The NAL header and emulation-prevention bytes must already have been
/// removed. All variable-size syntax loops have specification-defined bounds,
/// and all dimension and cropping arithmetic is checked.
pub fn parse_sequence_parameter_set(
    rbsp: &[u8],
) -> Result<SequenceParameterSet, SequenceParameterSetError> {
    let mut reader = BitReader::new(rbsp);

    let profile_idc = read_u8(&mut reader, 8)?;
    if !is_supported_profile(profile_idc) {
        return Err(SequenceParameterSetError::UnsupportedProfile { profile_idc });
    }

    let mut constraint_set_flags = [false; 6];
    for flag in &mut constraint_set_flags {
        *flag = reader.read_bit()?;
    }
    let reserved_bits = read_u8(&mut reader, 2)?;
    if reserved_bits != 0 {
        return Err(SequenceParameterSetError::ReservedBitsNonZero {
            value: reserved_bits,
        });
    }

    let level_idc = read_u8(&mut reader, 8)?;
    let seq_parameter_set_id = read_bounded_u8(
        &mut reader,
        "seq_parameter_set_id",
        0,
        MAX_SEQUENCE_PARAMETER_SET_ID,
    )?;

    let (
        chroma_format,
        separate_colour_plane,
        bit_depth_luma,
        bit_depth_chroma,
        qpprime_y_zero_transform_bypass,
        scaling_matrices,
    ) = if has_high_profile_syntax(profile_idc) {
        let chroma_format = ChromaFormat::from_idc(reader.read_ue()?)?;
        let separate_colour_plane = chroma_format == ChromaFormat::Yuv444 && reader.read_bit()?;
        let bit_depth_luma_minus8 = read_bounded_u8(
            &mut reader,
            "bit_depth_luma_minus8",
            0,
            MAX_BIT_DEPTH_MINUS_8,
        )?;
        let bit_depth_chroma_minus8 = read_bounded_u8(
            &mut reader,
            "bit_depth_chroma_minus8",
            0,
            MAX_BIT_DEPTH_MINUS_8,
        )?;
        if bit_depth_chroma_minus8 != bit_depth_luma_minus8 {
            return Err(SequenceParameterSetError::ValueOutOfRange {
                field: "bit_depth_chroma_minus8",
                value: u64::from(bit_depth_chroma_minus8),
                minimum: u64::from(bit_depth_luma_minus8),
                maximum: u64::from(bit_depth_luma_minus8),
            });
        }
        let qpprime_y_zero_transform_bypass = reader.read_bit()?;
        let scaling_matrices = if reader.read_bit()? {
            Some(parse_scaling_matrices(
                &mut reader,
                chroma_format == ChromaFormat::Yuv444,
            )?)
        } else {
            None
        };
        (
            chroma_format,
            separate_colour_plane,
            bit_depth_luma_minus8 + 8,
            bit_depth_chroma_minus8 + 8,
            qpprime_y_zero_transform_bypass,
            scaling_matrices,
        )
    } else {
        (ChromaFormat::Yuv420, false, 8, 8, false, None)
    };

    let log2_max_frame_num_minus4 = read_bounded_u8(
        &mut reader,
        "log2_max_frame_num_minus4",
        0,
        MAX_LOG2_MINUS_4,
    )?;
    let log2_max_frame_num = log2_max_frame_num_minus4 + 4;

    let picture_order_count = match reader.read_ue()? {
        0 => {
            let minus4 = read_bounded_u8(
                &mut reader,
                "log2_max_pic_order_cnt_lsb_minus4",
                0,
                MAX_LOG2_MINUS_4,
            )?;
            PictureOrderCount::TypeZero {
                log2_max_pic_order_cnt_lsb: minus4 + 4,
            }
        }
        1 => {
            let delta_pic_order_always_zero = reader.read_bit()?;
            let offset_for_non_ref_pic = read_i32_se(&mut reader, "offset_for_non_ref_pic")?;
            let offset_for_top_to_bottom_field =
                read_i32_se(&mut reader, "offset_for_top_to_bottom_field")?;
            let cycle_length = read_bounded_u16(
                &mut reader,
                "num_ref_frames_in_pic_order_cnt_cycle",
                0,
                MAX_POC_CYCLE_LENGTH,
            )?;
            let mut offsets_for_ref_frame = Vec::with_capacity(usize::from(cycle_length));
            for _ in 0..cycle_length {
                offsets_for_ref_frame.push(read_i32_se(&mut reader, "offset_for_ref_frame")?);
            }
            PictureOrderCount::TypeOne {
                delta_pic_order_always_zero,
                offset_for_non_ref_pic,
                offset_for_top_to_bottom_field,
                offsets_for_ref_frame,
            }
        }
        2 => PictureOrderCount::TypeTwo,
        value => return Err(out_of_range("pic_order_cnt_type", value, 0, 2)),
    };

    let max_num_ref_frames = u32::from(read_bounded_u8(
        &mut reader,
        "max_num_ref_frames",
        0,
        MAX_DPB_FRAMES,
    )?);
    let gaps_in_frame_num_value_allowed = reader.read_bit()?;
    let pic_width_in_mbs_minus1 = read_u32_ue(&mut reader, "pic_width_in_mbs_minus1")?;
    let pic_height_in_map_units_minus1 =
        read_u32_ue(&mut reader, "pic_height_in_map_units_minus1")?;
    let pic_width_in_mbs = pic_width_in_mbs_minus1.checked_add(1).ok_or(
        SequenceParameterSetError::ArithmeticOverflow {
            field: "pic_width_in_mbs",
        },
    )?;
    let pic_height_in_map_units = pic_height_in_map_units_minus1.checked_add(1).ok_or(
        SequenceParameterSetError::ArithmeticOverflow {
            field: "pic_height_in_map_units",
        },
    )?;

    let frame_mbs_only = reader.read_bit()?;
    let mb_adaptive_frame_field = !frame_mbs_only && reader.read_bit()?;
    let direct_8x8_inference = reader.read_bit()?;

    let coded_width =
        pic_width_in_mbs
            .checked_mul(16)
            .ok_or(SequenceParameterSetError::ArithmeticOverflow {
                field: "coded_width",
            })?;
    let frame_height_in_mbs = pic_height_in_map_units
        .checked_mul(if frame_mbs_only { 1 } else { 2 })
        .ok_or(SequenceParameterSetError::ArithmeticOverflow {
            field: "frame_height_in_mbs",
        })?;
    let coded_height = frame_height_in_mbs.checked_mul(16).ok_or(
        SequenceParameterSetError::ArithmeticOverflow {
            field: "coded_height",
        },
    )?;

    let frame_crop = if reader.read_bit()? {
        let left = read_u32_ue(&mut reader, "frame_crop_left_offset")?;
        let right = read_u32_ue(&mut reader, "frame_crop_right_offset")?;
        let top = read_u32_ue(&mut reader, "frame_crop_top_offset")?;
        let bottom = read_u32_ue(&mut reader, "frame_crop_bottom_offset")?;
        let (unit_x, unit_y) = crop_units(chroma_format, separate_colour_plane, frame_mbs_only);
        Some(FrameCrop {
            left,
            right,
            top,
            bottom,
            unit_x,
            unit_y,
        })
    } else {
        None
    };

    let (display_width, display_height) =
        derive_display_dimensions(coded_width, coded_height, frame_crop)?;

    let vui_parameters = if reader.read_bit()? {
        Some(parse_vui_parameters(&mut reader)?)
    } else {
        None
    };
    if let Some(restrictions) = vui_parameters
        .as_ref()
        .and_then(|vui| vui.bitstream_restrictions)
        && restrictions.max_dec_frame_buffering < max_num_ref_frames
    {
        return Err(SequenceParameterSetError::ValueOutOfRange {
            field: "max_dec_frame_buffering",
            value: u64::from(restrictions.max_dec_frame_buffering),
            minimum: u64::from(max_num_ref_frames),
            maximum: u64::from(u32::MAX),
        });
    }

    parse_rbsp_trailing_bits(&mut reader)?;

    Ok(SequenceParameterSet {
        profile_idc,
        constraint_set_flags,
        level_idc,
        seq_parameter_set_id,
        chroma_format,
        separate_colour_plane,
        bit_depth_luma,
        bit_depth_chroma,
        qpprime_y_zero_transform_bypass,
        scaling_matrices,
        log2_max_frame_num,
        picture_order_count,
        max_num_ref_frames,
        gaps_in_frame_num_value_allowed,
        pic_width_in_mbs,
        pic_height_in_map_units,
        frame_mbs_only,
        mb_adaptive_frame_field,
        direct_8x8_inference,
        frame_crop,
        coded_width,
        coded_height,
        display_width,
        display_height,
        vui_parameters,
    })
}

fn is_supported_profile(profile_idc: u8) -> bool {
    matches!(profile_idc, 44 | 66 | 77 | 88 | 100 | 110 | 122 | 244)
}

fn has_high_profile_syntax(profile_idc: u8) -> bool {
    matches!(profile_idc, 44 | 100 | 110 | 122 | 244)
}

fn parse_scaling_matrices(
    reader: &mut BitReader<'_>,
    is_yuv444: bool,
) -> Result<SequenceScalingMatrices, SequenceParameterSetError> {
    let mut lists_4x4 = [None; 6];
    let mut lists_8x8 = [None; 6];
    let list_count = if is_yuv444 { 12 } else { 8 };

    for index in 0..list_count {
        if !reader.read_bit()? {
            continue;
        }
        if index < 6 {
            lists_4x4[index] = Some(parse_scaling_list_4x4(reader)?);
        } else {
            lists_8x8[index - 6] = Some(parse_scaling_list_8x8(reader)?);
        }
    }

    Ok(SequenceScalingMatrices {
        lists_4x4,
        lists_8x8,
    })
}

fn parse_scaling_list_4x4(
    reader: &mut BitReader<'_>,
) -> Result<ScalingList4x4, SequenceParameterSetError> {
    let (values, use_default) = parse_scaling_list::<16>(reader)?;
    Ok(ScalingList4x4 {
        values,
        use_default,
    })
}

fn parse_scaling_list_8x8(
    reader: &mut BitReader<'_>,
) -> Result<ScalingList8x8, SequenceParameterSetError> {
    let (values, use_default) = parse_scaling_list::<64>(reader)?;
    Ok(ScalingList8x8 {
        values,
        use_default,
    })
}

fn parse_scaling_list<const N: usize>(
    reader: &mut BitReader<'_>,
) -> Result<([u8; N], bool), SequenceParameterSetError> {
    let mut values = [0_u8; N];
    let mut last_scale = 8_i64;
    let mut next_scale = 8_i64;
    let mut use_default = false;

    for (index, value) in values.iter_mut().enumerate() {
        if next_scale != 0 {
            let delta_scale = reader.read_se()?;
            if !(-128..=127).contains(&delta_scale) {
                return Err(SequenceParameterSetError::SignedValueOutOfRange {
                    field: "delta_scale",
                    value: delta_scale,
                    minimum: -128,
                    maximum: 127,
                });
            }
            next_scale = (last_scale + delta_scale + 256) % 256;
            use_default = index == 0 && next_scale == 0;
        }
        let scale = if next_scale == 0 {
            last_scale
        } else {
            next_scale
        };
        *value =
            u8::try_from(scale).map_err(|_| SequenceParameterSetError::ArithmeticOverflow {
                field: "scaling_list",
            })?;
        last_scale = scale;
    }

    Ok((values, use_default))
}

fn crop_units(
    chroma_format: ChromaFormat,
    separate_colour_plane: bool,
    frame_mbs_only: bool,
) -> (u8, u8) {
    let frame_factor = if frame_mbs_only { 1 } else { 2 };
    if separate_colour_plane || chroma_format == ChromaFormat::Monochrome {
        return (1, frame_factor);
    }

    match chroma_format {
        ChromaFormat::Monochrome => (1, frame_factor),
        ChromaFormat::Yuv420 => (2, 2 * frame_factor),
        ChromaFormat::Yuv422 => (2, frame_factor),
        ChromaFormat::Yuv444 => (1, frame_factor),
    }
}

fn derive_display_dimensions(
    coded_width: u32,
    coded_height: u32,
    frame_crop: Option<FrameCrop>,
) -> Result<(u32, u32), SequenceParameterSetError> {
    let Some(crop) = frame_crop else {
        return Ok((coded_width, coded_height));
    };

    let horizontal_offsets =
        crop.left
            .checked_add(crop.right)
            .ok_or(SequenceParameterSetError::ArithmeticOverflow {
                field: "horizontal frame crop",
            })?;
    let vertical_offsets =
        crop.top
            .checked_add(crop.bottom)
            .ok_or(SequenceParameterSetError::ArithmeticOverflow {
                field: "vertical frame crop",
            })?;
    let horizontal_crop = horizontal_offsets
        .checked_mul(u32::from(crop.unit_x))
        .ok_or(SequenceParameterSetError::ArithmeticOverflow {
            field: "horizontal frame crop",
        })?;
    let vertical_crop = vertical_offsets.checked_mul(u32::from(crop.unit_y)).ok_or(
        SequenceParameterSetError::ArithmeticOverflow {
            field: "vertical frame crop",
        },
    )?;
    let display_width = coded_width
        .checked_sub(horizontal_crop)
        .filter(|value| *value != 0)
        .ok_or(SequenceParameterSetError::InvalidFrameCrop)?;
    let display_height = coded_height
        .checked_sub(vertical_crop)
        .filter(|value| *value != 0)
        .ok_or(SequenceParameterSetError::InvalidFrameCrop)?;
    Ok((display_width, display_height))
}

fn parse_vui_parameters(
    reader: &mut BitReader<'_>,
) -> Result<VuiParameters, SequenceParameterSetError> {
    let aspect_ratio = if reader.read_bit()? {
        let idc = read_u8(reader, 8)?;
        if idc == 255 {
            let sar_width = read_u16(reader, 16)?;
            let sar_height = read_u16(reader, 16)?;
            if sar_width == 0 {
                return Err(SequenceParameterSetError::ZeroValue { field: "sar_width" });
            }
            if sar_height == 0 {
                return Err(SequenceParameterSetError::ZeroValue {
                    field: "sar_height",
                });
            }
            Some(AspectRatio {
                idc,
                sar_width: Some(sar_width),
                sar_height: Some(sar_height),
            })
        } else if idc <= 16 {
            Some(AspectRatio {
                idc,
                sar_width: None,
                sar_height: None,
            })
        } else {
            return Err(SequenceParameterSetError::ReservedAspectRatio {
                aspect_ratio_idc: idc,
            });
        }
    } else {
        None
    };

    let overscan_appropriate = if reader.read_bit()? {
        Some(reader.read_bit()?)
    } else {
        None
    };

    let video_signal_type = if reader.read_bit()? {
        let video_format = read_u8(reader, 3)?;
        if video_format > 5 {
            return Err(out_of_range("video_format", u64::from(video_format), 0, 5));
        }
        let full_range = reader.read_bit()?;
        let colour_description_present = reader.read_bit()?;
        let (colour_primaries, transfer_characteristics, matrix_coefficients) =
            if colour_description_present {
                (
                    Some(read_u8(reader, 8)?),
                    Some(read_u8(reader, 8)?),
                    Some(read_u8(reader, 8)?),
                )
            } else {
                (None, None, None)
            };
        Some(VideoSignalType {
            video_format,
            full_range,
            colour_primaries,
            transfer_characteristics,
            matrix_coefficients,
        })
    } else {
        None
    };

    let chroma_location = if reader.read_bit()? {
        Some(ChromaLocation {
            top_field: read_bounded_u8(reader, "chroma_sample_loc_type_top_field", 0, 5)?,
            bottom_field: read_bounded_u8(reader, "chroma_sample_loc_type_bottom_field", 0, 5)?,
        })
    } else {
        None
    };

    let timing_info = if reader.read_bit()? {
        let num_units_in_tick = read_u32(reader, 32)?;
        let time_scale = read_u32(reader, 32)?;
        if num_units_in_tick == 0 {
            return Err(SequenceParameterSetError::ZeroValue {
                field: "num_units_in_tick",
            });
        }
        if time_scale == 0 {
            return Err(SequenceParameterSetError::ZeroValue {
                field: "time_scale",
            });
        }
        Some(TimingInfo {
            num_units_in_tick,
            time_scale,
            fixed_frame_rate: reader.read_bit()?,
        })
    } else {
        None
    };

    let nal_hrd_parameters = if reader.read_bit()? {
        Some(parse_hrd_parameters(reader)?)
    } else {
        None
    };
    let vcl_hrd_parameters = if reader.read_bit()? {
        Some(parse_hrd_parameters(reader)?)
    } else {
        None
    };
    let low_delay_hrd = if nal_hrd_parameters.is_some() || vcl_hrd_parameters.is_some() {
        Some(reader.read_bit()?)
    } else {
        None
    };
    let pic_struct_present = reader.read_bit()?;

    let bitstream_restrictions = if reader.read_bit()? {
        let motion_vectors_over_pic_boundaries = reader.read_bit()?;
        let max_bytes_per_pic_denom = read_bounded_u8(reader, "max_bytes_per_pic_denom", 0, 16)?;
        let max_bits_per_mb_denom = read_bounded_u8(reader, "max_bits_per_mb_denom", 0, 16)?;
        let log2_max_mv_length_horizontal =
            read_bounded_u8(reader, "log2_max_mv_length_horizontal", 0, 16)?;
        let log2_max_mv_length_vertical =
            read_bounded_u8(reader, "log2_max_mv_length_vertical", 0, 16)?;
        let max_num_reorder_frames = u32::from(read_bounded_u8(
            reader,
            "max_num_reorder_frames",
            0,
            MAX_DPB_FRAMES,
        )?);
        let max_dec_frame_buffering = u32::from(read_bounded_u8(
            reader,
            "max_dec_frame_buffering",
            0,
            MAX_DPB_FRAMES,
        )?);
        if max_num_reorder_frames > max_dec_frame_buffering {
            return Err(SequenceParameterSetError::ValueOutOfRange {
                field: "max_num_reorder_frames",
                value: u64::from(max_num_reorder_frames),
                minimum: 0,
                maximum: u64::from(max_dec_frame_buffering),
            });
        }
        Some(BitstreamRestrictions {
            motion_vectors_over_pic_boundaries,
            max_bytes_per_pic_denom,
            max_bits_per_mb_denom,
            log2_max_mv_length_horizontal,
            log2_max_mv_length_vertical,
            max_num_reorder_frames,
            max_dec_frame_buffering,
        })
    } else {
        None
    };

    Ok(VuiParameters {
        aspect_ratio,
        overscan_appropriate,
        video_signal_type,
        chroma_location,
        timing_info,
        nal_hrd_parameters,
        vcl_hrd_parameters,
        low_delay_hrd,
        pic_struct_present,
        bitstream_restrictions,
    })
}

fn parse_hrd_parameters(
    reader: &mut BitReader<'_>,
) -> Result<HrdParameters, SequenceParameterSetError> {
    let cpb_count_minus1 = read_bounded_u8(reader, "cpb_cnt_minus1", 0, MAX_CPB_COUNT_MINUS_1)?;
    let bit_rate_scale = read_u8(reader, 4)?;
    let cpb_size_scale = read_u8(reader, 4)?;
    let entry_count = usize::from(cpb_count_minus1) + 1;
    let mut cpb_entries = Vec::with_capacity(entry_count);
    for _ in 0..entry_count {
        cpb_entries.push(CpbEntry {
            bit_rate_value_minus1: read_bounded_u32(
                reader,
                "bit_rate_value_minus1",
                0,
                u64::from(u32::MAX - 1),
            )?,
            cpb_size_value_minus1: read_bounded_u32(
                reader,
                "cpb_size_value_minus1",
                0,
                u64::from(u32::MAX - 1),
            )?,
            cbr: reader.read_bit()?,
        });
    }

    Ok(HrdParameters {
        bit_rate_scale,
        cpb_size_scale,
        cpb_entries,
        initial_cpb_removal_delay_length: read_u8(reader, 5)? + 1,
        cpb_removal_delay_length: read_u8(reader, 5)? + 1,
        dpb_output_delay_length: read_u8(reader, 5)? + 1,
        time_offset_length: read_u8(reader, 5)?,
    })
}

fn parse_rbsp_trailing_bits(reader: &mut BitReader<'_>) -> Result<(), SequenceParameterSetError> {
    let remaining = reader.remaining_bits();
    if remaining == 0 || remaining > 8 || !reader.read_bit()? {
        return Err(SequenceParameterSetError::InvalidRbspTrailingBits);
    }
    while reader.remaining_bits() != 0 {
        if reader.read_bit()? {
            return Err(SequenceParameterSetError::InvalidRbspTrailingBits);
        }
    }
    Ok(())
}

fn read_u8(reader: &mut BitReader<'_>, width: usize) -> Result<u8, SequenceParameterSetError> {
    u8::try_from(reader.read_bits(width)?).map_err(|_| {
        SequenceParameterSetError::ArithmeticOverflow {
            field: "fixed-width u8",
        }
    })
}

fn read_u16(reader: &mut BitReader<'_>, width: usize) -> Result<u16, SequenceParameterSetError> {
    u16::try_from(reader.read_bits(width)?).map_err(|_| {
        SequenceParameterSetError::ArithmeticOverflow {
            field: "fixed-width u16",
        }
    })
}

fn read_u32(reader: &mut BitReader<'_>, width: usize) -> Result<u32, SequenceParameterSetError> {
    u32::try_from(reader.read_bits(width)?).map_err(|_| {
        SequenceParameterSetError::ArithmeticOverflow {
            field: "fixed-width u32",
        }
    })
}

fn read_bounded_u8(
    reader: &mut BitReader<'_>,
    field: &'static str,
    minimum: u64,
    maximum: u64,
) -> Result<u8, SequenceParameterSetError> {
    let value = reader.read_ue()?;
    if !(minimum..=maximum).contains(&value) {
        return Err(out_of_range(field, value, minimum, maximum));
    }
    u8::try_from(value).map_err(|_| SequenceParameterSetError::ArithmeticOverflow { field })
}

fn read_bounded_u16(
    reader: &mut BitReader<'_>,
    field: &'static str,
    minimum: u64,
    maximum: u64,
) -> Result<u16, SequenceParameterSetError> {
    let value = reader.read_ue()?;
    if !(minimum..=maximum).contains(&value) {
        return Err(out_of_range(field, value, minimum, maximum));
    }
    u16::try_from(value).map_err(|_| SequenceParameterSetError::ArithmeticOverflow { field })
}

fn read_u32_ue(
    reader: &mut BitReader<'_>,
    field: &'static str,
) -> Result<u32, SequenceParameterSetError> {
    let value = reader.read_ue()?;
    u32::try_from(value).map_err(|_| SequenceParameterSetError::ValueOutOfRange {
        field,
        value,
        minimum: 0,
        maximum: u64::from(u32::MAX),
    })
}

fn read_bounded_u32(
    reader: &mut BitReader<'_>,
    field: &'static str,
    minimum: u64,
    maximum: u64,
) -> Result<u32, SequenceParameterSetError> {
    let value = reader.read_ue()?;
    if !(minimum..=maximum).contains(&value) {
        return Err(out_of_range(field, value, minimum, maximum));
    }
    u32::try_from(value).map_err(|_| SequenceParameterSetError::ArithmeticOverflow { field })
}

fn read_i32_se(
    reader: &mut BitReader<'_>,
    field: &'static str,
) -> Result<i32, SequenceParameterSetError> {
    let value = reader.read_se()?;
    let minimum = i64::from(i32::MIN) + 1;
    let maximum = i64::from(i32::MAX);
    if !(minimum..=maximum).contains(&value) {
        return Err(SequenceParameterSetError::SignedValueOutOfRange {
            field,
            value,
            minimum,
            maximum,
        });
    }
    i32::try_from(value).map_err(|_| SequenceParameterSetError::ArithmeticOverflow { field })
}

fn out_of_range(
    field: &'static str,
    value: u64,
    minimum: u64,
    maximum: u64,
) -> SequenceParameterSetError {
    SequenceParameterSetError::ValueOutOfRange {
        field,
        value,
        minimum,
        maximum,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ChromaFormat, PictureOrderCount, SequenceParameterSetError, parse_sequence_parameter_set,
    };

    #[derive(Default)]
    struct BitWriter {
        bits: Vec<bool>,
    }

    impl BitWriter {
        fn bit(&mut self, value: bool) {
            self.bits.push(value);
        }

        fn bits(&mut self, value: u64, width: usize) {
            for shift in (0..width).rev() {
                self.bit((value >> shift) & 1 != 0);
            }
        }

        fn ue(&mut self, value: u64) {
            let code_num = value.checked_add(1).unwrap();
            let width = (u64::BITS - code_num.leading_zeros()) as usize;
            self.bits.extend(std::iter::repeat_n(false, width - 1));
            self.bits(code_num, width);
        }

        fn se(&mut self, value: i64) {
            let code_num = if value <= 0 {
                value.unsigned_abs() * 2
            } else {
                value as u64 * 2 - 1
            };
            self.ue(code_num);
        }

        fn finish_rbsp(mut self) -> Vec<u8> {
            self.bit(true);
            while !self.bits.len().is_multiple_of(8) {
                self.bit(false);
            }
            self.into_bytes()
        }

        fn into_bytes(self) -> Vec<u8> {
            self.bits
                .chunks(8)
                .map(|chunk| {
                    chunk.iter().enumerate().fold(0_u8, |byte, (index, bit)| {
                        byte | (u8::from(*bit) << (7 - index))
                    })
                })
                .collect()
        }
    }

    #[derive(Clone, Copy)]
    struct Fixture {
        profile_idc: u8,
        chroma_format_idc: u8,
        separate_colour_plane: bool,
        width_in_mbs: u32,
        height_in_map_units: u32,
        frame_mbs_only: bool,
        max_num_ref_frames: u32,
        crop: Option<(u32, u32, u32, u32)>,
        vui: bool,
    }

    impl Default for Fixture {
        fn default() -> Self {
            Self {
                profile_idc: 66,
                chroma_format_idc: 1,
                separate_colour_plane: false,
                width_in_mbs: 40,
                height_in_map_units: 30,
                frame_mbs_only: true,
                max_num_ref_frames: 1,
                crop: None,
                vui: false,
            }
        }
    }

    fn write_sps_prefix(writer: &mut BitWriter, fixture: Fixture) {
        writer.bits(u64::from(fixture.profile_idc), 8);
        writer.bits(0, 8);
        writer.bits(30, 8);
        writer.ue(0);
        if super::has_high_profile_syntax(fixture.profile_idc) {
            writer.ue(u64::from(fixture.chroma_format_idc));
            if fixture.chroma_format_idc == 3 {
                writer.bit(fixture.separate_colour_plane);
            }
            writer.ue(0);
            writer.ue(0);
            writer.bit(false);
            writer.bit(false);
        }
        writer.ue(0);
        writer.ue(0);
        writer.ue(0);
        writer.ue(u64::from(fixture.max_num_ref_frames));
        writer.bit(false);
        writer.ue(u64::from(fixture.width_in_mbs - 1));
        writer.ue(u64::from(fixture.height_in_map_units - 1));
        writer.bit(fixture.frame_mbs_only);
        if !fixture.frame_mbs_only {
            writer.bit(false);
        }
        writer.bit(true);
        if let Some((left, right, top, bottom)) = fixture.crop {
            writer.bit(true);
            writer.ue(u64::from(left));
            writer.ue(u64::from(right));
            writer.ue(u64::from(top));
            writer.ue(u64::from(bottom));
        } else {
            writer.bit(false);
        }
        writer.bit(fixture.vui);
    }

    fn make_sps(fixture: Fixture) -> Vec<u8> {
        let mut writer = BitWriter::default();
        write_sps_prefix(&mut writer, fixture);
        if fixture.vui {
            write_minimal_vui(&mut writer);
        }
        writer.finish_rbsp()
    }

    fn write_minimal_vui(writer: &mut BitWriter) {
        for _ in 0..5 {
            writer.bit(false);
        }
        writer.bit(false);
        writer.bit(false);
        writer.bit(false);
        writer.bit(false);
    }

    #[test]
    fn parses_representative_baseline_main_and_high_profiles() {
        for profile_idc in [66, 77, 100] {
            let fixture = Fixture {
                profile_idc,
                ..Fixture::default()
            };
            let sps = parse_sequence_parameter_set(&make_sps(fixture)).unwrap();

            assert_eq!(sps.profile_idc, profile_idc);
            assert_eq!(sps.chroma_format, ChromaFormat::Yuv420);
            assert_eq!(sps.bit_depth_luma, 8);
            assert_eq!(sps.bit_depth_chroma, 8);
            assert_eq!(sps.log2_max_frame_num, 4);
            assert_eq!(
                sps.picture_order_count,
                PictureOrderCount::TypeZero {
                    log2_max_pic_order_cnt_lsb: 4
                }
            );
            assert_eq!((sps.coded_width, sps.coded_height), (640, 480));
            assert_eq!((sps.display_width, sps.display_height), (640, 480));
        }
    }

    #[test]
    fn derives_coded_and_cropped_1080p_dimensions() {
        let fixture = Fixture {
            profile_idc: 100,
            width_in_mbs: 120,
            height_in_map_units: 68,
            crop: Some((0, 0, 0, 4)),
            ..Fixture::default()
        };
        let sps = parse_sequence_parameter_set(&make_sps(fixture)).unwrap();

        assert_eq!((sps.coded_width, sps.coded_height), (1920, 1088));
        assert_eq!((sps.display_width, sps.display_height), (1920, 1080));
        assert_eq!(sps.frame_crop.unwrap().unit_x, 2);
        assert_eq!(sps.frame_crop.unwrap().unit_y, 2);
    }

    #[test]
    fn uses_correct_crop_units_for_all_chroma_and_frame_modes() {
        let cases = [
            (0, false, true, (1, 1)),
            (1, false, true, (2, 2)),
            (2, false, true, (2, 1)),
            (3, false, true, (1, 1)),
            (3, true, true, (1, 1)),
            (0, false, false, (1, 2)),
            (1, false, false, (2, 4)),
            (2, false, false, (2, 2)),
            (3, false, false, (1, 2)),
            (3, true, false, (1, 2)),
        ];

        for (chroma_format_idc, separate_colour_plane, frame_mbs_only, expected) in cases {
            let fixture = Fixture {
                profile_idc: 100,
                chroma_format_idc,
                separate_colour_plane,
                frame_mbs_only,
                crop: Some((1, 1, 1, 1)),
                ..Fixture::default()
            };
            let sps = parse_sequence_parameter_set(&make_sps(fixture)).unwrap();
            let crop = sps.frame_crop.unwrap();
            assert_eq!(
                (crop.unit_x, crop.unit_y),
                expected,
                "chroma={chroma_format_idc}, separate={separate_colour_plane}, frame={frame_mbs_only}"
            );
            assert_eq!(
                sps.display_width,
                sps.coded_width - 2 * u32::from(expected.0)
            );
            assert_eq!(
                sps.display_height,
                sps.coded_height - 2 * u32::from(expected.1)
            );
        }
    }

    #[test]
    fn parses_type_one_picture_order_count_and_scaling_lists() {
        let mut writer = BitWriter::default();
        writer.bits(100, 8);
        writer.bits(0, 8);
        writer.bits(40, 8);
        writer.ue(3);
        writer.ue(3);
        writer.bit(false);
        writer.ue(2);
        writer.ue(2);
        writer.bit(true);
        writer.bit(true);
        writer.bit(true);
        writer.se(-8);
        for _ in 1..6 {
            writer.bit(false);
        }
        writer.bit(true);
        for _ in 0..64 {
            writer.se(0);
        }
        for _ in 7..12 {
            writer.bit(false);
        }
        writer.ue(12);
        writer.ue(1);
        writer.bit(true);
        writer.se(-2);
        writer.se(3);
        writer.ue(3);
        writer.se(-1);
        writer.se(0);
        writer.se(2);
        writer.ue(4);
        writer.bit(true);
        writer.ue(0);
        writer.ue(0);
        writer.bit(true);
        writer.bit(true);
        writer.bit(false);
        writer.bit(false);
        let rbsp = writer.finish_rbsp();

        let sps = parse_sequence_parameter_set(&rbsp).unwrap();
        assert_eq!(sps.seq_parameter_set_id, 3);
        assert_eq!(sps.chroma_format, ChromaFormat::Yuv444);
        assert_eq!(sps.bit_depth_luma, 10);
        assert_eq!(sps.bit_depth_chroma, 10);
        assert!(sps.qpprime_y_zero_transform_bypass);
        let matrices = sps.scaling_matrices.unwrap();
        assert!(matrices.lists_4x4[0].unwrap().use_default);
        assert_eq!(matrices.lists_8x8[0].unwrap().values, [8; 64]);
        assert_eq!(sps.log2_max_frame_num, 16);
        assert_eq!(
            sps.picture_order_count,
            PictureOrderCount::TypeOne {
                delta_pic_order_always_zero: true,
                offset_for_non_ref_pic: -2,
                offset_for_top_to_bottom_field: 3,
                offsets_for_ref_frame: vec![-1, 0, 2],
            }
        );
    }

    #[test]
    fn parses_comprehensive_vui_and_hrd_syntax() {
        let fixture = Fixture {
            profile_idc: 100,
            vui: true,
            ..Fixture::default()
        };
        let mut writer = BitWriter::default();
        write_sps_prefix(&mut writer, fixture);
        writer.bit(true);
        writer.bits(255, 8);
        writer.bits(4, 16);
        writer.bits(3, 16);
        writer.bit(true);
        writer.bit(false);
        writer.bit(true);
        writer.bits(5, 3);
        writer.bit(true);
        writer.bit(true);
        writer.bits(1, 8);
        writer.bits(13, 8);
        writer.bits(6, 8);
        writer.bit(true);
        writer.ue(2);
        writer.ue(3);
        writer.bit(true);
        writer.bits(1001, 32);
        writer.bits(60000, 32);
        writer.bit(true);
        writer.bit(true);
        writer.ue(0);
        writer.bits(2, 4);
        writer.bits(3, 4);
        writer.ue(99);
        writer.ue(199);
        writer.bit(true);
        writer.bits(23, 5);
        writer.bits(23, 5);
        writer.bits(23, 5);
        writer.bits(24, 5);
        writer.bit(false);
        writer.bit(false);
        writer.bit(true);
        writer.bit(true);
        writer.bit(true);
        writer.ue(2);
        writer.ue(1);
        writer.ue(16);
        writer.ue(15);
        writer.ue(2);
        writer.ue(4);
        let rbsp = writer.finish_rbsp();

        let sps = parse_sequence_parameter_set(&rbsp).unwrap();
        let vui = sps.vui_parameters.unwrap();
        let aspect_ratio = vui.aspect_ratio.unwrap();
        assert_eq!(
            (aspect_ratio.sar_width, aspect_ratio.sar_height),
            (Some(4), Some(3))
        );
        assert_eq!(vui.overscan_appropriate, Some(false));
        assert_eq!(vui.video_signal_type.unwrap().matrix_coefficients, Some(6));
        assert_eq!(vui.chroma_location.unwrap().bottom_field, 3);
        assert_eq!(vui.timing_info.unwrap().time_scale, 60000);
        let hrd = vui.nal_hrd_parameters.unwrap();
        assert_eq!(hrd.cpb_entries.len(), 1);
        assert_eq!(hrd.cpb_entries[0].bit_rate_value_minus1, 99);
        assert_eq!(hrd.initial_cpb_removal_delay_length, 24);
        assert_eq!(vui.low_delay_hrd, Some(false));
        assert!(vui.pic_struct_present);
        assert_eq!(
            vui.bitstream_restrictions.unwrap().max_dec_frame_buffering,
            4
        );
    }

    #[test]
    fn rejects_unsupported_profiles_reserved_bits_and_bounded_values() {
        let mut unsupported = make_sps(Fixture::default());
        unsupported[0] = 1;
        assert_eq!(
            parse_sequence_parameter_set(&unsupported),
            Err(SequenceParameterSetError::UnsupportedProfile { profile_idc: 1 })
        );

        let mut reserved = make_sps(Fixture::default());
        reserved[1] = 1;
        assert_eq!(
            parse_sequence_parameter_set(&reserved),
            Err(SequenceParameterSetError::ReservedBitsNonZero { value: 1 })
        );

        let mut writer = BitWriter::default();
        writer.bits(66, 8);
        writer.bits(0, 8);
        writer.bits(30, 8);
        writer.ue(super::MAX_SEQUENCE_PARAMETER_SET_ID + 1);
        assert!(matches!(
            parse_sequence_parameter_set(&writer.finish_rbsp()),
            Err(SequenceParameterSetError::ValueOutOfRange {
                field: "seq_parameter_set_id",
                ..
            })
        ));

        let extension_profile = make_sps(Fixture {
            profile_idc: 83,
            ..Fixture::default()
        });
        assert_eq!(
            parse_sequence_parameter_set(&extension_profile),
            Err(SequenceParameterSetError::UnsupportedProfile { profile_idc: 83 })
        );
    }

    #[test]
    fn rejects_crop_overflow_zero_dimensions_and_invalid_trailing_bits() {
        let fixture = Fixture {
            profile_idc: 100,
            width_in_mbs: 1,
            height_in_map_units: 1,
            crop: Some((4, 4, 0, 0)),
            ..Fixture::default()
        };
        assert_eq!(
            parse_sequence_parameter_set(&make_sps(fixture)),
            Err(SequenceParameterSetError::InvalidFrameCrop)
        );

        let mut rbsp = make_sps(Fixture::default());
        rbsp.push(0);
        assert_eq!(
            parse_sequence_parameter_set(&rbsp),
            Err(SequenceParameterSetError::InvalidRbspTrailingBits)
        );

        let mut rbsp = make_sps(Fixture::default());
        *rbsp.last_mut().unwrap() = 0;
        assert_eq!(
            parse_sequence_parameter_set(&rbsp),
            Err(SequenceParameterSetError::InvalidRbspTrailingBits)
        );
    }

    #[test]
    fn parses_type_two_and_rejects_reference_and_dimension_overflow() {
        let mut writer = BitWriter::default();
        writer.bits(66, 8);
        writer.bits(0, 8);
        writer.bits(30, 8);
        writer.ue(0);
        writer.ue(0);
        writer.ue(2);
        writer.ue(super::MAX_DPB_FRAMES);
        writer.bit(false);
        writer.ue(0);
        writer.ue(0);
        writer.bit(true);
        writer.bit(true);
        writer.bit(false);
        writer.bit(false);
        let sps = parse_sequence_parameter_set(&writer.finish_rbsp()).unwrap();
        assert_eq!(sps.picture_order_count, PictureOrderCount::TypeTwo);
        assert_eq!(sps.max_num_ref_frames, 16);

        let too_many_references = make_sps(Fixture {
            max_num_ref_frames: 17,
            ..Fixture::default()
        });
        assert!(matches!(
            parse_sequence_parameter_set(&too_many_references),
            Err(SequenceParameterSetError::ValueOutOfRange {
                field: "max_num_ref_frames",
                ..
            })
        ));

        let mut writer = BitWriter::default();
        writer.bits(66, 8);
        writer.bits(0, 8);
        writer.bits(30, 8);
        writer.ue(0);
        writer.ue(0);
        writer.ue(2);
        writer.ue(0);
        writer.bit(false);
        writer.ue(u64::from(u32::MAX));
        writer.ue(0);
        assert_eq!(
            parse_sequence_parameter_set(&writer.finish_rbsp()),
            Err(SequenceParameterSetError::ArithmeticOverflow {
                field: "pic_width_in_mbs"
            })
        );
    }

    #[test]
    fn rejects_invalid_high_profile_and_vui_constraints() {
        let mut writer = BitWriter::default();
        writer.bits(100, 8);
        writer.bits(0, 8);
        writer.bits(30, 8);
        writer.ue(0);
        writer.ue(1);
        writer.ue(0);
        writer.ue(1);
        assert!(matches!(
            parse_sequence_parameter_set(&writer.finish_rbsp()),
            Err(SequenceParameterSetError::ValueOutOfRange {
                field: "bit_depth_chroma_minus8",
                ..
            })
        ));

        let fixture = Fixture {
            vui: true,
            ..Fixture::default()
        };
        let mut writer = BitWriter::default();
        write_sps_prefix(&mut writer, fixture);
        writer.bit(true);
        writer.bits(17, 8);
        assert_eq!(
            parse_sequence_parameter_set(&writer.finish_rbsp()),
            Err(SequenceParameterSetError::ReservedAspectRatio {
                aspect_ratio_idc: 17
            })
        );

        let mut writer = BitWriter::default();
        write_sps_prefix(&mut writer, fixture);
        writer.bit(false);
        writer.bit(false);
        writer.bit(true);
        writer.bits(6, 3);
        assert!(matches!(
            parse_sequence_parameter_set(&writer.finish_rbsp()),
            Err(SequenceParameterSetError::ValueOutOfRange {
                field: "video_format",
                ..
            })
        ));

        let mut writer = BitWriter::default();
        write_sps_prefix(&mut writer, fixture);
        for _ in 0..5 {
            writer.bit(false);
        }
        writer.bit(true);
        writer.ue(super::MAX_CPB_COUNT_MINUS_1 + 1);
        assert!(matches!(
            parse_sequence_parameter_set(&writer.finish_rbsp()),
            Err(SequenceParameterSetError::ValueOutOfRange {
                field: "cpb_cnt_minus1",
                ..
            })
        ));
    }

    #[test]
    fn validates_vui_decoded_buffer_limits_against_reference_count() {
        let fixture = Fixture {
            max_num_ref_frames: 5,
            vui: true,
            ..Fixture::default()
        };
        let mut writer = BitWriter::default();
        write_sps_prefix(&mut writer, fixture);
        for _ in 0..5 {
            writer.bit(false);
        }
        writer.bit(false);
        writer.bit(false);
        writer.bit(false);
        writer.bit(true);
        writer.bit(true);
        writer.ue(0);
        writer.ue(0);
        writer.ue(0);
        writer.ue(0);
        writer.ue(0);
        writer.ue(4);

        assert!(matches!(
            parse_sequence_parameter_set(&writer.finish_rbsp()),
            Err(SequenceParameterSetError::ValueOutOfRange {
                field: "max_dec_frame_buffering",
                ..
            })
        ));
    }

    #[test]
    fn every_truncation_of_valid_sps_is_an_error() {
        let valid = make_sps(Fixture {
            profile_idc: 100,
            width_in_mbs: 120,
            height_in_map_units: 68,
            crop: Some((0, 0, 0, 4)),
            vui: true,
            ..Fixture::default()
        });

        for length in 0..valid.len() {
            assert!(
                parse_sequence_parameter_set(&valid[..length]).is_err(),
                "length={length}"
            );
        }
        assert!(parse_sequence_parameter_set(&valid).is_ok());
    }

    #[test]
    fn exhaustive_short_inputs_and_broad_structured_inputs_are_panic_free() {
        let _ = parse_sequence_parameter_set(&[]);
        for value in u8::MIN..=u8::MAX {
            let _ = parse_sequence_parameter_set(&[value]);
        }
        for value in u16::MIN..=u16::MAX {
            let _ = parse_sequence_parameter_set(&value.to_be_bytes());
        }

        const ALPHABET: [u8; 7] = [0, 1, 0x42, 0x64, 0x80, 0xfe, 0xff];
        for length in 3..=6 {
            let combinations = ALPHABET.len().pow(length as u32);
            for mut value in 0..combinations {
                let mut input = vec![0; length];
                for byte in &mut input {
                    *byte = ALPHABET[value % ALPHABET.len()];
                    value /= ALPHABET.len();
                }
                let _ = parse_sequence_parameter_set(&input);
            }
        }
    }

    #[test]
    fn errors_have_stable_descriptions() {
        assert_eq!(
            SequenceParameterSetError::UnsupportedProfile { profile_idc: 1 }.to_string(),
            "unsupported H.264 profile_idc 1"
        );
        assert_eq!(
            SequenceParameterSetError::InvalidFrameCrop.to_string(),
            "SPS frame crop removes the entire coded picture"
        );
        assert_eq!(
            SequenceParameterSetError::InvalidRbspTrailingBits.to_string(),
            "SPS has invalid rbsp_trailing_bits"
        );
    }
}
