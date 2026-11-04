// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "The picture seen through a kaleidoscope: it is cut into wedges that are mirrors of one another, round a point, and the picture inside them turns (Image turn, and Image spin in degrees a second) and drifts on its own, so that the pattern flows and does not just spin. Mirrors within mirrors, and a different turn of the colours in alternate wedges, make it a mandala.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Trippy"],
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "wedges", "LABEL": "Wedges", "TYPE": "float", "MIN": 2.0, "MAX": 24.0, "DEFAULT": 8.0 },
		{ "NAME": "mirrorsWithin", "LABEL": "Mirrors within mirrors", "TYPE": "long", "VALUES": [1, 2, 3], "LABELS": ["One set", "Two sets", "Three sets"], "DEFAULT": 2 },
		{ "NAME": "centerX", "LABEL": "Centre x", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "centerY", "LABEL": "Centre y", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "rotation", "LABEL": "Rotation (°)", "TYPE": "float", "MIN": -180.0, "MAX": 180.0, "DEFAULT": 0.0 },
		{ "NAME": "spin", "LABEL": "Spin (°/s)", "TYPE": "float", "MIN": -180.0, "MAX": 180.0, "DEFAULT": 12.0 },
		{ "NAME": "pictureTurn", "LABEL": "Image turn °", "TYPE": "float", "MIN": -180.0, "MAX": 180.0, "DEFAULT": 0.0 },
		{ "NAME": "pictureSpin", "LABEL": "Image spin", "TYPE": "float", "MIN": -180.0, "MAX": 180.0, "DEFAULT": -7.0 },
		{ "NAME": "zoom", "LABEL": "Zoom", "TYPE": "float", "MIN": 0.2, "MAX": 4.0, "DEFAULT": 1.2 },
		{ "NAME": "driftSize", "LABEL": "Drift", "TYPE": "float", "MIN": 0.0, "MAX": 0.5, "DEFAULT": 0.1 },
		{ "NAME": "driftSpeed", "LABEL": "Drift turns/s", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.05 },
		{ "NAME": "wedgeHue", "LABEL": "Odd hue", "TYPE": "float", "MIN": 0.0, "MAX": 180.0, "DEFAULT": 40.0 },
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

// The point mirrored into the first wedge: the wedges are mirrors of one another, so every wedge shows the picture of the first, and the first is the picture of half of itself and
// the mirror image of that half.
vec2 mirrored_into_the_wedge(vec2 point, float wedge_angle)
{
	float angle = atan(point.y, point.x);
	float radius = length(point);
	float mirrored_angle = abs(mod(angle, wedge_angle) - 0.5 * wedge_angle);
	return radius * vec2(cos(mirrored_angle), sin(mirrored_angle));
}

void main()
{
	vec2 centre = vec2(centerX, centerY);
	float aspect = RENDERSIZE.x / RENDERSIZE.y;
	vec2 here = (isf_FragNormCoord - centre) * vec2(aspect, 1.0);
	float wedge_count = floor(wedges + 0.5);
	float wedge_angle = TAU / wedge_count;
	vec2 turned_here = rotated(here, -radians(rotation + spin * TIME));
	// Which wedge the point is in, as a whole number: the colours turn in the odd ones.
	float which_wedge = floor(atan(turned_here.y, turned_here.x) / wedge_angle);
	vec2 folded = turned_here;
	for (int set_number = 0; set_number < 3; set_number++) {
		if (set_number >= mirrorsWithin) {
			break;
		}
		folded = mirrored_into_the_wedge(folded, wedge_angle);
		// (The next set of mirrors is a larger, turned copy of what the last made.)
		folded = rotated(folded * 1.7, 0.45);
	}
	// The picture that is seen inside the wedges: turned and zoomed, and drifting.
	vec2 drift = driftSize * vec2(cos(TAU * driftSpeed * TIME), sin(TAU * driftSpeed * TIME * 1.3));
	vec2 inside = rotated(folded, radians(pictureTurn + pictureSpin * TIME)) / zoom;
	vec2 place = mirrored_repeat(centre + inside / vec2(aspect, 1.0) + drift);
	vec4 seen = IMG_NORM_PIXEL(inputImage, place);
	float hue_turn = radians(wedgeHue) * mod(which_wedge, 2.0);
	vec3 turned = hue_turned(seen.rgb, hue_turn);
	vec4 original = IMG_THIS_NORM_PIXEL(inputImage);
	gl_FragColor = mix(original, vec4(turned, seen.a), amount);
}
