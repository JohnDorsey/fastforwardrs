<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# The shaders that come with the program

Written 2026-11-03. ISF shaders that were written for the program, kept **inside it**: nothing is downloaded, and nothing can be lost or changed on the computer. They are the first
content that ships in the box (`distribution-notices.md`), made on the maintainer's word ("make some customizable fractal generators first; next come other trippy effects").

## Where they are and how they are used

* The files are in `assets/shaders/<folder>/<Name>.fs` (`fractals`, and `trippy`), and are put in the program with `include_str!` by the list in
  `src/shader_files/shaders_of_the_program.rs`. A test says that the list and the folder are the same files. `.gitattributes` gives the files the line ends of Unix on every system (a Windows checkout turns text into CRLF otherwise), because the key of a shader is the digest of its text, and has to be the same on all of them.
* The editor of a person installs them with the shaders it finds in folders (`find_plugins_the_way_a_person_uses_the_editor`): each is read and compiled like any shader, and is listed in
  the plugin manager under *Shaders that can be used* (its hover text says **built into the program**). The generators are in *New > Generator* on an empty part of a track, and the filters
  in the **+** menu under *ISF*, by their category. A shader that is also in a folder is one kind of effect, and the one that comes with the program is the one that counts. The tests leave them out
  (`install_the_shaders_of_the_program` is what asks for them), so that what a test finds in a folder is all that it finds.
* **A project that uses one has its text in the project file**, as for any shader: it runs on a computer that has another version of the program. A shader that is changed in a later version is another
  kind of effect (its key has the digest of its text), and the projects that used the old one go on running it.
* **Licence: CC0-1.0**, as for the five look-up tables: made for the program, nothing of anyone else's (they were written from the mathematics, not from another person's code), with two lines of SPDX at the top
  of each file. The maintainer can change this: it is those two lines (and the same two lines of `assets/shaders/shared`).
* Every input of every shader is a value like any other: a slider, the stopwatch and keyframes (a switch and a choice have no keyframes: they hold until they are changed). **Nothing moves by itself.** There is no input that
  sets a *rate* beside one that sets the thing that would change (no spin beside a rotation, no wobble beside the value that wobbles, no cycling of the colours beside the shift of them, no orbit beside a camera angle): a person can
  give that slider keyframes, which is more control than a rate is. Where there was no slider for what a rate moved, the input is that itself: *Depth* of the Droste spiral, *Flight* down the tunnel, *Phase* of the plasma, the
  quasicrystal and the ripples, *Flow* of the liquids, *Slide angle* of the hyperbolic tiling, *Along path* of Julia Dance (where the point is on its path), *Shift x* and *y* of the kaleidoscope. The only rates left are those of
  Video Feedback (the growth, turn, slide and colour turn of its echo each second), which build on the frame before and have nothing else that they could be. Every one has labels that say what each input does, and a way to
  colour it (eight palettes, and one that is made of two colours that a person picks).

## The fractal generators

| Shader | What it makes | What can be set (besides the colours) |
|---|---|---|
| **Mandelbrot Explorer** | the Mandelbrot set and its relatives, or the Julia set of a point (it begins on the whole set and, as the zoom goes in, glides to a branching point of the edge) | six folds of the plane (Mandelbrot, Burning Ship, Tricorn, Celtic, two more), any power from 2, the zoom, the point zoomed in on, rotation, iterations (and more for each power of two of zoom), the Julia point, six ways of colouring (smooth count, three orbit traps, distance to the set, count and angle), the glow along the edge, smoothing of the edges |
| **Julia Dance** | a Julia set for a point on a path: the edge of the main cardioid (or the disc beside it, or a circle) | how far out the path is, where along it the point is (*Along path*: the set grows branches and falls to dust as the point goes round), nudges, power, zoom, centre, rotation, iterations, four ways of colouring the outside and three of colouring the inside |
| **Newton Basins** | the basins of Newton's method for z to the power n less one: the root that each point walks to, and how long the walk took | 3 to 8 roots, the relaxation, a point added at each step, zoom, centre, rotation, steps of the walk, how much long walks are darkened, the colour of the points that never arrive |
| **Apollonian Gasket** | circles inside circles without end (the plane is folded into a tile and turned inside out again and again) | the strength of the turning inside out, folds, zoom, centre, rotation, three ways of colouring, dark lines along the circles |
| **Mandelbulb** | the Mandelbrot set in three dimensions, by ray marching | the power, iterations, a cut that opens it, the camera (angle round, height, distance, field of view), steps, coarseness of the detail, the light, glow, fog, the two colours of the sky |
| **Folded Sponge** | the Menger sponge or the Sierpinski tetrahedron, made by folding space | the shape, folds, a twist between the folds (it makes bending, spiralling relatives), and the camera, light, fog and glow of the Mandelbulb |
| **Mandelbox** | the Mandelbox: space folded back into a box and then into a ball | the scale, folds, the size of the box and of the ball, and the camera, light, fog and glow; the camera can be put inside the box |

**Measured** (2026-11-04, at 1920x1080, with the values that each begins as (nothing moves by itself, so every time is the same), the mean of six frames, `examples/shader_pictures.rs --measure 6`, DX12):

| | RTX 3090 | AMD Radeon (the graphics of the processor) |
|---|---|---|
| Apollonian Gasket | 3.1 ms | 21 ms |
| Folded Sponge | 3.2 ms | 39 ms |
| Mandelbulb | 3.6 ms | 61 ms |
| Mandelbrot Explorer | 4.2 ms | 94 ms |
| Newton Basins | 4.4 ms | 157 ms |
| Julia Dance | 4.8 ms | 184 ms |
| Mandelbox | 6.2 ms | 241 ms |

The 2D ones are drawn with 4 samples to a pixel to begin with (a choice of *Smoothing of the edges*) and the 3D ones are heavy by nature (*Steps of a ray*, *Coarseness of the detail* and the number of folds or iterations
make them cheaper). The program's preview makes the picture smaller on a computer that cannot keep up. **Limits:** the shaders compute in single precision, so a zoom deeper than about 14 powers of two turns the
Mandelbrot set into blocks (the description of the shader says so).

## The other trippy shaders

Four **generators** (they read no picture, and are made into clips with *New > Generator*) and nine **filters** (they work on the picture of the clip, track or group that they are on; every one has *Amount*, which is 0 for the picture as it is, so that the effect can be faded in with the stopwatch).

| Shader | What it does | What can be set |
|---|---|---|
| **Plasma Storm** (generator) | a plasma: waves added to waves, bent by waves, a storm of colour | the size of the waves, layers of bending and how much each bends, a swirl, the phase (how far the storm has got), the palette |
| **Quasicrystal** (generator) | waves in 3 to 12 directions added together: patterns with a five-fold or seven-fold symmetry that never repeat | the number of directions, the size and the phase of the waves, rotation, how sharp the pattern is |
| **Liquid Marble** (generator) | ink in water, or the veins of liquid marble: noise read at places that noise pushes, twice | the size of the swirls, the flow, how much the noise is pushed by noise, fine swirls on big ones, veins (how many, how thin, how bright) |
| **Hyperbolic Tiling** (generator) | a tiling of the hyperbolic plane in a disc (ten tilings to choose from, such as five-sided tiles with four at a corner), which can be slid about inside itself so that the tiles stream past and shrink into the rim | the tiling, how far the disc slides and which way, the size of the disc, rotation, the folds that are made, the lines between the tiles |
| **Kaleidoscope Mandala** | the picture seen through a kaleidoscope: wedges that are mirrors of one another, mirrors within mirrors, and a picture inside that can be turned and shifted | the number of wedges, mirrors within mirrors, the centre, the turn of the wedges and of the picture inside, zoom, a shift of the picture inside, a turn of the colours in alternate wedges |
| **Droste Spiral** | the picture inside the picture without end, along a spiral if you like | how many times smaller each copy is, the arms of the spiral (0 is a straight tunnel), how deep it goes in, how much of the picture each copy shows, how much deeper copies are darkened |
| **Tunnel Warp** | the picture on the walls of a tunnel that is flown down, twisting | the flight (how far down the tunnel you are), the twist, how many copies there are round the tunnel, how long it looks, how dark the far end is |
| **Liquid Warp** | the picture seen through liquid, with the colours pulled apart along the flow | how far it is pushed, the size of the ripples, the flow (how far the liquid has flowed), fine ripples, how much the noise is pushed by noise |
| **Prism Split** | red, green and blue pulled apart (one way, away from a point, or round it), and copies of the picture ringing the middle, each in a colour of its own | how far, the direction and the angle, how many copies, how far and how strong, how far round the ring they are |
| **Acid Palette** | the picture repainted in a palette by how light each place is, with waves of colour through it | the palette, how many times it goes round, a shift of the colours, waves and a shift of them, solarize, posterize, how much of the real colour is kept |
| **Neon Edges** | the edges drawn in glowing colours, as neon tubes, on a dimmed picture | the width of the lines, their brightness, how strong an edge has to be, what colours them (the way the edge faces, its strength, or where it is), the palette and a shift of it |
| **Video Feedback** | what a camera pointed at its own screen makes: every frame is the frame before it, a little larger, turned, shifted, turned in colour and faded, with the live picture let in (**it remembers the frames before it**: the preview is exact when it is played from the start, and the export always is; the box says *Depends on earlier frames*) | the growth, turn, slide and colour turn of the echo each second, how much of it is kept, how much of the live picture is let in and how |
| **Ripple Rings** | rings that run out from one place, two or three, as if drops fell on water that the picture is under: it bends and shines where the waves are | the height and length of the waves, their phase (how far the rings have run out), how fast they fade, the places they run out from (and how far round the ring they are), the shine |

**Noise is made by whole-number arithmetic.** The first noise (a hash of the corner of each cell by `fract(sin(...) * 43758.5453)`, as nearly every shader that is passed about has it) drew **five different pictures on the five
adapters of this computer** (the RTX 3090 on Vulkan had blocks in it; the Radeon was smooth): a card's `sin` of a large number is not the same on every make of card. The block `noise` has a hash of 32-bit whole numbers
instead, and the same shader drew **the same picture on all five adapters, the software one too, to within eight levels at every pixel** (checked by drawing it on each and comparing).

## How they are made, looked at and checked

* **Shared code.** An ISF file has no way to include another, so the code that several need (`palettes`, `complex`, `raymarch`, `noise`, `picture`) is **copied** into each, between `// --- shared: NAME (begin ...) ---` and
  `(end)` lines, from the one true text in `assets/shaders/shared/NAME.glsl`: `py tools/shaders/sync_shared_blocks.py` puts it in every copy (`--check` only looks), and a test (`the_copies_of_a_shared_block_are_all_the_true_text_of_it`)
  says the same, so that a fix of a palette cannot be made in one file only. Another test says that **no input has the name of a word in a block it is in**: the name of an input is a macro in the shader, and it is put
  in wherever the word is, so a parameter of a function of the block called `reach`, in a shader with an input `reach`, becomes `vec3 _isf_u[1].y` and the compiler says only that it expected a parenthesis.
* **Looking.** `cargo run --release --example shader_pictures -- --out <folder> --adapter 2 --size 1280x720 --times 0,3,6 --values "palette=2;palette=5" <shader or folder> ...` writes a PNG of each shader at each time
  with each set of values, and `sheet.png` with all of them; `--measure N` says how long a frame takes on that adapter, `--wgsl` writes the WGSL that the graphics library is given, `--list` says which adapters there are.
  It is `shader_files::pictures` (nine tests of its own). **Look at the pictures on a real adapter**, and then on `--software` (see below).
* **In the editor** (`application/fast_forward_application/shaders_of_the_program_tests.rs`): that they are installed when the editor is told to and not otherwise, that the editor of a person does tell it to, that the plugin
  manager counts them, that a generator is made into a clip with its text in the project, and that **the Mandelbrot explorer put on a track is drawn by the picture engine in the preview of a window** (a window with the engine, on the
  software graphics adapter: the test looks at the frame that the preview shows, which has many colours in it, and a generator clip that makes no layer fails it), and that **nothing in the box of any shader is under the buttons of its
  row or makes the box wider than the panel** (`nothing_in_the_box_of_a_shader_of_the_program_is_under_the_buttons_of_its_row_or_makes_the_box_wider_than_the_panel`: it puts each shader on a clip, as it begins and then with the
  longest words of every choice chosen, and measures the controls that are drawn; a number may be 8 points under the buttons, which is the margin of its own box). The window of the screenshot tests has no engine, so the
  preview in the picture of the box of a generator that it makes is black. **Not tried: the window of a person, on a real graphics card, with a real mouse.**
* **The boxes fit the panel.** The properties panel is 340 points wide, and what whoever writes a shader writes for it (the labels of its inputs, and the words of its choices) has to fit in it. **A number with a long label has its box
  under the buttons that come out of the end of its row while the pointer is on it** (the one that reads the number from a mask, and those of its keyframes), which hide the number while its slider is dragged; and **a choice with
  long words makes the whole box wider than the panel**, so that all of its rows go on past the edge, where they are cut off. Both were found by looking at pictures of the box of the Mandelbrot explorer
  (`write_screenshots_of_a_generator_that_comes_with_the_program` in `tests/application_screenshots.rs`, run by hand: its labels were of up to 31 characters, and one of its words had 43), and then by a test that looks at
  every box as it is drawn (see *In the editor*), which found more: the box of Julia Dance **began 18 points wider than the panel**, which no picture of the first generator had shown. A label of more than about ten or eleven letters
  is too long for a number (it depends on the letters: *Rotation (°)* fits, *Colour bands* did not), and a label and the words of a choice together should be no more than about 35 characters. The labels and words are short now
  (`°` for degrees, `x` and `y` for a place), and what they cannot say is said by the description of the shader (the zoom is counted
  in octaves, which are doublings; the echo of Video Feedback is for each second). (A panel that wraps a long label was tried and **changed the program's own boxes**: the text field of a text object moved under its label, and 33 of the 120 other pictures that the screenshot
  tests draw were different when they were drawn before and after and compared; it was taken out again.)
* **Tests of every shader** (`shader_files/shaders_of_the_program/tests.rs`): that the folder and the list are the same files; that each begins with the two lines of SPDX; that each reads, compiles, can be used, has a
  description, a category, a label on every input, a range that holds its first value and at most 60 numbers of inputs; that the fractals are generators; that each draws a picture with colours in it at two times; and that
  **every input of every shader changes the picture** in at least one way of setting the shader up (the test has a list of those ways for each shader: the Julia point is only used while Julia is on, an orbit trap only
  while a trap colours it). That test found two real mistakes while the shaders were written (a limit of a zoom that was never reached in the time of the picture, and the lowest number of iterations of the bulb, which was
  already fully detailed).

## What was found about the compiler, and about the software adapter (so that the next shader does not have to find it)

* The GLSL is read by naga's front end. Found by a probe that drew a green cell for each thing that works: float `mod` of a negative number, integer `%`, `break` and `continue`, `out` parameters, swizzle assignment,
  `mat2`, constant arrays with a dynamic index, `switch`, function-like macros, structs, early `return`, `tanh`, `long` as an `int` and `bool` as a `bool` all work. **`isnan` is refused** by the WGSL writer (write `x != x`), and
  `sqrt(-1.0)` with a constant is a compile error. A variable that is named like a built-in function (`round`, `step`) compiles and works, but the
  shaders do not name variables so (a `round` loop variable was suspected, wrongly, of making an input do nothing).
* **The software adapter of Windows (WARP) is not the real thing** and is what the tests (and the CI of Windows) draw with: it loses its device on a shader that a real card draws without trouble. Two kinds were found:
  a function with many early `return`s (the palettes) **called inside the body of a loop that leaves by `break`** (the colouring is done after the loop now), and heavy per-sample code in **two nested loops**
  (one flat loop over the samples now). It also gives a **wrong value at some pixels** that leave a loop earlier than the pixel beside them (a derivative that was 16 times too big at 925 pixels of 78 rows, one pixel
  wide, which showed as faint pale lines; the four real adapters, and a replica on the processor in single and double precision, had none). So a picture that is looked at is drawn on a real card, a shader that is
  new is also drawn on `--software` before it is put in the list, and the tests of the shaders ask only for what a loop that leaves early cannot spoil.
* **`sin` of a large number differs between makes of card** (see *Noise* above): a hash that is made with it draws a different picture on each. Use whole-number arithmetic for anything that must be the same everywhere.
