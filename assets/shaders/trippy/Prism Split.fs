// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "The picture seen through a prism: the red, green and blue of it are pulled apart (in one direction, away from a point, or round it), and copies of it ring the middle, each in a colour of its own, so that every edge grows a rainbow. Speeds and rates are in turns a second, unless said otherwise.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Trippy"],
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "spread", "LABEL": "Colour split", "TYPE": "float", "MIN": 0.0, "MAX": 0.15, "DEFAULT": 0.02 },
		{ "NAME": "pullDirection", "LABEL": "Split direction", "TYPE": "long", "VALUES": [0, 1, 2], "LABELS": ["One direction", "Away from the centre", "Round the centre"], "DEFAULT": 1 },
		{ "NAME": "angle", "LABEL": "Angle °", "TYPE": "float", "MIN": -180.0, "MAX": 180.0, "DEFAULT": 0.0 },
		{ "NAME": "angleSpin", "LABEL": "Angle (°/s)", "TYPE": "float", "MIN": -360.0, "MAX": 360.0, "DEFAULT": 20.0 },
		{ "NAME": "copies", "LABEL": "Copies round the middle", "TYPE": "long", "VALUES": [0, 2, 3, 4, 5, 6], "LABELS": ["None", "2", "3", "4", "5", "6"], "DEFAULT": 3 },
		{ "NAME": "copyDistance", "LABEL": "Copy radius", "TYPE": "float", "MIN": 0.0, "MAX": 0.5, "DEFAULT": 0.12 },
		{ "NAME": "copyStrength", "LABEL": "Copy power", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "copySpin", "LABEL": "Copy spin", "TYPE": "float", "MIN": -1.0, "MAX": 1.0, "DEFAULT": 0.08 },
		{ "NAME": "centerX", "LABEL": "Centre x", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "centerY", "LABEL": "Centre y", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
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

// How far, and which way, the red is pulled at a point of the picture (the blue is pulled the other way).
vec2 pull_at(vec2 uv, vec2 centre, float aspect)
{
	float turn = radians(angle + angleSpin * TIME);
	if (pullDirection == 0) {
		return spread * vec2(cos(turn), sin(turn)) / vec2(aspect, 1.0) * 1.0;
	}
	vec2 from_the_centre = (uv - centre) * vec2(aspect, 1.0);
	float distance_out = length(from_the_centre);
	vec2 outward = from_the_centre / max(distance_out, 1.0e-4);
	// (The pull grows with the distance from the centre, as it does at the edge of a lens.)
	if (pullDirection == 1) {
		return spread * 2.0 * distance_out * rotated(outward, turn) / vec2(aspect, 1.0);
	}
	vec2 round_about = vec2(-outward.y, outward.x);
	return spread * 2.0 * distance_out * rotated(round_about, turn) / vec2(aspect, 1.0);
}

void main()
{
	vec2 uv = isf_FragNormCoord;
	vec2 centre = vec2(centerX, centerY);
	float aspect = RENDERSIZE.x / RENDERSIZE.y;
	vec2 pull = pull_at(uv, centre, aspect);
	float red = IMG_NORM_PIXEL(inputImage, mirrored_repeat(uv + pull)).r;
	vec4 middle = IMG_NORM_PIXEL(inputImage, mirrored_repeat(uv));
	float blue = IMG_NORM_PIXEL(inputImage, mirrored_repeat(uv - pull)).b;
	vec3 colour = vec3(red, middle.g, blue);
	// The copies: the picture again, shifted round a ring, each copy turned to a colour of its own.
	vec3 ring = vec3(0.0);
	for (int copy_number = 0; copy_number < 6; copy_number++) {
		if (copy_number >= copies) {
			break;
		}
		float copy_angle = TAU * (float(copy_number) / float(copies) + copySpin * TIME);
		vec2 shifted = uv + copyDistance * vec2(cos(copy_angle), sin(copy_angle)) / vec2(aspect, 1.0);
		vec3 copy_colour = IMG_NORM_PIXEL(inputImage, mirrored_repeat(shifted)).rgb;
		ring += hue_turned(copy_colour, copy_angle);
	}
	if (copies > 0) {
		colour = mix(colour, max(colour, ring / float(copies) * 1.4), copyStrength);
	}
	vec4 original = IMG_THIS_NORM_PIXEL(inputImage);
	gl_FragColor = mix(original, vec4(colour, middle.a), amount);
}
