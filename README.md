# nalcore

`nalcore` is a Rust project for software decoding of H.264/AVC and H.265/HEVC bitstreams.

## Goal

Build a clean, correct, and maintainable decoder foundation first, then optimize for performance once the decode pipeline is stable.

## Initial Scope

- Bitstream parsing and NAL unit handling
- Sequence and picture parameter set parsing
- Slice parsing and decode pipeline plumbing
- Software decode to planar frame output
- Regression tests against small reference streams

## Non-Goals for the First Pass

- Container demuxing
- Hardware acceleration
- Aggressive SIMD optimization
- Broad codec support beyond H.264 and H.265

## Working Principles

- Correctness before speed
- Small, explicit Rust abstractions
- Minimal `unsafe`
- Deterministic tests and fixtures

## Early Milestones

1. Build the bit reader and common parsing utilities.
2. Implement H.264 baseline parsing and decode flow.
3. Add H.265 parsing and decode flow.
4. Add conformance-oriented regression tests.
5. Profile and optimize the hot paths.

## Status

Project bootstrap in progress.

## Planning

See [Longer-Term Plans](docs/PLANS.md) for tentative architecture, decoding,
testing, safety, tooling, and release ideas beyond the currently tracked
GitHub issues.
