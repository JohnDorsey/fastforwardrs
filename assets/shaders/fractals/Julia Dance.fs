// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "A Julia set made for a point on a path: Along path takes the point once round the edge of the main body of the Mandelbrot set (or round another path; reach 1 is the edge itself, less is inside it and more is outside), and as it goes the set grows branches, falls to dust and gathers again. Turn and zoom it (the zoom is counted in octaves, which are doublings), colour it in six ways, and colour the inside of it too. Nothing moves by itself: give any slider keyframes to animate it.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Fractal"],
	"INPUTS": [
		{ "NAME": "path", "LABEL": "Path", "TYPE": "long", "VALUES": [0, 1, 2], "LABELS": ["Edge of the main cardioid", "Edge of the disc to its left", "Circle round the middle"], "DEFAULT": 0 },
		{ "NAME": "pathReach", "LABEL": "Reach", "TYPE": "float", "MIN": 0.5, "MAX": 1.5, "DEFAULT": 1.0 },
		{ "NAME": "alongPath", "LABEL": "Along path", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.12 },
		{ "NAME": "nudgeX", "LABEL": "Nudge x", "TYPE": "float", "MIN": -0.5, "MAX": 0.5, "DEFAULT": 0.0 },
		{ "NAME": "nudgeY", "LABEL": "Nudge y", "TYPE": "float", "MIN": -0.5, "MAX": 0.5, "DEFAULT": 0.0 },
		{ "NAME": "power", "LABEL": "Power", "TYPE": "float", "MIN": 2.0, "MAX": 6.0, "DEFAULT": 2.0 },
		{ "NAME": "zoom", "LABEL": "Zoom", "TYPE": "float", "MIN": -1.0, "MAX": 8.0, "DEFAULT": 0.7 },
		{ "NAME": "centerX", "LABEL": "Centre x", "TYPE": "float", "MIN": -2.0, "MAX": 2.0, "DEFAULT": 0.0 },
		{ "NAME": "centerY", "LABEL": "Centre y", "TYPE": "float", "MIN": -2.0, "MAX": 2.0, "DEFAULT": 0.0 },
		{ "NAME": "rotation", "LABEL": "Rotation (°)", "TYPE": "float", "MIN": -180.0, "MAX": 180.0, "DEFAULT": 0.0 },
		{ "NAME": "iterations", "LABEL": "Iterations", "TYPE": "float", "MIN": 20.0, "MAX": 600.0, "DEFAULT": 140.0 },
		{ "NAME": "colourMode", "LABEL": "Outside", "TYPE": "long", "VALUES": [0, 1, 2, 3], "LABELS": ["Smooth iterations", "Orbit trap: a circle", "Orbit trap: a cross", "Iterations and angle"], "DEFAULT": 0 },
		{ "NAME": "insideMode", "LABEL": "Inside", "TYPE": "long", "VALUES": [0, 1, 2], "LABELS": ["One colour", "Where the orbit ends", "Nearness to the middle"], "DEFAULT": 1 },
		{ "NAME": "palette", "LABEL": "Palette", "TYPE": "long", "VALUES": [0, 1, 2, 3, 4, 5, 6, 7], "LABELS": ["Fire", "Ice", "Rainbow", "Acid", "Twilight", "Gold", "Grey", "Two colours"], "DEFAULT": 4 },
		{ "NAME": "colourA", "LABEL": "Colour A", "TYPE": "color", "DEFAULT": [0.05, 0.02, 0.35, 1.0] },
		{ "NAME": "colourB", "LABEL": "Colour B", "TYPE": "color", "DEFAULT": [1.0, 0.55, 0.85, 1.0] },
		{ "NAME": "colourScale", "LABEL": "Bands", "TYPE": "float", "MIN": 0.05, "MAX": 8.0, "DEFAULT": 1.0 },
		{ "NAME": "colourOffset", "LABEL": "Colour shift", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "insideColour", "LABEL": "Inside colour", "TYPE": "color", "DEFAULT": [0.0, 0.0, 0.0, 1.0] },
		{ "NAME": "trapSize", "LABEL": "Trap radius", "TYPE": "float", "MIN": 0.0, "MAX": 2.0, "DEFAULT": 0.6 },
		{ "NAME": "samples", "LABEL": "Edge smoothing", "TYPE": "long", "VALUES": [1, 2, 3], "LABELS": ["None", "4 samples a pixel", "9 samples a pixel"], "DEFAULT": 2 }
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

// The point that the set is made for: a place along its path (`turns` is how far round the path, from 0 to 1), moved by hand if it is to be.
vec2 the_point(float turns)
{
	float angle = TAU * turns;
	vec2 on_the_path;
	if (path == 0) {
		// The edge of the main cardioid.
		on_the_path = pathReach * (0.5 * vec2(cos(angle), sin(angle)) - 0.25 * vec2(cos(2.0 * angle), sin(2.0 * angle)));
	} else if (path == 1) {
		// The edge of the disc that touches the cardioid on its left.
		on_the_path = vec2(-1.0, 0.0) + pathReach * 0.25 * vec2(cos(angle), sin(angle));
	} else {
		on_the_path = pathReach * 0.75 * vec2(cos(angle), sin(angle));
	}
	return on_the_path + vec2(nudgeX, nudgeY);
}

struct Orbit
{
	float count;
	float trap_distance;
	float final_turns;
	float got_out;
	// How near to the middle the orbit came, and where it was after the last step (for the inside).
	float nearest_to_the_middle;
	vec2 last;
};

Orbit orbit_of(vec2 start, vec2 added, int most_steps)
{
	Orbit result;
	result.count = 0.0;
	result.trap_distance = 0.0;
	result.final_turns = 0.0;
	result.got_out = 0.0;
	result.nearest_to_the_middle = 0.0;
	vec2 z = start;
	float nearest_trap = 1.0e10;
	float nearest_middle = 1.0e10;
	float radius = min(64.0, exp(41.0 / power));
	float limit_squared = radius * radius;
	for (int step_number = 0; step_number < 2000; step_number++) {
		if (step_number >= most_steps) {
			break;
		}
		if (power == 2.0) {
			z = vec2(z.x * z.x - z.y * z.y, 2.0 * z.x * z.y) + added;
		} else {
			z = complex_power(z, power) + added;
		}
		nearest_middle = min(nearest_middle, length(z));
		if (colourMode == 1) {
			nearest_trap = min(nearest_trap, abs(length(z) - trapSize));
		} else if (colourMode == 2) {
			nearest_trap = min(nearest_trap, min(abs(z.x), abs(z.y)));
		}
		float size_squared = dot(z, z);
		if (size_squared > limit_squared) {
			result.count = float(step_number) + 1.0 - log(0.5 * log(size_squared) / log(radius)) / log(power);
			result.final_turns = atan(z.y, z.x) / TAU;
			result.got_out = 1.0;
			break;
		}
	}
	result.last = z;
	result.trap_distance = nearest_trap;
	result.nearest_to_the_middle = nearest_middle;
	return result;
}

vec3 colour_of_the_point(vec2 where_, vec2 julia_point, int most_steps)
{
	Orbit orbit = orbit_of(where_, julia_point, most_steps);
	float t;
	if (orbit.got_out < 0.5) {
		if (insideMode == 0) {
			return insideColour.rgb;
		}
		if (insideMode == 1) {
			t = atan(orbit.last.y, orbit.last.x) / TAU * colourScale;
		} else {
			t = orbit.nearest_to_the_middle * 2.0 * colourScale;
		}
		return palette_colour(palette, t + colourOffset) * 0.8;
	}
	// (The count is taken to a power under one, so that the bands are close near the set, where the count is large, and still go through the palette far from it.)
	float spread = pow(max(orbit.count, 0.0), 0.6) * 0.25;
	if (colourMode == 0) {
		t = spread;
	} else if (colourMode == 3) {
		t = spread + orbit.final_turns * 2.0;
	} else {
		t = orbit.trap_distance * 0.9;
	}
	return palette_colour(palette, t * colourScale + colourOffset);
}

void main()
{
	float view_height = 3.0 * exp2(-zoom);
	float turn = radians(rotation);
	float turn_cos = cos(turn);
	float turn_sin = sin(turn);
	vec2 julia_point = the_point(alongPath);
	int most_steps = int(min(iterations, 2000.0));
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
		total += colour_of_the_point(vec2(centerX, centerY) + turned, julia_point, most_steps);
	}
	gl_FragColor = vec4(total / float(across * across), 1.0);
}
