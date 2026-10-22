# Shaders that can be bundled (ISF)

The record of `bundled-plugins/shaders.json`, made by `tools/bundled_plugins/make_the_shader_manifest.py` from **Vidvox/ISF-Files at commit `395072d48b3ce7351ccb20a5fda54470591324df`**. The decisions are made
by the rules in that file (LG-02 of `plugin-system-features.md`: the permissive licences, and GPL and LGPL that are compatible with GPLv3), from **what each file says of itself**, and then by running this build of the program on the
files that may be bundled (`engine::isf_corpus_tests`: read, compile, install and draw each), so that **a shader in the manifest is one that runs**. Nothing that was read is ever run by the tool.

## What came of it

* **In the manifest: 157 shaders** (132 filters, 25 generators).
* Left out: **170** of the 327 shaders of the repository: 47 that may not be bundled, and 123 that may be and that this build does not run yet.

### Why the ones that may not be bundled are not

* 42: names a source of code that says no licence
* 4: brings pictures of its own, whose licence is not said
* 1: credited to a person whose shaders are Creative Commons non-commercial

### What stops this build from running the others

* 74: structure: this shader reads a picture called X, which this version cannot give it yet
* 20: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet
* 8: structure: the shader reads N pictures, and N are the most
* 6: compile: Expected RightParen, found LeftParen
* 3: compile: Unknown variable: fftImage
* 2: compile: Expected identifier, found LeftParen
* 1: compile: Expected identifier, found LeftParen; Expected RightParen, found TypeName(Type { name: None, inner: V
* 1: compile: HirExpr { kind: Call(FunctionCall { kind: TypeConstructor([N]), args: [[N]] }), meta: Span { start: N
* 1: compile: Unknown variable: _inputImage_imgRect
* 1: compile: Unknown variable: _maskImage_imgRect
* 1: compile: Unknown variable: audio
* 1: compile: Unknown variable: audioFlow
* 1: compile: the shader is not valid: Handle N of naga::ir::LocalVariable is either not present, or inaccessible y
* 1: read: the input size: MIN (N) is more than MAX (N.N)
* 1: structure: the WIDTH of pass number N: $buffQuality is not something that a size can use ($WIDTH and $HEIGHT a
* 1: structure: the WIDTH of pass number N: $trailCount is not something that a size can use ($WIDTH and $HEIGHT ar

## Every shader that was left out

| file | why |
|---|---|
| Butterfly Wave Scrawler.fs | may be bundled, and this build does not run it: compile: Expected RightParen, found LeftParen |
| Cubic Warp.fs | may be bundled, and this build does not run it: compile: Expected RightParen, found LeftParen |
| Dreamy.fs | may be bundled, and this build does not run it: compile: Expected RightParen, found LeftParen |
| Grid Warp.fs | may be bundled, and this build does not run it: compile: Expected RightParen, found LeftParen |
| Shape Mask.fs | may be bundled, and this build does not run it: compile: Expected RightParen, found LeftParen |
| cube.fs | may be bundled, and this build does not run it: compile: Expected RightParen, found LeftParen |
| RGB Invert.fs | may be bundled, and this build does not run it: compile: Expected identifier, found LeftParen |
| RGB Strobe.fs | may be bundled, and this build does not run it: compile: Expected identifier, found LeftParen |
| Radial Gradient.fs | may be bundled, and this build does not run it: compile: Expected identifier, found LeftParen; Expected RightParen, found TypeName(Type { name: None, inner: Vector { size: Bi, scalar: Scalar { kind: Float, width: 4 } } }) |
| CrossZoom.fs | may be bundled, and this build does not run it: compile: HirExpr { kind: Call(FunctionCall { kind: TypeConstructor([7]), args: [[3]] }), meta: Span { start: 326, end: 4834 } } cannot be in the left hand side |
| FFT Color Lines.fs | may be bundled, and this build does not run it: compile: line 115, column 27: Unknown variable: fftImage |
| Test Pattern Generator.fs | may be bundled, and this build does not run it: compile: line 156, column 32: Unknown variable: audioFlow |
| Waveform Displace.fs | may be bundled, and this build does not run it: compile: line 66, column 34: Unknown variable: audio |
| Radial Replicate.fs | may be bundled, and this build does not run it: compile: line 69, column 14: Unknown variable: _inputImage_imgRect |
| FFT Filled Waveform.fs | may be bundled, and this build does not run it: compile: line 83, column 28: Unknown variable: fftImage |
| Trail Mask.fs | may be bundled, and this build does not run it: compile: line 83, column 71: Unknown variable: _maskImage_imgRect |
| FFT Spectrogram.fs | may be bundled, and this build does not run it: compile: line 99, column 36: Unknown variable: fftImage |
| Pixelize.fs | may be bundled, and this build does not run it: compile: the shader is not valid: Handle 0 of naga::ir::LocalVariable is either not present, or inaccessible yet |
| Radial Spectrogram.fs | may be bundled, and this build does not run it: read: line 18, column 21: the input size: MIN (1) is more than MAX (0.05) |
| Circle Trails.fs | may be bundled, and this build does not run it: structure: the WIDTH of pass number 1: $trailCount is not something that a size can use ($WIDTH and $HEIGHT are) (at character 1 of the expression) |
| Key Frame Artifacts.fs | may be bundled, and this build does not run it: structure: the WIDTH of pass number 2: $buffQuality is not something that a size can use ($WIDTH and $HEIGHT are) (at character 22 of the expression) |
| Chroma Desaturation Mask.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Chroma Mask.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Circular Feedback Mask.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Color Blowout.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Color Replacement.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Crop and Feather.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Doodler.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Etch-a-Sketch.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| FastMosh.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Flipbook.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Freeze Frame.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Interlace.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Life.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Pattern Glitch.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Random Freeze.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Random Shape Blast.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Slit Scan.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Sorting Smear.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Video Snake.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Zooming Feedback.fs | may be bundled, and this build does not run it: structure: the last pass draws into a buffer and not into the picture the effect makes, which is not run yet |
| Micro Buffer.fs | may be bundled, and this build does not run it: structure: the shader reads 10 pictures, and 7 are the most |
| Time Glitch RGB.fs | may be bundled, and this build does not run it: structure: the shader reads 10 pictures, and 7 are the most |
| Bloom.fs | may be bundled, and this build does not run it: structure: the shader reads 8 pictures, and 7 are the most |
| Edge Blur.fs | may be bundled, and this build does not run it: structure: the shader reads 8 pictures, and 7 are the most |
| Fast Blur.fs | may be bundled, and this build does not run it: structure: the shader reads 8 pictures, and 7 are the most |
| Gloom.fs | may be bundled, and this build does not run it: structure: the shader reads 8 pictures, and 7 are the most |
| City Lights.fs | may be bundled, and this build does not run it: structure: the shader reads 9 pictures, and 7 are the most |
| MultiFrame 3x3.fs | may be bundled, and this build does not run it: structure: the shader reads 9 pictures, and 7 are the most |
| Glitch Shifter.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called altImage, which this version cannot give it yet |
| Displace.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called displaceImage, which this version cannot give it yet |
| Histogram Viewer.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called histogramImage, which this version cannot give it yet |
| Micro Buffer RGB.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called inputDelayImage, which this version cannot give it yet |
| v002 Light Leak.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called leakImage, which this version cannot give it yet |
| Color Relookup.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called lookupImage, which this version cannot give it yet |
| Multi Gradient.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called lookupImage, which this version cannot give it yet |
| RE RGB Gradient Generator.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called lookupImage, which this version cannot give it yet |
| Layer Mask.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called maskImage, which this version cannot give it yet |
| Auto Colors Histogram.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called sourceHistogram, which this version cannot give it yet |
| Duotone From Histogram.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called sourceHistogram, which this version cannot give it yet |
| Angular.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Bounce.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Bow Tie Horizontal.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Bow Tie Vertical.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Burn.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Circle Crop.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Circle Open.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Circle.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Color Phase.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Colour Distance.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Crazy Parametric Fun.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Crosshatch.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Crosswarp.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Directional Warp.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Directional Wipe.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Directional.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Displacement.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Doom Screen Transition.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Doorway.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Dreamy Zoom.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Fade Color.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Fade Gray Scale.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Fade.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Film Burn.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Fly Eye.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Glitch Displace.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Glitch Memories.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Grid Flip.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Heart Transition.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Hexagonalize.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Inverted Page Curl.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Kaleidoscope Transition.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Linear Blur.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Luma Transition.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Luminance Melt.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Morph.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Mosaic.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Multiply Blend.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Perlin Transition.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Pinwheel.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Polar Function.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Polka Dots Curtain.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Radial.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Random Squares.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Ripple Transition.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Rotate Scale Fade.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Simple Zoom Transition.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Squares Wire.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Squeeze.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Stereo Viewer.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Swap Transition.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Swirl.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| TV Static.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Undulating Burn Out.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Water Drop.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Wind.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Window Blinds.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Window Slice.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Wipe Down.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Wipe Left.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Wipe Right.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Wipe Up.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Zoom In Circles.fs | may be bundled, and this build does not run it: structure: this shader reads a picture called startImage, which this version cannot give it yet |
| Cursor Overlay.fs | not bundled: brings pictures of its own, whose licence is not said |
| Cursor.fs | not bundled: brings pictures of its own, whose licence is not said |
| Night Vision.fs | not bundled: brings pictures of its own, whose licence is not said |
| v002-CRT-Mask.fs | not bundled: brings pictures of its own, whose licence is not said |
| Boxinator.fs | not bundled: credited to a person whose shaders are Creative Commons non-commercial |
| Star.fs | not bundled: names a source of code that says no licence -- //	Adapted from https://glsl.io/transition/d1f891c5585fc40b55ea |
| Heart.fs | not bundled: names a source of code that says no licence -- //	Adapted from https://glsl.io/transition/d71472a550601b96d69d |
| Sepia Tone.fs | not bundled: names a source of code that says no licence -- //	Adapted from https://www.omniref.com/ruby/gems/essytas/0.0.1/files/lib/glsl/sepia.frag |
| VHS Glitch.fs | not bundled: names a source of code that says no licence -- //	Based on https://github.com/staffantan/unity-vhsglitch |
| v002 Bleach Bypass.fs | not bundled: names a source of code that says no licence -- //	Based on v002 bleach bypass – https://github.com/v002/v002-Film-Effects/ |
| v002 Technicolor.fs | not bundled: names a source of code that says no licence -- //	Based on v002 technicolor – https://github.com/v002/v002-Film-Effects/ |
| v002 Vignette.fs | not bundled: names a source of code that says no licence -- //	Based on v002 vignette – https://github.com/v002/v002-Film-Effects/ |
| Color Organ Polyphonic.fs | not bundled: names a source of code that says no licence -- //	Color Scales via http://rhythmiclight.com/archives/ideas/colorscales.html |
| Color Scales.fs | not bundled: names a source of code that says no licence -- //	Color Scales via http://rhythmiclight.com/archives/ideas/colorscales.html |
| Smoke Screen.fs | not bundled: names a source of code that says no licence -- //	Converted from https://www.shadertoy.com/view/4t2SRz by jackdavenport |
| Comet Tails.fs | not bundled: names a source of code that says no licence -- //	From https://bavc.github.io/avaa/artifacts/image_lag.html |
| Tiny Date Time Overlay.fs | not bundled: names a source of code that says no licence -- //	Inspired / adapted from Tim Gfrerer's Texture-less Text Rendering blog post: |
| Random Characters.fs | not bundled: names a source of code that says no licence -- //	Inspired / adapted from Tim Gfrerer's brilliant Texture-less Text Rendering blog post: |
| Saturation Bleed.fs | not bundled: names a source of code that says no licence -- //	Inspired by Oversaturation glitches |
| Digital Clock.fs | not bundled: names a source of code that says no licence -- //	a simplfied version of the number drawing from http://www.interactiveshaderformat.com/sketches/12 |
| Median.fs | not bundled: names a source of code that says no licence -- //	adapted from |
| Truchet Tile.fs | not bundled: names a source of code that says no licence -- //	adapted from https://github.com/pjkarlik/TruchetTiles/blob/master/src/shader/truchet/fragmentShad |
| Convergence.fs | not bundled: names a source of code that says no licence -- //	adapted from maxilla inc's https://github.com/maxillacult/ofxPostGlitch/ |
| Lens Flare.fs | not bundled: names a source of code that says no licence -- //	as a guide, http://john-chapman-graphics.blogspot.co.uk/2013/02/pseudo-lens-flare.html |
| Dither-Bayer.fs | not bundled: names a source of code that says no licence -- //	based on https://github.com/hughsk/glsl-dither |
| Line Group.fs | not bundled: names a source of code that says no licence -- //	based on https://github.com/hughsk/glsl-dither |
| Stylize Glitch.fs | not bundled: names a source of code that says no licence -- //	based on https://github.com/hughsk/glsl-dither |
| Triangle.fs | not bundled: names a source of code that says no licence -- //	functions via http://stackoverflow.com/questions/2049582/how-to-determine-a-point-in-a-triangle |
| Audio Waveform Shape.fs | not bundled: names a source of code that says no licence -- //	https://github.com/patriciogonzalezvivo/PixelSpiritDeck/tree/master/lib |
| Shape Morph Feedback Mask.fs | not bundled: names a source of code that says no licence -- //	https://github.com/patriciogonzalezvivo/PixelSpiritDeck/tree/master/lib |
| Shape Morph Wrap.fs | not bundled: names a source of code that says no licence -- //	https://github.com/patriciogonzalezvivo/PixelSpiritDeck/tree/master/lib |
| v002 Glitch Analog.fs | not bundled: names a source of code that says no licence -- //	https://github.com/v002/v002-Glitch/blob/master/v002AnalogGlitchPlugIn.m |
| Optical Flow Distort.fs | not bundled: names a source of code that says no licence -- //	https://github.com/v002/v002-Optical-Flow |
| Optical Flow Generator.fs | not bundled: names a source of code that says no licence -- //	https://github.com/v002/v002-Optical-Flow |
| Y-C Time Blur.fs | not bundled: names a source of code that says no licence -- //	inspired by (but not exactly the same thing as) y/c delay error |
| Glow-Fast.fs | not bundled: names a source of code that says no licence -- //	original blur implementation as v002.blur in QC by anton marini and tom butterworth, ported by zo |
| Glow.fs | not bundled: names a source of code that says no licence -- //	original blur implementation as v002.blur in QC by anton marini and tom butterworth, ported by zo |
| Thermal Camera.fs | not bundled: names a source of code that says no licence -- //	partly adapted from http://coding-experiments.blogspot.com/2010/10/thermal-vision-pixel-shader.ht |
| Trio Tone.fs | not bundled: names a source of code that says no licence -- //	partly adapted from http://coding-experiments.blogspot.com/2010/10/thermal-vision-pixel-shader.ht |
| Toon.fs | not bundled: names a source of code that says no licence -- //	with help from https://github.com/neilmendoza/ofxPostProcessing/blob/master/src/ToonPass.cpp |
| Brick Pattern.fs | not bundled: names a source of code that says no licence -- // Based on Brick example from https://thebookofshaders.com/09/ |
| Video Bricks.fs | not bundled: names a source of code that says no licence -- // Based on Brick example from https://thebookofshaders.com/09/ |
| v002 Crosshatch.fs | not bundled: names a source of code that says no licence -- //http://learningwebgl.com/blog/?p=2858 |
| Dual Side Scroller And Flip.fs | not bundled: names a source of code that says no licence -- Inspired by Side Scroller and Flip by BrianChasalow |
| Frosted Glass.fs | not bundled: names a source of code that says no licence -- adapted from http://www.geeks3d.com/20101228/shader-library-frosted-glass-post-processing-shader-gls |
| ASCII Art.fs | not bundled: names a source of code that says no licence -- by VIDVOX (Ported from https://www.shadertoy.com/view/lssGDj) |
| Multi Pass Gaussian Blur.fs | not bundled: names a source of code that says no licence -- original implementation as v002.blur in QC by anton marini and tom butterworth, ported by zoidberg |
