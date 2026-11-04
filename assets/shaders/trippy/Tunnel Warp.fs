// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "The picture on the walls of a tunnel that you fly down: it is wrapped round the inside of a pipe that goes away to a point in the middle, twisting as it comes. Flight takes you down it (1 is there and back, which looks the same as 0). Mirrored, so that there is never an edge. Nothing moves by itself: give any slider keyframes to animate it.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Trippy"],
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "flight", "LABEL": "Flight", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "twist", "LABEL": "Twist", "TYPE": "float", "MIN": -3.0, "MAX": 3.0, "DEFAULT": 0.8 },
		{ "NAME": "wraps", "LABEL": "Copies round the tunnel", "TYPE": "long", "VALUES": [1, 2, 3, 4, 5, 6], "LABELS": ["1", "2", "3", "4", "5", "6"], "DEFAULT": 2 },
		{ "NAME": "tunnelLength", "LABEL": "Length", "TYPE": "float", "MIN": 0.1, "MAX": 3.0, "DEFAULT": 0.5 },
		{ "NAME": "pictureSize", "LABEL": "Picture size", "TYPE": "float", "MIN": 0.2, "MAX": 3.0, "DEFAULT": 1.0 },
		{ "NAME": "darkness", "LABEL": "Far darkness", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.7 },
		{ "NAME": "centerX", "LABEL": "Centre x", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "centerY", "LABEL": "Centre y", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "rotation", "LABEL": "Rotation (°)", "TYPE": "float", "MIN": -180.0, "MAX": 180.0, "DEFAULT": 0.0 },
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
	vec2 centre = vec2(centerX, centerY);
	float aspect = RENDERSIZE.x / RENDERSIZE.y;
	vec2 here = rotated((isf_FragNormCoord - centre) * vec2(aspect, 1.0), -radians(rotation));
	float radius = max(length(here), 1.0e-4);
	float angle = atan(here.y, here.x);
	// How far down the tunnel the wall is at this point: the nearer the middle, the farther.
	float depth = tunnelLength / radius;
	float around = angle / TAU * float(wraps) + twist * depth * 0.1;
	float along = depth / pictureSize + 2.0 * flight;
	vec2 place = mirrored_repeat(vec2(around, along));
	vec4 seen = IMG_NORM_PIXEL(inputImage, place);
	// (The far end is dark, and the very middle, where the picture is too fine to be anything but noise, is darker still.)
	float far_end = (1.0 - darkness * exp(-radius * 5.0)) * mix(0.1, 1.0, smoothstep(0.0, 0.12, radius));
	vec4 original = IMG_THIS_NORM_PIXEL(inputImage);
	gl_FragColor = mix(original, vec4(seen.rgb * far_end, seen.a), amount);
}
