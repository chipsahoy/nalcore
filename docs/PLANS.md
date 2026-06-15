# Longer-Term Plans

This document records tentative design ideas beyond the work currently tracked
in GitHub issues. It is context for future contributors and agents, not a
promise of scope or API stability. Update it as implementation experience
invalidates assumptions.

## Project Direction

`nalcore` should be a library-first, software-only decoder for H.264/AVC and
H.265/HEVC.

Priorities, in order:

1. Correctness
2. Clear separation between syntax parsing and decoding
3. Robust handling of malformed input
4. Testability
5. Performance

Container demuxing, playback, audio, and hardware acceleration should remain
outside the core crate.

## Proposed Architecture

Potential layers:

- `bitstream`: bit-level reading and Exp-Golomb decoding
- `nal`: Annex B framing and RBSP extraction
- `h264::syntax`: headers, parameter sets, slices, and macroblocks
- `h264::decode`: picture state and reconstruction
- `h265::syntax`: HEVC syntax parsing
- `h265::decode`: HEVC reconstruction
- `frame`: pixel formats, planes, strides, and decoded pictures
- `error`: malformed, truncated, unsupported, and resource-limit errors

Parsing should produce explicit syntax structures. Decode logic should consume
those structures rather than mixing bit reading with pixel reconstruction
throughout the implementation.

## H.264 Work After Slice Headers

Likely sequence:

1. Parameter-set storage and activation
2. Access-unit and picture-boundary detection
3. CAVLC entropy decoding
4. Macroblock-layer parsing
5. Inverse scanning and quantization
6. Integer transforms
7. Intra prediction
8. Decoded picture buffer
9. Inter prediction and motion compensation
10. Reference picture list management
11. Deblocking filter
12. Frame output
13. CABAC entropy decoding
14. Broader profile and interlace support

A practical first decode target is constrained-baseline, progressive, 8-bit,
4:2:0 video. Main and High profile features should follow after that path works
end to end.

## H.265 Strategy

H.265 should initially reuse only genuinely codec-neutral components:

- Bit reader
- Annex B framing
- RBSP extraction
- Frame storage concepts
- Common error conventions

The H.264 and H.265 syntax and decode pipelines should otherwise remain
distinct. A generalized decoder model should only be introduced where working
implementations demonstrate a useful common abstraction.

H.265 work should begin after the H.264 architecture has survived a complete
decode path:

1. HEVC NAL headers
2. VPS, SPS, and PPS
3. Slice segment headers
4. Coding tree units
5. Intra prediction
6. Transform and quantization
7. Inter prediction
8. Decoded picture buffer
9. Deblocking
10. Sample adaptive offset
11. Frame output

## Public API

A future API might resemble:

```rust
let mut decoder = h264::Decoder::new(config);

for nal in nal_units {
    for frame in decoder.push(nal)? {
        process(frame);
    }
}

for frame in decoder.flush()? {
    process(frame);
}
```

Open API questions:

- Whether input should be NAL units or arbitrary Annex B chunks
- Whether frame output should borrow decoder memory or own its planes
- How stream reconfiguration should be reported
- How timestamps and user metadata should be associated with frames
- Whether unsupported features should be recoverable errors
- How resource limits should be configured

These questions should be answered through decoder requirements and working
code rather than fixed prematurely.

## Frame Representation

The first frame format should likely support planar 8-bit YUV 4:2:0.

Later extensions may include:

- 4:2:2 and 4:4:4 chroma
- Monochrome
- 10-bit and 12-bit samples
- Cropped display views
- Color metadata
- User-provided frame allocation

RGB conversion should remain outside the decoder core.

## Error Model

Errors should distinguish:

- Truncated input
- Malformed syntax
- Unsupported valid syntax
- Missing parameter sets
- Resource limits
- Internal invariant failures

Malformed input must never cause a panic, unbounded allocation, arithmetic
overflow, or excessive loops.

## Resource Limits

Decoder configuration should eventually limit:

- Maximum coded dimensions
- Maximum frame allocation
- Maximum parameter-set count
- Maximum reference pictures
- Maximum NAL unit size
- Maximum Exp-Golomb prefix length
- Maximum recursion or syntax nesting where applicable

These limits are necessary for safely processing untrusted media.

## Testing Strategy

Several test layers will be needed:

- Focused unit tests for bitstream and syntax primitives
- Hand-constructed fixtures for boundary cases
- Golden tests using small encoded streams
- Frame hashes or raw YUV comparisons
- Differential testing against FFmpeg
- Official conformance streams where licensing permits
- Fuzzing for all untrusted-input parsers
- Regression tests for every discovered bug

Test media should be small, redistributable, and documented with its origin and
license.

## Performance Strategy

Performance work should begin only after representative streams decode
correctly.

Likely optimization areas:

- Bit reading
- Entropy decoding
- Inverse transforms
- Intra prediction
- Motion compensation
- Deblocking
- Frame allocation and reuse

Portable scalar implementations should remain available. Architecture-specific
SIMD can be added behind internal dispatch without changing the public API.

## Tooling Ideas

Useful supporting tools may include:

- `nalcore-inspect`: dump NAL and parameter-set syntax
- `nalcore-decode`: decode Annex B streams to raw YUV
- Reference comparison scripts using FFmpeg
- Criterion benchmarks for decoding primitives
- `cargo-fuzz` targets for framing and syntax parsers

These tools could live in a workspace while the decoder remains a reusable
library crate.

## Documentation

The repository should eventually document:

- Supported codecs, profiles, levels, and pixel formats
- Unsupported features
- Safety and resource-limit behavior
- Decoder state model
- Public API examples
- Conformance status
- Benchmark methodology
- Contribution and test-fixture rules

## Release Approach

Suggested phases:

- `0.1`: parsing and inspection APIs
- `0.2`: limited H.264 constrained-baseline decoding
- `0.3`: broader H.264 support
- `0.4`: stable H.264 decoding API
- `0.5`: initial H.265 parsing
- Later releases: H.265 decode coverage and optimization

No API stability should be promised until at least one complete codec path has
been exercised by real users.
