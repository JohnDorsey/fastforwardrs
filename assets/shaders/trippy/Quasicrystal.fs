// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "A quasicrystal: waves that run in several directions at once, evenly spread round the circle, added together. Where they agree the picture lights up, and the sum makes patterns that have a symmetry of five or seven or twelve and never repeat. Phase moves the waves, and Wave scale makes the pattern finer or coarser. Nothing moves by itself: give any slider keyframes to animate it.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Trippy"],
	"INPUTS": [
		{ "NAME": "directions", "LABEL": "Directions (symmetry)", "TYPE": "long", "VALUES": [3, 4, 5, 6, 7, 8, 9, 10, 11, 12], "LABELS": ["3", "4", "5", "6", "7", "8", "9", "10", "11", "12"], "DEFAULT": 7 },
		{ "NAME": "waveSize", "LABEL": "Wave scale", "TYPE": "float", "MIN": 2.0, "MAX": 80.0, "DEFAULT": 18.0 },
		{ "NAME": "wavePhase", "LABEL": "Phase", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "rotation", "LABEL": "Rotation (°)", "TYPE": "float", "MIN": -180.0, "MAX": 180.0, "DEFAULT": 0.0 },
		{ "NAME": "sharpness", "LABEL": "Sharpness", "TYPE": "float", "MIN": 0.2, "MAX": 6.0, "DEFAULT": 1.5 },
		{ "NAME": "centerX", "LABEL": "Centre x", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "centerY", "LABEL": "Centre y", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "palette", "LABEL": "Palette", "TYPE": "long", "VALUES": [0, 1, 2, 3, 4, 5, 6, 7], "LABELS": ["Fire", "Ice", "Rainbow", "Acid", "Twilight", "Gold", "Grey", "Two colours"], "DEFAULT": 2 },
		{ "NAME": "colourA", "LABEL": "Colour A", "TYPE": "color", "DEFAULT": [0.0, 0.05, 0.3, 1.0] },
		{ "NAME": "colourB", "LABEL": "Colour B", "TYPE": "color", "DEFAULT": [1.0, 0.9, 0.5, 1.0] },
		{ "NAME": "colourScale", "LABEL": "Bands", "TYPE": "float", "MIN": 0.1, "MAX": 6.0, "DEFAULT": 1.0 },
		{ "NAME": "colourOffset", "LABEL": "Colour shift", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 }
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

void main()
{
	float aspect = RENDERSIZE.x / RENDERSIZE.y;
	vec2 here = (isf_FragNormCoord - vec2(centerX, centerY)) * vec2(aspect, 1.0);
	float turn = radians(rotation);
	float size = waveSize;
	float sum = 0.0;
	for (int direction_number = 0; direction_number < 12; direction_number++) {
		if (direction_number >= directions) {
			break;
		}
		float direction_angle = TAU * float(direction_number) / float(directions) + turn;
		float along = dot(here, vec2(cos(direction_angle), sin(direction_angle)));
		sum += cos(size * along + TAU * wavePhase + float(direction_number) * 0.35);
	}
	// The sum of the waves is from -n to n: how much of the most that it is, from -1 to 1, made sharper.
	float agreed = sum / float(directions);
	float level = 0.5 + 0.5 * sign(agreed) * pow(abs(agreed), 1.0 / sharpness);
	gl_FragColor = vec4(palette_colour(palette, level * colourScale + colourOffset), 1.0);
}
