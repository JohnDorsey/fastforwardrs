// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "What a camera pointed at its own screen makes: every frame is the frame before it, a little larger and turned, with its colours turned a little and a little of it faded, and the live picture is let in at the middle, so that tunnels, spirals and trails of the picture go on for ever. It remembers the frames before it, so the preview is only exact when it is played from the start, and the export always is.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Trippy"],
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "zoomIn", "LABEL": "Echo growth (a second)", "TYPE": "float", "MIN": 0.5, "MAX": 3.0, "DEFAULT": 1.4 },
		{ "NAME": "spinSpeed", "LABEL": "Echo turn (degrees a second)", "TYPE": "float", "MIN": -360.0, "MAX": 360.0, "DEFAULT": 35.0 },
		{ "NAME": "hueSpeed", "LABEL": "Colour turn (degrees a second)", "TYPE": "float", "MIN": -720.0, "MAX": 720.0, "DEFAULT": 90.0 },
		{ "NAME": "shiftX", "LABEL": "Echo slide across (a second)", "TYPE": "float", "MIN": -1.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "shiftY", "LABEL": "Echo slide up (a second)", "TYPE": "float", "MIN": -1.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "keep", "LABEL": "Echo kept after a second", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.55 },
		{ "NAME": "inputMix", "LABEL": "Live picture let in (a frame)", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.12 },
		{ "NAME": "combine", "LABEL": "How the live picture is let in", "TYPE": "long", "VALUES": [0, 1, 2], "LABELS": ["Blended into the echo", "The lighter of the two", "Over the echo where it is not dark"], "DEFAULT": 0 },
		{ "NAME": "centerX", "LABEL": "Centre across", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "centerY", "LABEL": "Centre up", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "amount", "LABEL": "Amount of the effect", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 1.0 }
	],
	"PASSES": [
		{ "TARGET": "echo", "PERSISTENT": true, "FLOAT": true },
		{ }
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

void main()
{
	vec4 live = IMG_THIS_NORM_PIXEL(inputImage);
	if (PASSINDEX == 0) {
		// The first pass makes the echo: the echo of the frame before, moved a little, turned in its colours and faded, with the live picture let in.
		vec2 centre = vec2(centerX, centerY);
		float aspect = RENDERSIZE.x / RENDERSIZE.y;
		vec2 here = (isf_FragNormCoord - centre) * vec2(aspect, 1.0);
		float frame_zoom = pow(zoomIn, TIMEDELTA);
		float frame_turn = radians(spinSpeed) * TIMEDELTA;
		vec2 before = rotated(here / frame_zoom, -frame_turn) - vec2(shiftX, shiftY) * TIMEDELTA;
		vec4 echoed = IMG_NORM_PIXEL(echo, mirrored_repeat(centre + before / vec2(aspect, 1.0)));
		// (The first frame has no echo: the picture begins as the live picture.)
		if (echoed.a < 0.01) {
			echoed = live;
		}
		vec3 faded = hue_turned(echoed.rgb, radians(hueSpeed) * TIMEDELTA) * pow(keep, TIMEDELTA);
		// How much of the live picture is let in: the share is of a frame at thirty frames a second, whatever the rate of this one.
		float let_in = 1.0 - pow(1.0 - inputMix, TIMEDELTA * 30.0);
		vec3 made;
		if (combine == 0) {
			made = mix(faded, live.rgb, let_in);
		} else if (combine == 1) {
			made = max(faded, live.rgb * let_in * 3.0);
		} else {
			float lightness = dot(live.rgb, vec3(0.299, 0.587, 0.114));
			made = mix(faded, live.rgb, let_in * smoothstep(0.08, 0.35, lightness) * 3.0);
		}
		gl_FragColor = vec4(clamp(made, 0.0, 4.0), 1.0);
	} else {
		gl_FragColor = mix(live, IMG_THIS_NORM_PIXEL(echo), amount);
	}
}
