// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "The picture inside the picture inside the picture, without end: each copy is a number of times smaller than the one outside it, and they go on in towards the middle for ever, along a spiral if you like, and in or out as the clip plays.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Trippy"],
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "ratio", "LABEL": "Shrink of each copy", "TYPE": "float", "MIN": 1.3, "MAX": 16.0, "DEFAULT": 3.0 },
		{ "NAME": "arms", "LABEL": "Spiral arms (0 = straight)", "TYPE": "long", "VALUES": [-3, -2, -1, 0, 1, 2, 3], "LABELS": ["3 the other way", "2 the other way", "1 the other way", "0", "1", "2", "3"], "DEFAULT": 1 },
		{ "NAME": "speed", "LABEL": "Speed in (copies a second)", "TYPE": "float", "MIN": -2.0, "MAX": 2.0, "DEFAULT": 0.25 },
		{ "NAME": "pictureSize", "LABEL": "Picture in each copy", "TYPE": "float", "MIN": 0.3, "MAX": 1.4, "DEFAULT": 0.75 },
		{ "NAME": "centerX", "LABEL": "Centre across", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "centerY", "LABEL": "Centre up", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "rotation", "LABEL": "Rotation (degrees)", "TYPE": "float", "MIN": -180.0, "MAX": 180.0, "DEFAULT": 0.0 },
		{ "NAME": "spin", "LABEL": "Spin (degrees a second)", "TYPE": "float", "MIN": -180.0, "MAX": 180.0, "DEFAULT": 0.0 },
		{ "NAME": "darkenInward", "LABEL": "Darkening of deeper copies", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.15 },
		{ "NAME": "amount", "LABEL": "Amount of the effect", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 1.0 }
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
	vec2 here = rotated((isf_FragNormCoord - centre) * vec2(aspect, 1.0), -radians(rotation + spin * TIME));
	float radius = max(length(here), 1.0e-5);
	float angle = atan(here.y, here.x);
	// The size of a point is its logarithm: a copy that is `ratio` times smaller is one period further in, and a spiral is a size that goes with the angle. (The arms are whole numbers,
	// so that the size is the same at the two ends of the angle, where it is cut.)
	float period = log(ratio);
	float along = log(radius) - float(arms) * (angle / TAU) * period + speed * TIME * period;
	// Where in its period the point is, from just past the size of the copy that is inside to the size of the copy that holds it.
	float within = mod(along, period) - period;
	float copy_radius = exp(within) * pictureSize;
	vec2 place = mirrored_repeat(centre + copy_radius * vec2(cos(angle), sin(angle)) / vec2(aspect, 1.0));
	vec4 seen = IMG_NORM_PIXEL(inputImage, place);
	// How many copies deep the point is: 0 for the outermost.
	float depth = floor(along / period) * -1.0;
	vec3 dimmed = seen.rgb * clamp(1.0 - darkenInward * max(depth, 0.0) * 0.25, 0.0, 1.0);
	vec4 original = IMG_THIS_NORM_PIXEL(inputImage);
	gl_FragColor = mix(original, vec4(dimmed, seen.a), amount);
}
