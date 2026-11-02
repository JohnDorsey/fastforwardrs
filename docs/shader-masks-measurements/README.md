# Measurements for shader masks (2026-11-01)

What was measured and with what, for [`../shader-masks-design.md`](../shader-masks-design.md). **None of this is part of the program**: it is a throwaway proof of concept, kept so that the numbers can be repeated. It does not build in this folder.

| File | What it is |
|---|---|
| `rewrite_poc.diff` | the patch to `src/isf/rewrite.rs` (as of commit `f383d15`) that lets a float input be read from a picture, **at every use** (the macro of the input becomes a read of the mask) or **once per pixel** (a global that `_isf_set_up_driven()` fills in before `main` and before the globals' initialisers) |
| `poc_tests.rs` | a test (put it in `src/isf/` and name it in `src/isf/mod.rs` under `#[cfg(test)]`) that compiles looped shaders in four ways with the real rewriter and compiler, writes their WGSL to `E:/a/poc_out/`, and times the compile. (Its strings have some raw line ends in them: the bash tool of the session that made it mangled the backslashes; the GLSL is the same.) |
| `gpu_cost.rs` | an example (put it in `examples/`) that runs that WGSL on a real graphics adapter and times it; and sends the planes of a video picture to the card and draws them into a texture |
| `results/matrix_*` | the output of `gpu_cost` for each adapter, each size, and a flat mask (the control: the same value everywhere, and the uniform given the same value) and a ramp |
| `results/engine_benchmark_*` | the output of the program's own `engine_benchmark` example (`--frames 60 --in-flight 1`, at the size in the name) |
| `results/decode_of_test_clips.txt` | the time ffmpeg takes to decode a frame of H.264 |

To repeat: `git archive HEAD` into a folder, apply `rewrite_poc.diff`, add the two files, `cargo test --lib --release poc_write_the_shaders` (it writes the WGSL), then `cargo run --release --example gpu_cost -- AMD 1920x1080 8,32,128,b32 flat` (the first words are part of the adapter's name, the size, the loops (`b32` is the shader with branches), and `flat` or `ramp`).

## How each number was made, and what was done against being fooled

* **Time of a pass**: 8 to 4096 full-screen draws in one render pass (chosen so that one batch takes about 40 ms), submitted, and waited for; the median of 9 batches, **the variants taking turns round after round** (the order a turn further each round) and `u` measured twice (`u again`: 0.96 to 1.01 of the first time, so the card did not drift).
* **Do the variants make the same picture?** Every variant's pixels were read back and compared with `u`: with the flat mask `e` and `o` differ from `u` by at most 2 of 255 (rounding) in at most 5,735 bytes of 8.3 million. (With the ramp they differ by up to 175 **by design**: the mask is not 0.5 everywhere. `e` and `o` have the same mean in every run.)
* **A result that was suspicious**: on the AMD graphics the shaders that read a mask were 16 to 25 % *faster* than the one that reads a uniform. The flat mask (so that every shader reads the same places of the picture), the turns, and the second `u` all left it as it was; the cause is **not known** (the driver's compiler, perhaps). It is reported as it was measured.
* **The adapters**: an NVIDIA GeForce RTX 3090 (Vulkan, a card of its own) and the AMD Radeon graphics of the Ryzen 5 7600X (Vulkan, integrated), which is the one the project's gate is defined on. Other drivers (Intel, Apple's, Mesa's) were not measured.
