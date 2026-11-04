// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "Rings that run out across the picture from one point, two or three, as if drops fell on water that the picture is under: it bends and shines where the waves are, the colours can be pulled apart along them, and they fade with the distance. Phase is how far the rings have run out (1 is one ring further, which looks the same as 0), and Source turn takes the sources round. Nothing moves by itself: give any slider keyframes to animate it.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Trippy"],
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "waveHeight", "LABEL": "Height", "TYPE": "float", "MIN": 0.0, "MAX": 0.1, "DEFAULT": 0.02 },
		{ "NAME": "waveLength", "LABEL": "Spacing", "TYPE": "float", "MIN": 0.01, "MAX": 0.5, "DEFAULT": 0.09 },
		{ "NAME": "wavePhase", "LABEL": "Phase", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "fadeAway", "LABEL": "Fade", "TYPE": "float", "MIN": 0.0, "MAX": 5.0, "DEFAULT": 0.9 },
		{ "NAME": "sources", "LABEL": "Sources", "TYPE": "long", "VALUES": [1, 2, 3], "LABELS": ["1", "2", "3"], "DEFAULT": 2 },
		{ "NAME": "sourceSpread", "LABEL": "Source dist.", "TYPE": "float", "MIN": 0.0, "MAX": 0.5, "DEFAULT": 0.18 },
		{ "NAME": "sourceTurn", "LABEL": "Source turn", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "centerX", "LABEL": "Centre x", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "centerY", "LABEL": "Centre y", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "shine", "LABEL": "Shine", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.35 },
		{ "NAME": "splitColours", "LABEL": "Colour split", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.25 },
		{ "NAME": "amount", "LABEL": "Amount", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 1.0 }
	]
}*/

// --- shared: picture (begin: a copy of assets/shaders/shared/picture.glsl, changed only by tools/shaders/sync_shared_blocks.py) ---
// Helpers for the shaders that work on a picture.

// A place of the picture, made to be on the picture by mirroring: what goes out at an edge comes back in, so that there is never an edge to see.
vec2 mirrored_repeat(vec2 place)
{
	return 1.0 - abs(1.0 - mod(place, 2.0));
}

// The vector `arrow` turned by `turn` radians, counter-clockwise.
vec2 rotated(vec2 arrow, float turn)
{
	float along = cos(turn);
	float across = sin(turn);
	return vec2(along * arrow.x - across * arrow.y, across * arrow.x + along * arrow.y);
}

// The colour turned round the wheel of colours by `turn` radians, keeping how bright it is (the colour is taken apart into its brightness and two numbers of colour, and the two are turned).
vec3 hue_turned(vec3 colour, float turn)
{
	vec3 brightness_and_colour = vec3(
		dot(colour, vec3(0.299, 0.587, 0.114)),
		dot(colour, vec3(0.596, -0.274, -0.322)),
		dot(colour, vec3(0.211, -0.523, 0.312)));
	vec2 colour_turned = rotated(brightness_and_colour.yz, turn);
	return vec3(
		brightness_and_colour.x + 0.956 * colour_turned.x + 0.621 * colour_turned.y,
		brightness_and_colour.x - 0.272 * colour_turned.x - 0.647 * colour_turned.y,
		brightness_and_colour.x - 1.106 * colour_turned.x + 1.703 * colour_turned.y);
}
// --- shared: picture (end) ---

#define TAU 6.28318530718

void main()
{
	vec2 uv = isf_FragNormCoord;
	vec2 centre = vec2(centerX, centerY);
	float aspect = RENDERSIZE.x / RENDERSIZE.y;
	vec2 push = vec2(0.0);
	float slope = 0.0;
	for (int source_number = 0; source_number < 3; source_number++) {
		if (source_number >= sources) {
			break;
		}
		float source_angle = TAU * (float(source_number) / float(sources) + sourceTurn);
		vec2 source = centre + sourceSpread * vec2(cos(source_angle), sin(source_angle)) / vec2(aspect, 1.0);
		vec2 away = (uv - source) * vec2(aspect, 1.0);
		float distance_away = max(length(away), 1.0e-4);
		float phase = TAU * (distance_away / waveLength - wavePhase);
		float fading = exp(-fadeAway * distance_away);
		push += away / distance_away * sin(phase) * fading;
		slope += cos(phase) * fading;
	}
	vec2 bend = push * waveHeight / vec2(aspect, 1.0);
	vec4 middle = IMG_NORM_PIXEL(inputImage, mirrored_repeat(uv + bend));
	float red = IMG_NORM_PIXEL(inputImage, mirrored_repeat(uv + bend * (1.0 + splitColours))).r;
	float blue = IMG_NORM_PIXEL(inputImage, mirrored_repeat(uv + bend * (1.0 - splitColours))).b;
	vec3 colour = vec3(red, middle.g, blue) * (1.0 + shine * 0.5 * slope / float(sources));
	vec4 original = IMG_THIS_NORM_PIXEL(inputImage);
	gl_FragColor = mix(original, vec4(colour, middle.a), amount);
}
