// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "The Mandelbrot set and its relatives, or the Julia set of a point, drawn with smooth colours and zoomed in as the clip plays. Several folds of the plane (Burning Ship, Tricorn, Celtic and others), any power from 2 up, six ways of colouring, eight palettes. Single precision: deeper than about 14 powers of two the picture turns blocky.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Fractal"],
	"INPUTS": [
		{ "NAME": "variant", "LABEL": "Fold of the plane", "TYPE": "long", "VALUES": [0, 1, 2, 3, 4, 5], "LABELS": ["Mandelbrot", "Burning Ship", "Tricorn (conjugate)", "Celtic (fold the real part after the power)", "Fold the real part before the power", "Fold the imaginary part before the power"], "DEFAULT": 0 },
		{ "NAME": "power", "LABEL": "Power", "TYPE": "float", "MIN": 2.0, "MAX": 8.0, "DEFAULT": 2.0 },
		{ "NAME": "zoom", "LABEL": "Zoom (powers of two)", "TYPE": "float", "MIN": -1.0, "MAX": 16.0, "DEFAULT": 0.0 },
		{ "NAME": "zoomSpeed", "LABEL": "Zoom speed (doublings a second)", "TYPE": "float", "MIN": -3.0, "MAX": 3.0, "DEFAULT": 0.7 },
		{ "NAME": "zoomLimit", "LABEL": "Zoom turns back at (doublings)", "TYPE": "float", "MIN": 1.0, "MAX": 16.0, "DEFAULT": 12.0 },
		{ "NAME": "centerX", "LABEL": "Zoom in on, across", "TYPE": "float", "MIN": -2.5, "MAX": 2.5, "DEFAULT": -0.10109636 },
		{ "NAME": "centerY", "LABEL": "Zoom in on, up", "TYPE": "float", "MIN": -2.5, "MAX": 2.5, "DEFAULT": 0.95628651 },
		{ "NAME": "wholeSetFirst", "LABEL": "Begin on the whole set", "TYPE": "bool", "DEFAULT": true },
		{ "NAME": "rotation", "LABEL": "Rotation (degrees)", "TYPE": "float", "MIN": -180.0, "MAX": 180.0, "DEFAULT": 0.0 },
		{ "NAME": "spin", "LABEL": "Spin (degrees a second)", "TYPE": "float", "MIN": -90.0, "MAX": 90.0, "DEFAULT": 3.0 },
		{ "NAME": "iterations", "LABEL": "Iterations", "TYPE": "float", "MIN": 16.0, "MAX": 1500.0, "DEFAULT": 150.0 },
		{ "NAME": "iterationsPerZoom", "LABEL": "Iterations per power of two", "TYPE": "float", "MIN": 0.0, "MAX": 100.0, "DEFAULT": 25.0 },
		{ "NAME": "bailout", "LABEL": "Escape radius", "TYPE": "float", "MIN": 4.0, "MAX": 1000.0, "DEFAULT": 64.0 },
		{ "NAME": "julia", "LABEL": "Julia set of the point below", "TYPE": "bool", "DEFAULT": false },
		{ "NAME": "juliaX", "LABEL": "Julia point across", "TYPE": "float", "MIN": -2.0, "MAX": 2.0, "DEFAULT": -0.8 },
		{ "NAME": "juliaY", "LABEL": "Julia point up", "TYPE": "float", "MIN": -2.0, "MAX": 2.0, "DEFAULT": 0.156 },
		{ "NAME": "juliaOrbit", "LABEL": "Julia point circles by", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "juliaOrbitSpeed", "LABEL": "Julia circling (turns a second)", "TYPE": "float", "MIN": -1.0, "MAX": 1.0, "DEFAULT": 0.05 },
		{ "NAME": "colourMode", "LABEL": "Colour by", "TYPE": "long", "VALUES": [0, 1, 2, 3, 4, 5], "LABELS": ["Smooth iteration count", "Orbit trap: a point", "Orbit trap: a cross", "Orbit trap: a circle", "Distance to the set", "Iteration count and final angle"], "DEFAULT": 0 },
		{ "NAME": "palette", "LABEL": "Palette", "TYPE": "long", "VALUES": [0, 1, 2, 3, 4, 5, 6, 7], "LABELS": ["Fire", "Ice", "Rainbow", "Acid", "Twilight", "Gold", "Grey", "Two colours"], "DEFAULT": 3 },
		{ "NAME": "colourA", "LABEL": "Colour A (two-colour palette)", "TYPE": "color", "DEFAULT": [0.04, 0.05, 0.45, 1.0] },
		{ "NAME": "colourB", "LABEL": "Colour B (two-colour palette)", "TYPE": "color", "DEFAULT": [1.0, 0.82, 0.25, 1.0] },
		{ "NAME": "colourScale", "LABEL": "Colour bands", "TYPE": "float", "MIN": 0.05, "MAX": 8.0, "DEFAULT": 1.0 },
		{ "NAME": "colourOffset", "LABEL": "Colour shift", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "colourCycle", "LABEL": "Colour cycling (turns a second)", "TYPE": "float", "MIN": -2.0, "MAX": 2.0, "DEFAULT": 0.08 },
		{ "NAME": "insideColour", "LABEL": "Colour of the inside of the set", "TYPE": "color", "DEFAULT": [0.0, 0.0, 0.0, 1.0] },
		{ "NAME": "glow", "LABEL": "Glow along the edge of the set", "TYPE": "float", "MIN": 0.0, "MAX": 3.0, "DEFAULT": 0.5 },
		{ "NAME": "trapX", "LABEL": "Orbit trap across", "TYPE": "float", "MIN": -2.0, "MAX": 2.0, "DEFAULT": 0.0 },
		{ "NAME": "trapY", "LABEL": "Orbit trap up", "TYPE": "float", "MIN": -2.0, "MAX": 2.0, "DEFAULT": 0.0 },
		{ "NAME": "trapSize", "LABEL": "Orbit trap circle radius", "TYPE": "float", "MIN": 0.0, "MAX": 2.0, "DEFAULT": 0.5 },
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

// ---- the plane ----

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

// What the fold of the plane does to a point before it is taken to the power.
vec2 folded(vec2 z)
{
	if (variant == 1) {
		return abs(z);
	}
	if (variant == 2) {
		return vec2(z.x, -z.y);
	}
	if (variant == 4) {
		return vec2(abs(z.x), z.y);
	}
	if (variant == 5) {
		return vec2(z.x, abs(z.y));
	}
	return z;
}

struct Escape
{
	// How many steps it took to get out, made smooth (the steps, less how far past the edge it landed); 0 for a point that never got out.
	float count;
	// How far the point is from the set, in the units of the plane.
	float distance_to_the_set;
	// How near the orbit came to the trap, for the modes that use one.
	float trap_distance;
	// The angle of the point where the orbit left, in turns.
	float final_turns;
	float got_out;
};

Escape escape_of(vec2 start, vec2 added, int most_steps)
{
	Escape result;
	result.count = 0.0;
	result.distance_to_the_set = 0.0;
	result.trap_distance = 0.0;
	result.final_turns = 0.0;
	result.got_out = 0.0;
	vec2 z = start;
	// How fast the orbit moves away when the starting point moves (for the Julia set), or when the added point moves (for the others).
	vec2 slope = julia ? vec2(1.0, 0.0) : vec2(0.0, 0.0);
	vec2 step_of_slope = julia ? vec2(0.0, 0.0) : vec2(1.0, 0.0);
	float nearest = 1.0e10;
	// (The orbit is let leave by this radius, or by the one that keeps the next step from overflowing a number of this precision.)
	float radius = min(bailout, exp(41.0 / power));
	float limit_squared = radius * radius;
	for (int step_number = 0; step_number < 4000; step_number++) {
		if (step_number >= most_steps) {
			break;
		}
		vec2 base = folded(z);
		vec2 next;
		vec2 turn_of_next;
		if (power == 2.0) {
			next = vec2(base.x * base.x - base.y * base.y, 2.0 * base.x * base.y);
			turn_of_next = 2.0 * base;
		} else {
			next = complex_power(base, power);
			turn_of_next = power * complex_power(base, power - 1.0);
		}
		if (variant == 3) {
			next.x = abs(next.x);
		}
		slope = complex_multiply(turn_of_next, slope) + step_of_slope;
		z = next + added;
		if (colourMode == 1) {
			nearest = min(nearest, length(z - vec2(trapX, trapY)));
		} else if (colourMode == 2) {
			nearest = min(nearest, min(abs(z.x - trapX), abs(z.y - trapY)));
		} else if (colourMode == 3) {
			nearest = min(nearest, abs(length(z - vec2(trapX, trapY)) - trapSize));
		}
		float size_squared = dot(z, z);
		if (size_squared > limit_squared) {
			float size_log = 0.5 * log(size_squared);
			result.count = float(step_number) + 1.0 - log(size_log / log(radius)) / log(power);
			result.distance_to_the_set = 0.5 * sqrt(size_squared) * size_log / max(length(slope), 1.0e-20);
			result.final_turns = atan(z.y, z.x) / TAU;
			result.got_out = 1.0;
			break;
		}
	}
	result.trap_distance = nearest;
	return result;
}

// The colour of the point of the plane `where_`, which a pixel that is `pixel_size` across looks at.
vec3 colour_of_the_point(vec2 where_, float pixel_size, int most_steps, float colour_time, vec2 julia_point)
{
	Escape escape;
	if (julia) {
		escape = escape_of(where_, julia_point, most_steps);
	} else {
		escape = escape_of(vec2(0.0, 0.0), where_, most_steps);
	}
	if (escape.got_out < 0.5) {
		return insideColour.rgb;
	}
	float t;
	if (colourMode == 0) {
		t = escape.count * 0.08;
	} else if (colourMode == 4) {
		t = log2(max(escape.distance_to_the_set / pixel_size, 1.0)) * 0.12;
	} else if (colourMode == 5) {
		t = escape.count * 0.08 + escape.final_turns * 2.0;
	} else {
		t = escape.trap_distance * 0.7;
	}
	vec3 colour = palette_colour(palette, t * colourScale + colourOffset + colour_time);
	float rim = glow * exp(-escape.distance_to_the_set / pixel_size * 0.35);
	return mix(colour, vec3(1.0, 0.97, 0.9), clamp(rim, 0.0, 1.0));
}

// Where the middle of the picture is when the whole of the set is in view.
vec2 whole_set_centre()
{
	if (julia) {
		return vec2(0.0, 0.0);
	}
	if (variant == 0) {
		return vec2(-0.5 * clamp(3.0 - power, 0.0, 1.0), 0.0);
	}
	if (variant == 1) {
		return vec2(-0.45, -0.5);
	}
	if (variant == 2) {
		return vec2(0.0, 0.0);
	}
	return vec2(-0.4, 0.0);
}

void main()
{
	// How deep the zoom is now: it goes in at the speed that is asked for, and turns round at its limit.
	float span = zoomLimit - zoom;
	float travelled = abs(zoomSpeed) * TIME;
	if (span > 0.001) {
		float phase = mod(travelled, 2.0 * span);
		travelled = phase < span ? phase : 2.0 * span - phase;
	}
	float depth = zoom + (zoomSpeed >= 0.0 ? travelled : -travelled);
	float view_height = 3.0 * exp2(-depth);
	float pixel_size = view_height / RENDERSIZE.y;
	// The middle of the picture: the point that is zoomed in on, or, while the zoom is still shallow and the whole set is wanted first, a place between that and the middle of the set, which
	// the picture glides from as it goes in (the point is held at the same place on the picture until the glide is over).
	float depth_in = max(depth, 0.0);
	float glide = wholeSetFirst ? smoothstep(0.0, 3.5, depth_in) : 1.0;
	vec2 aimed_at = vec2(centerX, centerY);
	vec2 middle_of_the_picture = aimed_at + (whole_set_centre() - aimed_at) * exp2(-depth_in) * (1.0 - glide);
	float turn = radians(rotation + spin * TIME);
	float turn_cos = cos(turn);
	float turn_sin = sin(turn);
	int most_steps = int(min(iterations + iterationsPerZoom * max(depth, 0.0), 3000.0));
	float orbit_angle = TAU * juliaOrbitSpeed * TIME;
	vec2 julia_point = vec2(juliaX, juliaY) + juliaOrbit * vec2(cos(orbit_angle), sin(orbit_angle));
	float colour_time = colourCycle * TIME;
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
		total += colour_of_the_point(middle_of_the_picture + turned, pixel_size, most_steps, colour_time, julia_point);
	}
	gl_FragColor = vec4(total / float(across * across), 1.0);
}
