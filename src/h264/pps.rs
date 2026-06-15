//! H.264 picture parameter set syntax.

use core::fmt;
use std::collections::{BTreeMap, HashMap};

use crate::bitstream::{BitReader, BitReaderError};

use super::{ChromaFormat, ScalingList4x4, ScalingList8x8, SequenceParameterSet};

const MAX_PICTURE_PARAMETER_SET_ID: u64 = 255;
const MAX_SEQUENCE_PARAMETER_SET_ID: u64 = 31;
const MAX_SLICE_GROUPS_MINUS1: u64 = 7;
const MAX_REF_IDX_ACTIVE_MINUS1: u64 = 31;
const MAX_EXPLICIT_SLICE_GROUP_MAP_UNITS: u32 = 1_048_576;

/// Looks up sequence parameter sets by their syntax identifier.
///
/// Decoder state can implement this trait without exposing its storage model.
pub trait SequenceParameterSetLookup {
    /// Returns the sequence parameter set with `id`, if it is available.
    fn sequence_parameter_set(&self, id: u8) -> Option<&SequenceParameterSet>;
}

impl SequenceParameterSetLookup for [Option<SequenceParameterSet>; 32] {
    fn sequence_parameter_set(&self, id: u8) -> Option<&SequenceParameterSet> {
        self.get(usize::from(id)).and_then(Option::as_ref)
    }
}

impl SequenceParameterSetLookup for BTreeMap<u8, SequenceParameterSet> {
    fn sequence_parameter_set(&self, id: u8) -> Option<&SequenceParameterSet> {
        self.get(&id)
    }
}

impl SequenceParameterSetLookup for HashMap<u8, SequenceParameterSet> {
    fn sequence_parameter_set(&self, id: u8) -> Option<&SequenceParameterSet> {
        self.get(&id)
    }
}

/// A foreground slice-group rectangle in map-unit coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SliceGroupRectangle {
    /// First map-unit address in raster-scan order.
    pub top_left: u32,
    /// Last map-unit address in raster-scan order.
    pub bottom_right: u32,
}

/// Dynamic slice-group map ordering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SliceGroupChangeType {
    /// Box-out map type 3.
    BoxOut,
    /// Raster-scan map type 4.
    RasterScan,
    /// Wipe map type 5.
    Wipe,
}

/// Flexible macroblock ordering syntax.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SliceGroups {
    /// Map type 0, interleaved groups.
    Interleaved {
        /// Run lengths for each group.
        run_lengths: Vec<u32>,
    },
    /// Map type 1, dispersed groups.
    Dispersed {
        /// Number of slice groups.
        group_count: u8,
    },
    /// Map type 2, foreground rectangles plus a leftover group.
    Foreground {
        /// Foreground rectangles in group order.
        rectangles: Vec<SliceGroupRectangle>,
    },
    /// Map types 3 through 5.
    Changing {
        /// Dynamic map construction type.
        change_type: SliceGroupChangeType,
        /// Direction flag from the PPS.
        direction: bool,
        /// Number of map units added or removed per change cycle.
        change_rate: u32,
    },
    /// Map type 6, an explicit group identifier for every map unit.
    Explicit {
        /// Number of declared slice groups.
        group_count: u8,
        /// Slice-group identifier for each map unit.
        slice_group_ids: Vec<u8>,
    },
}

impl SliceGroups {
    /// Returns the number of slice groups.
    #[must_use]
    pub fn group_count(&self) -> u8 {
        match self {
            Self::Interleaved { run_lengths } => u8::try_from(run_lengths.len()).unwrap_or(8),
            Self::Dispersed { group_count } => *group_count,
            Self::Foreground { rectangles } => {
                u8::try_from(rectangles.len().saturating_add(1)).unwrap_or(8)
            }
            Self::Changing { .. } => 2,
            Self::Explicit { group_count, .. } => *group_count,
        }
    }
}

/// Scaling matrices explicitly present in a picture parameter set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PictureScalingMatrices {
    /// The six possible 4x4 scaling lists.
    pub lists_4x4: [Option<ScalingList4x4>; 6],
    /// The six possible 8x8 scaling lists. Non-4:4:4 streams use at most two.
    pub lists_8x8: [Option<ScalingList8x8>; 6],
}

/// A parsed H.264 picture parameter set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PictureParameterSet {
    /// Picture parameter set identifier.
    pub pic_parameter_set_id: u8,
    /// Referenced sequence parameter set identifier.
    pub seq_parameter_set_id: u8,
    /// Whether slice data uses CABAC rather than CAVLC.
    pub entropy_coding_mode: bool,
    /// Whether bottom-field picture order is present in frame slice headers.
    pub bottom_field_pic_order_in_frame_present: bool,
    /// Optional flexible macroblock ordering syntax.
    pub slice_groups: Option<SliceGroups>,
    /// Default active list-0 reference count.
    pub num_ref_idx_l0_default_active: u8,
    /// Default active list-1 reference count.
    pub num_ref_idx_l1_default_active: u8,
    /// Whether explicit weighted prediction is used for P and SP slices.
    pub weighted_pred: bool,
    /// Weighted biprediction mode for B slices.
    pub weighted_bipred_idc: u8,
    /// Initial luma quantization parameter.
    pub pic_init_qp: i16,
    /// Initial SP/SI quantization parameter.
    pub pic_init_qs: i8,
    /// First chroma quantization parameter offset.
    pub chroma_qp_index_offset: i8,
    /// Whether slice headers carry deblocking filter controls.
    pub deblocking_filter_control_present: bool,
    /// Whether intra prediction is constrained to intra-coded neighbors.
    pub constrained_intra_pred: bool,
    /// Whether slice headers carry redundant picture counts.
    pub redundant_pic_cnt_present: bool,
    /// Whether 8x8 transform syntax is enabled.
    pub transform_8x8_mode: bool,
    /// Optional picture scaling matrices.
    pub scaling_matrices: Option<PictureScalingMatrices>,
    /// Second chroma quantization parameter offset.
    pub second_chroma_qp_index_offset: i8,
}

/// An error returned while parsing an H.264 picture parameter set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PictureParameterSetError {
    /// A bitstream primitive could not be read.
    Bitstream(BitReaderError),
    /// The referenced sequence parameter set is not available.
    MissingSequenceParameterSet {
        /// Missing `seq_parameter_set_id`.
        seq_parameter_set_id: u8,
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
    /// Valid syntax uses a feature the parser intentionally does not support.
    Unsupported {
        /// Unsupported syntax feature.
        feature: &'static str,
    },
    /// Slice-group rectangle coordinates do not describe a valid rectangle.
    InvalidSliceGroupRectangle {
        /// Rectangle index.
        index: u8,
        /// Raw top-left map-unit address.
        top_left: u32,
        /// Raw bottom-right map-unit address.
        bottom_right: u32,
    },
    /// The RBSP stop bit or its alignment bits were malformed.
    InvalidRbspTrailingBits,
}

impl fmt::Display for PictureParameterSetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bitstream(error) => write!(formatter, "invalid PPS bitstream: {error}"),
            Self::MissingSequenceParameterSet {
                seq_parameter_set_id,
            } => write!(
                formatter,
                "PPS references missing sequence parameter set {seq_parameter_set_id}"
            ),
            Self::ValueOutOfRange {
                field,
                value,
                minimum,
                maximum,
            } => write!(
                formatter,
                "PPS field {field} has value {value}, expected {minimum}..={maximum}"
            ),
            Self::SignedValueOutOfRange {
                field,
                value,
                minimum,
                maximum,
            } => write!(
                formatter,
                "PPS field {field} has value {value}, expected {minimum}..={maximum}"
            ),
            Self::Unsupported { feature } => {
                write!(formatter, "PPS uses unsupported {feature}")
            }
            Self::InvalidSliceGroupRectangle {
                index,
                top_left,
                bottom_right,
            } => write!(
                formatter,
                "PPS slice-group rectangle {index} is invalid: {top_left}..={bottom_right}"
            ),
            Self::InvalidRbspTrailingBits => {
                formatter.write_str("PPS has invalid rbsp_trailing_bits")
            }
        }
    }
}

impl std::error::Error for PictureParameterSetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bitstream(error) => Some(error),
            _ => None,
        }
    }
}

impl From<BitReaderError> for PictureParameterSetError {
    fn from(error: BitReaderError) -> Self {
        Self::Bitstream(error)
    }
}

/// Parses an H.264 picture parameter set from its RBSP bytes.
///
/// The NAL header and emulation-prevention bytes must already have been
/// removed. The referenced SPS is resolved before SPS-dependent fields are
/// parsed. Every syntax loop is bounded by H.264 limits or the SPS picture
/// dimensions.
pub fn parse_picture_parameter_set(
    rbsp: &[u8],
    sequence_parameter_sets: &impl SequenceParameterSetLookup,
) -> Result<PictureParameterSet, PictureParameterSetError> {
    let mut reader = BitReader::new(rbsp);

    let pic_parameter_set_id = read_bounded_u8(
        &mut reader,
        "pic_parameter_set_id",
        0,
        MAX_PICTURE_PARAMETER_SET_ID,
    )?;
    let seq_parameter_set_id = read_bounded_u8(
        &mut reader,
        "seq_parameter_set_id",
        0,
        MAX_SEQUENCE_PARAMETER_SET_ID,
    )?;
    let sps = sequence_parameter_sets
        .sequence_parameter_set(seq_parameter_set_id)
        .ok_or(PictureParameterSetError::MissingSequenceParameterSet {
            seq_parameter_set_id,
        })?;

    let entropy_coding_mode = reader.read_bit()?;
    let bottom_field_pic_order_in_frame_present = reader.read_bit()?;
    let slice_groups = parse_slice_groups(&mut reader, sps)?;
    let num_ref_idx_l0_default_active = read_bounded_u8(
        &mut reader,
        "num_ref_idx_l0_default_active_minus1",
        0,
        MAX_REF_IDX_ACTIVE_MINUS1,
    )?
    .saturating_add(1);
    let num_ref_idx_l1_default_active = read_bounded_u8(
        &mut reader,
        "num_ref_idx_l1_default_active_minus1",
        0,
        MAX_REF_IDX_ACTIVE_MINUS1,
    )?
    .saturating_add(1);
    let weighted_pred = reader.read_bit()?;
    let weighted_bipred_idc = read_u8(&mut reader, 2)?;
    if weighted_bipred_idc > 2 {
        return Err(out_of_range(
            "weighted_bipred_idc",
            u64::from(weighted_bipred_idc),
            0,
            2,
        ));
    }

    let qp_bd_offset_y = i64::from(sps.bit_depth_luma.saturating_sub(8)) * 6;
    let pic_init_qp_minus26 =
        read_bounded_se(&mut reader, "pic_init_qp_minus26", -26 - qp_bd_offset_y, 25)?;
    let pic_init_qs_minus26 = read_bounded_se(&mut reader, "pic_init_qs_minus26", -26, 25)?;
    let chroma_qp_index_offset = read_bounded_se(&mut reader, "chroma_qp_index_offset", -12, 12)?;
    let deblocking_filter_control_present = reader.read_bit()?;
    let constrained_intra_pred = reader.read_bit()?;
    let redundant_pic_cnt_present = reader.read_bit()?;

    let mut transform_8x8_mode = false;
    let mut scaling_matrices = None;
    let mut second_chroma_qp_index_offset = chroma_qp_index_offset;
    if more_rbsp_data(&reader) {
        transform_8x8_mode = reader.read_bit()?;
        if reader.read_bit()? {
            scaling_matrices = Some(parse_scaling_matrices(
                &mut reader,
                sps.chroma_format,
                transform_8x8_mode,
            )?);
        }
        second_chroma_qp_index_offset =
            read_bounded_se(&mut reader, "second_chroma_qp_index_offset", -12, 12)?;
    }

    parse_rbsp_trailing_bits(&mut reader)?;

    Ok(PictureParameterSet {
        pic_parameter_set_id,
        seq_parameter_set_id,
        entropy_coding_mode,
        bottom_field_pic_order_in_frame_present,
        slice_groups,
        num_ref_idx_l0_default_active,
        num_ref_idx_l1_default_active,
        weighted_pred,
        weighted_bipred_idc,
        pic_init_qp: i16::try_from(pic_init_qp_minus26 + 26).map_err(|_| {
            PictureParameterSetError::Unsupported {
                feature: "luma bit depth too large for pic_init_qp",
            }
        })?,
        pic_init_qs: i8::try_from(pic_init_qs_minus26 + 26).map_err(|_| {
            PictureParameterSetError::Unsupported {
                feature: "pic_init_qs representation",
            }
        })?,
        chroma_qp_index_offset: i8::try_from(chroma_qp_index_offset).map_err(|_| {
            PictureParameterSetError::Unsupported {
                feature: "chroma QP offset representation",
            }
        })?,
        deblocking_filter_control_present,
        constrained_intra_pred,
        redundant_pic_cnt_present,
        transform_8x8_mode,
        scaling_matrices,
        second_chroma_qp_index_offset: i8::try_from(second_chroma_qp_index_offset).map_err(
            |_| PictureParameterSetError::Unsupported {
                feature: "second chroma QP offset representation",
            },
        )?,
    })
}

fn parse_slice_groups(
    reader: &mut BitReader<'_>,
    sps: &SequenceParameterSet,
) -> Result<Option<SliceGroups>, PictureParameterSetError> {
    let num_slice_groups_minus1 = read_bounded_u8(
        reader,
        "num_slice_groups_minus1",
        0,
        MAX_SLICE_GROUPS_MINUS1,
    )?;
    if num_slice_groups_minus1 == 0 {
        return Ok(None);
    }

    let group_count = num_slice_groups_minus1 + 1;
    let pic_size = picture_size_in_map_units(sps)?;
    let map_type = read_bounded_u8(reader, "slice_group_map_type", 0, 6)?;
    let groups = match map_type {
        0 => {
            let mut run_lengths = Vec::with_capacity(usize::from(group_count));
            for _ in 0..group_count {
                let run_length_minus1 = read_bounded_u32(
                    reader,
                    "run_length_minus1",
                    0,
                    u64::from(pic_size.saturating_sub(1)),
                )?;
                run_lengths.push(run_length_minus1 + 1);
            }
            SliceGroups::Interleaved { run_lengths }
        }
        1 => SliceGroups::Dispersed { group_count },
        2 => {
            let rectangle_count = usize::from(num_slice_groups_minus1);
            let mut rectangles = Vec::with_capacity(rectangle_count);
            for index in 0..rectangle_count {
                let top_left = read_bounded_u32(reader, "top_left", 0, u64::from(pic_size - 1))?;
                let bottom_right =
                    read_bounded_u32(reader, "bottom_right", 0, u64::from(pic_size - 1))?;
                let valid = top_left <= bottom_right
                    && top_left % sps.pic_width_in_mbs <= bottom_right % sps.pic_width_in_mbs;
                if !valid {
                    return Err(PictureParameterSetError::InvalidSliceGroupRectangle {
                        index: u8::try_from(index).unwrap_or(7),
                        top_left,
                        bottom_right,
                    });
                }
                rectangles.push(SliceGroupRectangle {
                    top_left,
                    bottom_right,
                });
            }
            SliceGroups::Foreground { rectangles }
        }
        3..=5 => {
            if group_count != 2 {
                return Err(out_of_range(
                    "num_slice_groups_minus1",
                    u64::from(num_slice_groups_minus1),
                    1,
                    1,
                ));
            }
            let direction = reader.read_bit()?;
            let change_rate_minus1 = read_bounded_u32(
                reader,
                "slice_group_change_rate_minus1",
                0,
                u64::from(pic_size - 1),
            )?;
            let change_type = match map_type {
                3 => SliceGroupChangeType::BoxOut,
                4 => SliceGroupChangeType::RasterScan,
                _ => SliceGroupChangeType::Wipe,
            };
            SliceGroups::Changing {
                change_type,
                direction,
                change_rate: change_rate_minus1 + 1,
            }
        }
        6 => {
            if pic_size > MAX_EXPLICIT_SLICE_GROUP_MAP_UNITS {
                return Err(PictureParameterSetError::Unsupported {
                    feature: "explicit slice-group map exceeding 1048576 map units",
                });
            }
            let pic_size_in_map_units_minus1 = read_bounded_u32(
                reader,
                "pic_size_in_map_units_minus1",
                0,
                u64::from(pic_size - 1),
            )?;
            if pic_size_in_map_units_minus1 != pic_size - 1 {
                return Err(out_of_range(
                    "pic_size_in_map_units_minus1",
                    u64::from(pic_size_in_map_units_minus1),
                    u64::from(pic_size - 1),
                    u64::from(pic_size - 1),
                ));
            }
            let id_width = (u8::BITS - num_slice_groups_minus1.leading_zeros()) as usize;
            let mut slice_group_ids = Vec::with_capacity(pic_size as usize);
            for _ in 0..pic_size {
                let id = read_u8(reader, id_width)?;
                if id >= group_count {
                    return Err(out_of_range(
                        "slice_group_id",
                        u64::from(id),
                        0,
                        u64::from(group_count - 1),
                    ));
                }
                slice_group_ids.push(id);
            }
            SliceGroups::Explicit {
                group_count,
                slice_group_ids,
            }
        }
        value => {
            return Err(out_of_range("slice_group_map_type", u64::from(value), 0, 6));
        }
    };
    Ok(Some(groups))
}

fn picture_size_in_map_units(sps: &SequenceParameterSet) -> Result<u32, PictureParameterSetError> {
    sps.pic_width_in_mbs
        .checked_mul(sps.pic_height_in_map_units)
        .filter(|size| *size != 0)
        .ok_or(PictureParameterSetError::Unsupported {
            feature: "SPS picture size outside PPS parser limits",
        })
}

fn parse_scaling_matrices(
    reader: &mut BitReader<'_>,
    chroma_format: ChromaFormat,
    transform_8x8_mode: bool,
) -> Result<PictureScalingMatrices, PictureParameterSetError> {
    let mut lists_4x4 = [None; 6];
    let mut lists_8x8 = [None; 6];
    let list_count = 6 + if transform_8x8_mode {
        if chroma_format == ChromaFormat::Yuv444 {
            6
        } else {
            2
        }
    } else {
        0
    };

    for index in 0..list_count {
        if !reader.read_bit()? {
            continue;
        }
        if index < 6 {
            let (values, use_default) = parse_scaling_list::<16>(reader)?;
            lists_4x4[index] = Some(ScalingList4x4 {
                values,
                use_default,
            });
        } else {
            let (values, use_default) = parse_scaling_list::<64>(reader)?;
            lists_8x8[index - 6] = Some(ScalingList8x8 {
                values,
                use_default,
            });
        }
    }

    Ok(PictureScalingMatrices {
        lists_4x4,
        lists_8x8,
    })
}

fn parse_scaling_list<const N: usize>(
    reader: &mut BitReader<'_>,
) -> Result<([u8; N], bool), PictureParameterSetError> {
    let mut values = [0_u8; N];
    let mut last_scale = 8_i64;
    let mut next_scale = 8_i64;
    let mut use_default = false;

    for (index, value) in values.iter_mut().enumerate() {
        if next_scale != 0 {
            let delta_scale = read_bounded_se(reader, "delta_scale", -128, 127)?;
            next_scale = (last_scale + delta_scale + 256) % 256;
            use_default = index == 0 && next_scale == 0;
        }
        let scale = if next_scale == 0 {
            last_scale
        } else {
            next_scale
        };
        *value = u8::try_from(scale).map_err(|_| PictureParameterSetError::Unsupported {
            feature: "scaling-list value representation",
        })?;
        last_scale = scale;
    }

    Ok((values, use_default))
}

fn more_rbsp_data(reader: &BitReader<'_>) -> bool {
    let remaining = reader.remaining_bits();
    if remaining == 0 || remaining > 8 {
        return remaining != 0;
    }

    let mut trailing = reader.clone();
    if trailing.read_bit() != Ok(true) {
        return true;
    }
    while trailing.remaining_bits() != 0 {
        if trailing.read_bit() != Ok(false) {
            return true;
        }
    }
    false
}

fn parse_rbsp_trailing_bits(reader: &mut BitReader<'_>) -> Result<(), PictureParameterSetError> {
    let remaining = reader.remaining_bits();
    if remaining == 0 || remaining > 8 || !reader.read_bit()? {
        return Err(PictureParameterSetError::InvalidRbspTrailingBits);
    }
    while reader.remaining_bits() != 0 {
        if reader.read_bit()? {
            return Err(PictureParameterSetError::InvalidRbspTrailingBits);
        }
    }
    Ok(())
}

fn read_u8(reader: &mut BitReader<'_>, width: usize) -> Result<u8, PictureParameterSetError> {
    u8::try_from(reader.read_bits(width)?).map_err(|_| PictureParameterSetError::Unsupported {
        feature: "fixed-width u8 representation",
    })
}

fn read_bounded_u8(
    reader: &mut BitReader<'_>,
    field: &'static str,
    minimum: u64,
    maximum: u64,
) -> Result<u8, PictureParameterSetError> {
    let value = reader.read_ue()?;
    if !(minimum..=maximum).contains(&value) {
        return Err(out_of_range(field, value, minimum, maximum));
    }
    u8::try_from(value).map_err(|_| PictureParameterSetError::Unsupported {
        feature: "bounded u8 representation",
    })
}

fn read_bounded_u32(
    reader: &mut BitReader<'_>,
    field: &'static str,
    minimum: u64,
    maximum: u64,
) -> Result<u32, PictureParameterSetError> {
    let value = reader.read_ue()?;
    if !(minimum..=maximum).contains(&value) {
        return Err(out_of_range(field, value, minimum, maximum));
    }
    u32::try_from(value).map_err(|_| PictureParameterSetError::Unsupported {
        feature: "bounded u32 representation",
    })
}

fn read_bounded_se(
    reader: &mut BitReader<'_>,
    field: &'static str,
    minimum: i64,
    maximum: i64,
) -> Result<i64, PictureParameterSetError> {
    let value = reader.read_se()?;
    if !(minimum..=maximum).contains(&value) {
        return Err(PictureParameterSetError::SignedValueOutOfRange {
            field,
            value,
            minimum,
            maximum,
        });
    }
    Ok(value)
}

fn out_of_range(
    field: &'static str,
    value: u64,
    minimum: u64,
    maximum: u64,
) -> PictureParameterSetError {
    PictureParameterSetError::ValueOutOfRange {
        field,
        value,
        minimum,
        maximum,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PictureParameterSetError, SliceGroupChangeType, SliceGroups, parse_picture_parameter_set,
    };
    use crate::h264::{SequenceParameterSet, parse_sequence_parameter_set};

    type SliceGroupCase = (u8, Box<dyn Fn(&mut BitWriter)>, Box<dyn Fn(&SliceGroups)>);

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

    fn make_sps(
        profile_idc: u8,
        chroma_format_idc: u8,
        width: u32,
        height: u32,
    ) -> SequenceParameterSet {
        let mut writer = BitWriter::default();
        writer.bits(u64::from(profile_idc), 8);
        writer.bits(0, 8);
        writer.bits(30, 8);
        writer.ue(0);
        if matches!(profile_idc, 44 | 100 | 110 | 122 | 244) {
            writer.ue(u64::from(chroma_format_idc));
            if chroma_format_idc == 3 {
                writer.bit(false);
            }
            writer.ue(0);
            writer.ue(0);
            writer.bit(false);
            writer.bit(false);
        }
        writer.ue(0);
        writer.ue(0);
        writer.ue(0);
        writer.ue(1);
        writer.bit(false);
        writer.ue(u64::from(width - 1));
        writer.ue(u64::from(height - 1));
        writer.bit(true);
        writer.bit(true);
        writer.bit(false);
        writer.bit(false);
        parse_sequence_parameter_set(&writer.finish_rbsp()).unwrap()
    }

    fn lookup_with(sps: SequenceParameterSet) -> [Option<SequenceParameterSet>; 32] {
        let mut lookup = std::array::from_fn(|_| None);
        let id = usize::from(sps.seq_parameter_set_id);
        lookup[id] = Some(sps);
        lookup
    }

    fn write_pps_prefix(writer: &mut BitWriter, pps_id: u64, sps_id: u64) {
        writer.ue(pps_id);
        writer.ue(sps_id);
        writer.bit(false);
        writer.bit(false);
    }

    fn write_pps_suffix(writer: &mut BitWriter) {
        writer.ue(0);
        writer.ue(0);
        writer.bit(false);
        writer.bits(0, 2);
        writer.se(0);
        writer.se(0);
        writer.se(0);
        writer.bit(true);
        writer.bit(false);
        writer.bit(false);
    }

    fn make_minimal_pps() -> Vec<u8> {
        let mut writer = BitWriter::default();
        write_pps_prefix(&mut writer, 0, 0);
        writer.ue(0);
        write_pps_suffix(&mut writer);
        writer.finish_rbsp()
    }

    #[test]
    fn parses_representative_baseline_fields_and_defaults() {
        let lookup = lookup_with(make_sps(66, 1, 40, 30));
        let mut writer = BitWriter::default();
        writer.ue(17);
        writer.ue(0);
        writer.bit(true);
        writer.bit(true);
        writer.ue(0);
        writer.ue(2);
        writer.ue(3);
        writer.bit(true);
        writer.bits(2, 2);
        writer.se(-4);
        writer.se(3);
        writer.se(-2);
        writer.bit(true);
        writer.bit(true);
        writer.bit(true);

        let pps = parse_picture_parameter_set(&writer.finish_rbsp(), &lookup).unwrap();
        assert_eq!(pps.pic_parameter_set_id, 17);
        assert!(pps.entropy_coding_mode);
        assert!(pps.bottom_field_pic_order_in_frame_present);
        assert_eq!(pps.num_ref_idx_l0_default_active, 3);
        assert_eq!(pps.num_ref_idx_l1_default_active, 4);
        assert!(pps.weighted_pred);
        assert_eq!(pps.weighted_bipred_idc, 2);
        assert_eq!(pps.pic_init_qp, 22);
        assert_eq!(pps.pic_init_qs, 29);
        assert_eq!(pps.chroma_qp_index_offset, -2);
        assert_eq!(pps.second_chroma_qp_index_offset, -2);
        assert!(!pps.transform_8x8_mode);
    }

    #[test]
    fn parses_all_slice_group_map_types() {
        let lookup = lookup_with(make_sps(66, 1, 4, 3));

        let cases: Vec<SliceGroupCase> = vec![
            (
                0,
                Box::new(|writer| {
                    writer.ue(1);
                    writer.ue(2);
                }),
                Box::new(|groups| {
                    assert_eq!(
                        groups,
                        &SliceGroups::Interleaved {
                            run_lengths: vec![2, 3]
                        }
                    );
                }),
            ),
            (
                1,
                Box::new(|_| {}),
                Box::new(|groups| {
                    assert_eq!(groups, &SliceGroups::Dispersed { group_count: 2 });
                }),
            ),
            (
                2,
                Box::new(|writer| {
                    writer.ue(1);
                    writer.ue(6);
                }),
                Box::new(|groups| {
                    assert!(matches!(
                        groups,
                        SliceGroups::Foreground { rectangles }
                            if rectangles[0].top_left == 1 && rectangles[0].bottom_right == 6
                    ));
                }),
            ),
            (
                3,
                Box::new(|writer| {
                    writer.bit(true);
                    writer.ue(3);
                }),
                Box::new(|groups| {
                    assert_eq!(
                        groups,
                        &SliceGroups::Changing {
                            change_type: SliceGroupChangeType::BoxOut,
                            direction: true,
                            change_rate: 4
                        }
                    );
                }),
            ),
            (
                4,
                Box::new(|writer| {
                    writer.bit(false);
                    writer.ue(0);
                }),
                Box::new(|groups| {
                    assert!(matches!(
                        groups,
                        SliceGroups::Changing {
                            change_type: SliceGroupChangeType::RasterScan,
                            ..
                        }
                    ));
                }),
            ),
            (
                5,
                Box::new(|writer| {
                    writer.bit(false);
                    writer.ue(11);
                }),
                Box::new(|groups| {
                    assert!(matches!(
                        groups,
                        SliceGroups::Changing {
                            change_type: SliceGroupChangeType::Wipe,
                            change_rate: 12,
                            ..
                        }
                    ));
                }),
            ),
            (
                6,
                Box::new(|writer| {
                    writer.ue(11);
                    for index in 0..12 {
                        writer.bits(index % 2, 1);
                    }
                }),
                Box::new(|groups| {
                    assert!(matches!(
                        groups,
                        SliceGroups::Explicit {
                            group_count: 2,
                            slice_group_ids
                        }
                            if slice_group_ids == &[0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1]
                    ));
                }),
            ),
        ];

        for (map_type, write_map, check) in cases {
            let mut writer = BitWriter::default();
            write_pps_prefix(&mut writer, u64::from(map_type), 0);
            writer.ue(1);
            writer.ue(u64::from(map_type));
            write_map(&mut writer);
            write_pps_suffix(&mut writer);
            let pps = parse_picture_parameter_set(&writer.finish_rbsp(), &lookup)
                .unwrap_or_else(|error| panic!("map_type={map_type}: {error}"));
            check(pps.slice_groups.as_ref().unwrap());
        }
    }

    #[test]
    fn parses_high_profile_transform_and_scaling_matrices() {
        let lookup = lookup_with(make_sps(100, 1, 4, 3));
        let mut writer = BitWriter::default();
        write_pps_prefix(&mut writer, 0, 0);
        writer.ue(0);
        write_pps_suffix(&mut writer);
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
        writer.bit(false);
        writer.se(7);

        let pps = parse_picture_parameter_set(&writer.finish_rbsp(), &lookup).unwrap();
        assert!(pps.transform_8x8_mode);
        assert_eq!(pps.second_chroma_qp_index_offset, 7);
        let matrices = pps.scaling_matrices.unwrap();
        assert!(matrices.lists_4x4[0].unwrap().use_default);
        assert_eq!(matrices.lists_8x8[0].unwrap().values, [8; 64]);
        assert!(matrices.lists_8x8[1].is_none());
    }

    #[test]
    fn uses_sps_chroma_format_for_yuv444_scaling_list_count() {
        let lookup = lookup_with(make_sps(100, 3, 1, 1));
        let mut writer = BitWriter::default();
        write_pps_prefix(&mut writer, 0, 0);
        writer.ue(0);
        write_pps_suffix(&mut writer);
        writer.bit(true);
        writer.bit(true);
        for _ in 0..11 {
            writer.bit(false);
        }
        writer.bit(true);
        writer.se(-8);
        writer.se(-12);

        let pps = parse_picture_parameter_set(&writer.finish_rbsp(), &lookup).unwrap();
        assert!(
            pps.scaling_matrices.unwrap().lists_8x8[5]
                .unwrap()
                .use_default
        );
        assert_eq!(pps.second_chroma_qp_index_offset, -12);
    }

    #[test]
    fn rejects_missing_sps_and_identifier_ranges() {
        let empty = std::array::from_fn(|_| None);
        assert_eq!(
            parse_picture_parameter_set(&make_minimal_pps(), &empty),
            Err(PictureParameterSetError::MissingSequenceParameterSet {
                seq_parameter_set_id: 0
            })
        );

        let lookup = lookup_with(make_sps(66, 1, 1, 1));
        let mut writer = BitWriter::default();
        writer.ue(256);
        assert!(matches!(
            parse_picture_parameter_set(&writer.finish_rbsp(), &lookup),
            Err(PictureParameterSetError::ValueOutOfRange {
                field: "pic_parameter_set_id",
                ..
            })
        ));

        let mut writer = BitWriter::default();
        writer.ue(0);
        writer.ue(32);
        assert!(matches!(
            parse_picture_parameter_set(&writer.finish_rbsp(), &lookup),
            Err(PictureParameterSetError::ValueOutOfRange {
                field: "seq_parameter_set_id",
                ..
            })
        ));
    }

    #[test]
    fn rejects_invalid_field_ranges() {
        let lookup = lookup_with(make_sps(66, 1, 4, 3));

        for (field, write_invalid) in [
            (
                "num_slice_groups_minus1",
                Box::new(|writer: &mut BitWriter| writer.ue(8)) as Box<dyn Fn(&mut BitWriter)>,
            ),
            (
                "weighted_bipred_idc",
                Box::new(|writer: &mut BitWriter| {
                    writer.ue(0);
                    writer.ue(0);
                    writer.ue(0);
                    writer.bit(false);
                    writer.bits(3, 2);
                }),
            ),
            (
                "pic_init_qp_minus26",
                Box::new(|writer: &mut BitWriter| {
                    writer.ue(0);
                    writer.ue(0);
                    writer.ue(0);
                    writer.bit(false);
                    writer.bits(0, 2);
                    writer.se(26);
                }),
            ),
        ] {
            let mut writer = BitWriter::default();
            write_pps_prefix(&mut writer, 0, 0);
            write_invalid(&mut writer);
            let result = parse_picture_parameter_set(&writer.finish_rbsp(), &lookup);
            assert!(
                matches!(
                    result,
                    Err(PictureParameterSetError::ValueOutOfRange {
                        field: actual,
                        ..
                    }) if actual == field
                ) || matches!(
                    result,
                    Err(PictureParameterSetError::SignedValueOutOfRange {
                        field: actual,
                        ..
                    }) if actual == field
                ),
                "field={field}"
            );
        }
    }

    #[test]
    fn distinguishes_unsupported_and_malformed_slice_groups() {
        let lookup = lookup_with(make_sps(66, 1, 4, 3));

        let mut writer = BitWriter::default();
        write_pps_prefix(&mut writer, 0, 0);
        writer.ue(2);
        writer.ue(3);
        assert!(matches!(
            parse_picture_parameter_set(&writer.finish_rbsp(), &lookup),
            Err(PictureParameterSetError::ValueOutOfRange {
                field: "num_slice_groups_minus1",
                ..
            })
        ));

        let mut writer = BitWriter::default();
        write_pps_prefix(&mut writer, 0, 0);
        writer.ue(1);
        writer.ue(2);
        writer.ue(7);
        writer.ue(4);
        assert_eq!(
            parse_picture_parameter_set(&writer.finish_rbsp(), &lookup),
            Err(PictureParameterSetError::InvalidSliceGroupRectangle {
                index: 0,
                top_left: 7,
                bottom_right: 4
            })
        );

        let large_lookup = lookup_with(make_sps(
            66,
            1,
            super::MAX_EXPLICIT_SLICE_GROUP_MAP_UNITS + 1,
            1,
        ));
        let mut writer = BitWriter::default();
        write_pps_prefix(&mut writer, 0, 0);
        writer.ue(1);
        writer.ue(6);
        assert_eq!(
            parse_picture_parameter_set(&writer.finish_rbsp(), &large_lookup),
            Err(PictureParameterSetError::Unsupported {
                feature: "explicit slice-group map exceeding 1048576 map units"
            })
        );
    }

    #[test]
    fn validates_explicit_slice_group_map_size_and_ids() {
        let lookup = lookup_with(make_sps(66, 1, 2, 2));

        let mut wrong_size = BitWriter::default();
        write_pps_prefix(&mut wrong_size, 0, 0);
        wrong_size.ue(2);
        wrong_size.ue(6);
        wrong_size.ue(2);
        assert!(matches!(
            parse_picture_parameter_set(&wrong_size.finish_rbsp(), &lookup),
            Err(PictureParameterSetError::ValueOutOfRange {
                field: "pic_size_in_map_units_minus1",
                ..
            })
        ));

        let mut invalid_id = BitWriter::default();
        write_pps_prefix(&mut invalid_id, 0, 0);
        invalid_id.ue(2);
        invalid_id.ue(6);
        invalid_id.ue(3);
        invalid_id.bits(3, 2);
        assert!(matches!(
            parse_picture_parameter_set(&invalid_id.finish_rbsp(), &lookup),
            Err(PictureParameterSetError::ValueOutOfRange {
                field: "slice_group_id",
                ..
            })
        ));
    }

    #[test]
    fn rejects_bad_trailing_bits_and_every_truncation() {
        let lookup = lookup_with(make_sps(66, 1, 40, 30));
        let valid = make_minimal_pps();
        for length in 0..valid.len() {
            assert!(
                parse_picture_parameter_set(&valid[..length], &lookup).is_err(),
                "length={length}"
            );
        }
        assert!(parse_picture_parameter_set(&valid, &lookup).is_ok());

        let mut extra = valid.clone();
        extra.push(0);
        assert!(parse_picture_parameter_set(&extra, &lookup).is_err());

        let mut missing_stop = valid;
        *missing_stop.last_mut().unwrap() = 0;
        assert!(parse_picture_parameter_set(&missing_stop, &lookup).is_err());
    }

    #[test]
    fn exhaustive_short_and_structured_inputs_are_panic_free() {
        let lookup = lookup_with(make_sps(100, 3, 4, 3));
        let _ = parse_picture_parameter_set(&[], &lookup);
        for value in u8::MIN..=u8::MAX {
            let _ = parse_picture_parameter_set(&[value], &lookup);
        }
        for value in u16::MIN..=u16::MAX {
            let _ = parse_picture_parameter_set(&value.to_be_bytes(), &lookup);
        }

        const ALPHABET: [u8; 6] = [0, 1, 0x40, 0x80, 0xfe, 0xff];
        for length in 3..=6 {
            let combinations = ALPHABET.len().pow(length as u32);
            for mut value in 0..combinations {
                let mut input = vec![0; length];
                for byte in &mut input {
                    *byte = ALPHABET[value % ALPHABET.len()];
                    value /= ALPHABET.len();
                }
                let _ = parse_picture_parameter_set(&input, &lookup);
            }
        }
    }

    #[test]
    fn errors_have_stable_descriptions() {
        assert_eq!(
            PictureParameterSetError::MissingSequenceParameterSet {
                seq_parameter_set_id: 7
            }
            .to_string(),
            "PPS references missing sequence parameter set 7"
        );
        assert_eq!(
            PictureParameterSetError::Unsupported {
                feature: "test feature"
            }
            .to_string(),
            "PPS uses unsupported test feature"
        );
        assert_eq!(
            PictureParameterSetError::InvalidRbspTrailingBits.to_string(),
            "PPS has invalid rbsp_trailing_bits"
        );
    }
}
