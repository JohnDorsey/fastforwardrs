// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "Circles inside circles inside circles: the plane is folded into a tile and turned inside out again and again, and what is left is a gasket of circles that never ends. The strength of the turning inside out changes how the circles crowd together. The zoom is counted in octaves, which are doublings. Nothing moves by itself: give any slider keyframes to animate it.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Fractal"],
	"INPUTS": [
		{ "NAME": "inversion", "LABEL": "Inversion", "TYPE": "float", "MIN": 1.0, "MAX": 1.6, "DEFAULT": 1.18 },
		{ "NAME": "iterations", "LABEL": "Folds", "TYPE": "float", "MIN": 2.0, "MAX": 14.0, "DEFAULT": 8.0 },
		{ "NAME": "zoom", "LABEL": "Zoom", "TYPE": "float", "MIN": -2.0, "MAX": 6.0, "DEFAULT": 0.0 },
		{ "NAME": "centerX", "LABEL": "Centre x", "TYPE": "float", "MIN": -4.0, "MAX": 4.0, "DEFAULT": 0.0 },
		{ "NAME": "centerY", "LABEL": "Centre y", "TYPE": "float", "MIN": -4.0, "MAX": 4.0, "DEFAULT": 0.0 },
		{ "NAME": "rotation", "LABEL": "Rotation (°)", "TYPE": "float", "MIN": -180.0, "MAX": 180.0, "DEFAULT": 0.0 },
		{ "NAME": "colourMode", "LABEL": "Colour by", "TYPE": "long", "VALUES": [0, 1, 2], "LABELS": ["Orbit near a circle", "Fold depth", "Both"], "DEFAULT": 2 },
		{ "NAME": "palette", "LABEL": "Palette", "TYPE": "long", "VALUES": [0, 1, 2, 3, 4, 5, 6, 7], "LABELS": ["Fire", "Ice", "Rainbow", "Acid", "Twilight", "Gold", "Grey", "Two colours"], "DEFAULT": 4 },
		{ "NAME": "colourA", "LABEL": "Colour A", "TYPE": "color", "DEFAULT": [0.02, 0.05, 0.3, 1.0] },
		{ "NAME": "colourB", "LABEL": "Colour B", "TYPE": "color", "DEFAULT": [1.0, 0.85, 0.4, 1.0] },
		{ "NAME": "colourScale", "LABEL": "Bands", "TYPE": "float", "MIN": 0.05, "MAX": 8.0, "DEFAULT": 1.0 },
		{ "NAME": "colourOffset", "LABEL": "Colour shift", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "lines", "LABEL": "Dark lines", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.6 },
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

// The colour of the point `start` of the plane: the plane is folded into a square tile, and the tile is turned inside out through a circle, again and again.
vec3 gasket_colour(vec2 start, float strength, int most_folds)
{
	vec2 p = start;
	// How much the plane has been stretched in all by the turnings inside out: what the sizes of the last tile are divided by to give sizes of the plane.
	float stretched = 1.0;
	float nearest = 1.0e3;
	for (int fold = 0; fold < 14; fold++) {
		if (fold >= most_folds) {
			break;
		}
		p = -1.0 + 2.0 * fract(0.5 * p + 0.5);
		float size_squared = max(dot(p, p), 1.0e-6);
		nearest = min(nearest, size_squared);
		float stretch = strength / size_squared;
		p *= stretch;
		stretched *= stretch;
	}
	float depth = log(stretched);
	float t;
	if (colourMode == 0) {
		t = sqrt(nearest) * 1.2;
	} else if (colourMode == 1) {
		t = depth * 0.12;
	} else {
		t = sqrt(nearest) * 0.9 + depth * 0.08;
	}
	vec3 colour = palette_colour(palette, t * colourScale + colourOffset);
	// Dark lines where the last tile has the edge of a circle: how far the point is from the circle of the tile, in the sizes of the plane.
	float to_the_circle = abs(length(p) - 1.0) / stretched;
	float line = 1.0 - smoothstep(0.0, 0.004 + 0.0, to_the_circle);
	return colour * (1.0 - lines * line);
}

void main()
{
	float view_height = 4.0 * exp2(-zoom);
	float turn = radians(rotation);
	float turn_cos = cos(turn);
	float turn_sin = sin(turn);
	float strength = inversion;
	int most_folds = int(min(iterations, 14.0));
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
		total += gasket_colour(vec2(centerX, centerY) + turned, strength, most_folds);
	}
	gl_FragColor = vec4(total / float(across * across), 1.0);
}
