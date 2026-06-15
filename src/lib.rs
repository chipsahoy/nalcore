//! Software decoding for H.264/AVC and H.265/HEVC.

pub mod bitstream;
pub mod h264;
pub mod h265;
pub mod nal;

#[cfg(test)]
mod tests {
    #[test]
    fn crate_bootstraps() {
        assert_eq!(env!("CARGO_PKG_NAME"), "nalcore");
    }
}
