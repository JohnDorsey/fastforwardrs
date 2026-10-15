# Phase 0.5 spikes (throwaway code, kept so that the findings can be repeated)

The findings are in [`docs/plugin-system-spike-findings.md`](../../../docs/plugin-system-spike-findings.md). This folder is **not part of the program**: it has a Cargo
project of its own (outside the workspace, so `cargo build` and `cargo test` in the project's root never see it), it is formatted with the project's rustfmt settings but never run through clippy,
it has no tests, and nothing in the program may depend on it. It was written on 2026-10-15 against the versions the program uses (wgpu 30.0.1, naga 30.0.1).

**Safety rule: nothing here opens a real graphics card.** `gpu::software_gpu()` picks the adapter whose device type is `Cpu` (on Windows the "Microsoft Basic Render Driver",
which is WARP; on Linux lavapipe) and panics if there is none. The one exception is `race_repro`, which can be pointed at another adapter with `SPIKE_REPRO_ADAPTER=Dx12:AMD`
because it only draws two small triangles. The spike with shaders that do not finish (`s2_hang`) must be run through `run_s2.ps1` / `run_s2_linux.sh`, which kill the process
by its id after a hard limit.

| Program | What it does | Finding |
|---|---|---|
| `adapters` | Lists the adapters wgpu sees; opens none | |
| `glsl_probe` | 39 small GLSL idioms through naga's GLSL front end (no GPU) | S1 |
| `s1_isf` | 8 ISF-style shaders (`shaders/`) through the rewriting step (`src/isf.rs`) and naga to pictures, with checks of values | S1 |
| `s1_errors` | Where and how well compile errors are reported | S1 |
| `race_repro` | Two draws with separate uniform buffers: right or wrong? (WARP gets it wrong) | S1, the wgpu/WARP finding |
| `s2_hang` | A shader that does not finish: watchdog, device loss, recovery, CPU used afterwards | S2 |
| `s2_guard` | The loop cap of `src/guard.rs` on shaders that do not finish and on ones that do | S2 |
| `s3_lut` | A `.cube` table in a shader (tetrahedral, trilinear) against a CPU reference and against ffmpeg's `lut3d` | S3 |

Run on Windows (from this folder; `cargo` is on the path after `$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"`):

```
cargo build --bins
target\debug\s1_isf.exe out_s1 shaders
powershell -File run_s2.ps1 infinite_loop 5
powershell -File run_guard.ps1 infinite_while 20000
target\debug\s3_lut.exe out_s3          # needs ffmpeg on the path
```

On Linux the folder is copied to `~/spikes` in WSL and `run_all_linux.sh`, `run_s2_linux.sh` and `run_guard_linux.sh` are run there (they set `VK_DRIVER_FILES` to the
lavapipe driver only, because in WSL there is also a driver for the real graphics card).
