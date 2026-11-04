// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "A plasma: waves added to waves, bent by waves, until the whole picture is a slow storm of colour. Every layer of bending makes it more tangled; the palette runs through it and cycles with the time. Speeds and rates are in turns a second, unless said otherwise.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Trippy"],
	"INPUTS": [
		{ "NAME": "scale", "LABEL": "Wave scale", "TYPE": "float", "MIN": 0.5, "MAX": 12.0, "DEFAULT": 3.0 },
		{ "NAME": "bendLayers", "LABEL": "Layers of bending", "TYPE": "long", "VALUES": [0, 1, 2, 3, 4], "LABELS": ["None", "1", "2", "3", "4"], "DEFAULT": 2 },
		{ "NAME": "bendAmount", "LABEL": "Bend", "TYPE": "float", "MIN": 0.0, "MAX": 3.0, "DEFAULT": 1.2 },
		{ "NAME": "swirl", "LABEL": "Swirl", "TYPE": "float", "MIN": -3.0, "MAX": 3.0, "DEFAULT": 0.6 },
		{ "NAME": "speed", "LABEL": "Speed", "TYPE": "float", "MIN": 0.0, "MAX": 4.0, "DEFAULT": 0.6 },
		{ "NAME": "centerX", "LABEL": "Centre x", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "centerY", "LABEL": "Centre y", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "palette", "LABEL": "Palette", "TYPE": "long", "VALUES": [0, 1, 2, 3, 4, 5, 6, 7], "LABELS": ["Fire", "Ice", "Rainbow", "Acid", "Twilight", "Gold", "Grey", "Two colours"], "DEFAULT": 3 },
		{ "NAME": "colourA", "LABEL": "Colour A", "TYPE": "color", "DEFAULT": [0.05, 0.0, 0.3, 1.0] },
		{ "NAME": "colourB", "LABEL": "Colour B", "TYPE": "color", "DEFAULT": [1.0, 0.4, 0.8, 1.0] },
		{ "NAME": "colourScale", "LABEL": "Bands", "TYPE": "float", "MIN": 0.1, "MAX": 6.0, "DEFAULT": 1.2 },
		{ "NAME": "colourOffset", "LABEL": "Colour shift", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "colourCycle", "LABEL": "Cycle speed", "TYPE": "float", "MIN": -2.0, "MAX": 2.0, "DEFAULT": 0.06 }
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
	vec2 p = (isf_FragNormCoord - vec2(centerX, centerY)) * vec2(aspect, 1.0) * scale;
	float moving = TIME * speed;
	// The middle turns faster than the edge.
	float swirl_angle = swirl * (1.0 - clamp(length(p) / (scale * 0.8), 0.0, 1.0)) * 2.0;
	p = vec2(cos(swirl_angle) * p.x - sin(swirl_angle) * p.y, sin(swirl_angle) * p.x + cos(swirl_angle) * p.y);
	// Each layer of bending moves the point by waves that depend on where it is, so the waves of the next layer are read at bent places.
	for (int layer_number = 0; layer_number < 4; layer_number++) {
		if (layer_number >= bendLayers) {
			break;
		}
		float layer = float(layer_number);
		p += bendAmount / (1.0 + layer) * vec2(sin(p.y * 1.3 + moving * 1.1 + layer * 1.7), cos(p.x * 1.1 - moving * 0.9 + layer * 2.3));
	}
	float sum = sin(p.x + moving) + sin(p.y * 1.2 - moving * 0.7) + sin((p.x + p.y) * 0.8 + moving * 0.5) + sin(length(p) * 1.5 - moving);
	float level = sum * 0.125 + 0.5;
	gl_FragColor = vec4(palette_colour(palette, level * colourScale + colourOffset + colourCycle * TIME), 1.0);
}
