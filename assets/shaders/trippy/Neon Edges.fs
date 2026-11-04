// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "The edges of the picture drawn in glowing colours, as if it were made of neon tubes: where the picture changes quickly a line lights up (an edge weaker than the cut-off does not), in a colour that goes with the way the edge faces, how strong it is or where it is, and Colour shift moves the colours along the palette. The picture under the lines can be dimmed or kept. Nothing moves by itself: give any slider keyframes to animate it.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Trippy"],
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "edgeWidth", "LABEL": "Line width", "TYPE": "float", "MIN": 0.5, "MAX": 6.0, "DEFAULT": 1.5 },
		{ "NAME": "gain", "LABEL": "Brightness", "TYPE": "float", "MIN": 0.0, "MAX": 10.0, "DEFAULT": 4.0 },
		{ "NAME": "cutOff", "LABEL": "Cut-off", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.05 },
		{ "NAME": "colourBy", "LABEL": "Colour by", "TYPE": "long", "VALUES": [0, 1, 2], "LABELS": ["The way the edge faces", "How strong the edge is", "Where the line is"], "DEFAULT": 0 },
		{ "NAME": "palette", "LABEL": "Palette", "TYPE": "long", "VALUES": [0, 1, 2, 3, 4, 5, 6, 7], "LABELS": ["Fire", "Ice", "Rainbow", "Acid", "Twilight", "Gold", "Grey", "Two colours"], "DEFAULT": 2 },
		{ "NAME": "colourA", "LABEL": "Colour A", "TYPE": "color", "DEFAULT": [0.0, 0.8, 1.0, 1.0] },
		{ "NAME": "colourB", "LABEL": "Colour B", "TYPE": "color", "DEFAULT": [1.0, 0.1, 0.8, 1.0] },
		{ "NAME": "colourScale", "LABEL": "Bands", "TYPE": "float", "MIN": 0.1, "MAX": 6.0, "DEFAULT": 1.0 },
		{ "NAME": "colourOffset", "LABEL": "Colour shift", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "keepPicture", "LABEL": "Picture", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.15 },
		{ "NAME": "amount", "LABEL": "Amount", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 1.0 }
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

// How light the picture is at a place `across` and `up` pixels from this one.
float light_at(vec2 across_and_up)
{
	return dot(IMG_NORM_PIXEL(inputImage, isf_FragNormCoord + across_and_up / RENDERSIZE).rgb, vec3(0.299, 0.587, 0.114));
}

void main()
{
	vec4 original = IMG_THIS_NORM_PIXEL(inputImage);
	// The slope of the lightness across and up, from the eight places round this one (Sobel's way).
	vec2 reach_of_the_look = vec2(edgeWidth);
	float top_left = light_at(vec2(-1.0, 1.0) * reach_of_the_look);
	float top = light_at(vec2(0.0, 1.0) * reach_of_the_look);
	float top_right = light_at(vec2(1.0, 1.0) * reach_of_the_look);
	float left = light_at(vec2(-1.0, 0.0) * reach_of_the_look);
	float right = light_at(vec2(1.0, 0.0) * reach_of_the_look);
	float bottom_left = light_at(vec2(-1.0, -1.0) * reach_of_the_look);
	float bottom = light_at(vec2(0.0, -1.0) * reach_of_the_look);
	float bottom_right = light_at(vec2(1.0, -1.0) * reach_of_the_look);
	float slope_across = (top_right + 2.0 * right + bottom_right) - (top_left + 2.0 * left + bottom_left);
	float slope_up = (top_left + 2.0 * top + top_right) - (bottom_left + 2.0 * bottom + bottom_right);
	float strength = length(vec2(slope_across, slope_up));
	float tube_strength = clamp((strength - cutOff) * gain, 0.0, 1.0);
	float along;
	if (colourBy == 0) {
		along = atan(slope_up, slope_across) / 6.2831853 * colourScale;
	} else if (colourBy == 1) {
		along = strength * colourScale * 0.5;
	} else {
		along = (isf_FragNormCoord.x + isf_FragNormCoord.y * 0.6) * colourScale;
	}
	vec3 tube = palette_colour(palette, along + colourOffset);
	// A little extra where the line is brightest, as a tube glows white in the middle.
	vec3 lit = tube * tube_strength + vec3(1.0) * tube_strength * tube_strength * tube_strength * 0.35;
	vec3 colour = original.rgb * keepPicture + lit;
	gl_FragColor = mix(original, vec4(colour, original.a), amount);
}
