// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "The basins of Newton's method for z to the power n, less one: every point of the picture is walked towards one of the roots, and is coloured by the root it reaches and how long the walk took. A relaxation that wobbles with the time, and a point that is added at each step, turn the basins into a slow flow of shapes.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Fractal"],
	"INPUTS": [
		{ "NAME": "roots", "LABEL": "Roots", "TYPE": "long", "VALUES": [3, 4, 5, 6, 7, 8], "LABELS": ["3", "4", "5", "6", "7", "8"], "DEFAULT": 5 },
		{ "NAME": "relaxation", "LABEL": "Relaxation (1 = Newton)", "TYPE": "float", "MIN": 0.2, "MAX": 2.2, "DEFAULT": 1.0 },
		{ "NAME": "wobble", "LABEL": "How far the relaxation wobbles", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.3 },
		{ "NAME": "wobbleSpeed", "LABEL": "Wobble speed (turns a second)", "TYPE": "float", "MIN": 0.0, "MAX": 2.0, "DEFAULT": 0.12 },
		{ "NAME": "addX", "LABEL": "Point added at each step, across", "TYPE": "float", "MIN": -1.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "addY", "LABEL": "Point added at each step, up", "TYPE": "float", "MIN": -1.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "zoom", "LABEL": "Zoom (powers of two)", "TYPE": "float", "MIN": -2.0, "MAX": 10.0, "DEFAULT": 0.0 },
		{ "NAME": "centerX", "LABEL": "Centre across", "TYPE": "float", "MIN": -3.0, "MAX": 3.0, "DEFAULT": 0.0 },
		{ "NAME": "centerY", "LABEL": "Centre up", "TYPE": "float", "MIN": -3.0, "MAX": 3.0, "DEFAULT": 0.0 },
		{ "NAME": "rotation", "LABEL": "Rotation (degrees)", "TYPE": "float", "MIN": -180.0, "MAX": 180.0, "DEFAULT": 0.0 },
		{ "NAME": "spin", "LABEL": "Spin (degrees a second)", "TYPE": "float", "MIN": -90.0, "MAX": 90.0, "DEFAULT": 3.0 },
		{ "NAME": "iterations", "LABEL": "Steps of the walk", "TYPE": "float", "MIN": 8.0, "MAX": 150.0, "DEFAULT": 40.0 },
		{ "NAME": "shade", "LABEL": "Darkening of long walks", "TYPE": "float", "MIN": 0.0, "MAX": 3.0, "DEFAULT": 0.8 },
		{ "NAME": "palette", "LABEL": "Palette of the roots", "TYPE": "long", "VALUES": [0, 1, 2, 3, 4, 5, 6, 7], "LABELS": ["Fire", "Ice", "Rainbow", "Acid", "Twilight", "Gold", "Grey", "Two colours"], "DEFAULT": 2 },
		{ "NAME": "colourA", "LABEL": "Colour A (two-colour palette)", "TYPE": "color", "DEFAULT": [0.05, 0.1, 0.6, 1.0] },
		{ "NAME": "colourB", "LABEL": "Colour B (two-colour palette)", "TYPE": "color", "DEFAULT": [1.0, 0.7, 0.2, 1.0] },
		{ "NAME": "colourOffset", "LABEL": "Colour shift", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "colourCycle", "LABEL": "Colour cycling (turns a second)", "TYPE": "float", "MIN": -2.0, "MAX": 2.0, "DEFAULT": 0.04 },
		{ "NAME": "lostColour", "LABEL": "Colour of lost points", "TYPE": "color", "DEFAULT": [0.0, 0.0, 0.0, 1.0] },
		{ "NAME": "samples", "LABEL": "Smoothing of the edges", "TYPE": "long", "VALUES": [1, 2, 3], "LABELS": ["None", "4 samples a pixel", "9 samples a pixel"], "DEFAULT": 2 }
	]
}*/

// --- shared: palettes (begin: a copy of assets/shaders/shared/palettes.glsl, changed only by tools/shaders/sync_shared_blocks.py) ---
#define TAU 6.28318530718

// The colour of a hue (0 to 1 round the wheel), at full strength.
vec3 hue_to_rgb(float hue)
{
	return clamp(abs(fract(hue + vec3(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0) - 1.0, 0.0, 1.0);
}

// A wave that goes 1, 0, 1 as t goes from 0 to 1, so that a palette that is drawn with it has no edge where it begins again.
float mirrored(float t)
{
	return abs(fract(t) * 2.0 - 1.0);
}

vec3 cosine_colour(float t, vec3 middle, vec3 reach, vec3 turns, vec3 start)
{
	return middle + reach * cos(TAU * (turns * t + start));
}

// The colour that palette `which` (the input `palette`) gives to t. A palette goes up and comes back down as t goes up, so that its two ends meet and t may be any number.
// The last palette is a blend of the inputs `colourA` and `colourB`.
vec3 palette_colour(int which, float t)
{
	float x = 1.0 - mirrored(t);
	if (which == 0) {
		return clamp(vec3(3.0 * x, 3.0 * x - 1.0, 3.0 * x - 2.0), 0.0, 1.0);
	}
	if (which == 1) {
		vec3 deep = vec3(0.02, 0.05, 0.25);
		vec3 water = vec3(0.10, 0.70, 0.95);
		return x < 0.6 ? mix(deep, water, x / 0.6) : mix(water, vec3(1.0), (x - 0.6) / 0.4);
	}
	if (which == 2) {
		return hue_to_rgb(t) * 0.92 + 0.04;
	}
	if (which == 3) {
		return cosine_colour(x, vec3(0.50, 0.50, 0.50), vec3(0.55, 0.50, 0.50), vec3(1.5, 1.0, 0.5), vec3(0.85, 0.25, 0.60));
	}
	if (which == 4) {
		return cosine_colour(x, vec3(0.45, 0.30, 0.55), vec3(0.45, 0.35, 0.40), vec3(0.90, 0.75, 1.00), vec3(0.00, 0.20, 0.50));
	}
	if (which == 5) {
		return clamp(vec3(0.15, 0.08, 0.02) + vec3(1.15, 0.85, 0.30) * x, 0.0, 1.0) + vec3(0.25, 0.25, 0.25) * x * x * x;
	}
	if (which == 6) {
		return vec3(x);
	}
	return mix(colourA.rgb, colourB.rgb, x);
}
// --- shared: palettes (end) ---

// --- shared: complex (begin: a copy of assets/shaders/shared/complex.glsl, changed only by tools/shaders/sync_shared_blocks.py) ---
// Complex numbers are vec2: the real part across, the imaginary part up.

vec2 complex_multiply(vec2 a, vec2 b)
{
	return vec2(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}

vec2 complex_divide(vec2 a, vec2 b)
{
	return vec2(a.x * b.x + a.y * b.y, a.y * b.x - a.x * b.y) / max(dot(b, b), 1.0e-30);
}

// z to a real power, by its size and its angle. (The angle is cut where it turns round, so a power that is not a whole number has a seam on the negative side.)
vec2 complex_power(vec2 z, float exponent)
{
	float size_squared = dot(z, z);
	if (size_squared < 1e-30) {
		return vec2(0.0);
	}
	float size = exp(0.5 * exponent * log(size_squared));
	float angle = exponent * atan(z.y, z.x);
	return size * vec2(cos(angle), sin(angle));
}
// --- shared: complex (end) ---

// How near a step has to be to nothing for the walk to have arrived.
#define ARRIVED 0.001

// The colour of the point `start`, walked by Newton's method (with this relaxation, and this point added after each step).
vec3 basin_colour(vec2 start, float relax, vec2 added, int most_steps, float colour_time)
{
	float root_count = float(roots);
	vec2 z = start;
	float last_step = 1.0e3;
	// How long the walk was, if it arrived; -1 if it did not.
	float walk = -1.0;
	for (int step_number = 0; step_number < 150; step_number++) {
		if (step_number >= most_steps) {
			break;
		}
		// z to the power one less than the number of roots, by multiplying.
		vec2 power_less_one = vec2(1.0, 0.0);
		for (int times = 0; times < 8; times++) {
			if (times >= roots - 1) {
				break;
			}
			power_less_one = complex_multiply(power_less_one, z);
		}
		vec2 value = complex_multiply(power_less_one, z) - vec2(1.0, 0.0);
		vec2 slope = root_count * power_less_one;
		vec2 move = added - relax * complex_divide(value, slope);
		z += move;
		float moved = length(move);
		if (moved < ARRIVED) {
			// When the size of the steps crossed ARRIVED, between the step before and this one (they get smaller by about the same share each time).
			float before = max(last_step, ARRIVED);
			float share = clamp(log(before / ARRIVED) / max(log(before / max(moved, 1.0e-12)), 1.0e-6), 0.0, 1.0);
			walk = float(step_number) - 1.0 + share;
			break;
		}
		last_step = moved;
	}
	if (walk < 0.0) {
		return lostColour.rgb;
	}
	// Which root it arrived at, by the angle of the point; the roots are the corners of a polygon.
	float which = mod(floor(atan(z.y, z.x) / (TAU / root_count) + 0.5), root_count);
	// (The roots are spread along the way up the palette, which is half of it, because a palette comes back down; the rainbow has no way back, and has the whole wheel.)
	float along = (which + 0.5) / root_count * (palette == 2 ? 1.0 : 0.5);
	vec3 root_colour = palette_colour(palette, along + colourOffset + colour_time);
	return root_colour * (0.2 + 0.8 * exp(-shade * 0.1 * walk));
}

void main()
{
	float view_height = 4.0 * exp2(-zoom);
	float turn = radians(rotation + spin * TIME);
	float turn_cos = cos(turn);
	float turn_sin = sin(turn);
	float relax = relaxation + wobble * sin(TAU * wobbleSpeed * TIME);
	int most_steps = int(min(iterations, 150.0));
	float colour_time = colourCycle * TIME;
	vec2 added = vec2(addX, addY);
	int across = samples;
	vec3 total = vec3(0.0);
	// (One loop over the samples, and not one in another: the software adapter of Windows stops on shaders that have two, with a heavy call in them.)
	for (int sample_number = 0; sample_number < 9; sample_number++) {
		if (sample_number >= across * across) {
			break;
		}
		vec2 offset = (vec2(float(sample_number - (sample_number / across) * across), float(sample_number / across)) + 0.5) / float(across) - 0.5;
		vec2 from_the_middle = (gl_FragCoord.xy + offset - 0.5 * RENDERSIZE.xy) / RENDERSIZE.y * view_height;
		vec2 turned = vec2(turn_cos * from_the_middle.x - turn_sin * from_the_middle.y, turn_sin * from_the_middle.x + turn_cos * from_the_middle.y);
		total += basin_colour(vec2(centerX, centerY) + turned, relax, added, most_steps, colour_time);
	}
	gl_FragColor = vec4(total / float(across * across), 1.0);
}
