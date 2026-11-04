// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "The picture seen through moving liquid: it is pushed about by smooth noise that is itself pushed about by noise, so that it melts, ripples and flows, and the three colours can be pulled apart along the flow. Mirrored at the edges.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Trippy"],
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "amountOfWarp", "LABEL": "Warp", "TYPE": "float", "MIN": 0.0, "MAX": 0.5, "DEFAULT": 0.08 },
		{ "NAME": "fineness", "LABEL": "Ripple scale", "TYPE": "float", "MIN": 0.5, "MAX": 12.0, "DEFAULT": 3.0 },
		{ "NAME": "flowSpeed", "LABEL": "Flow speed", "TYPE": "float", "MIN": 0.0, "MAX": 3.0, "DEFAULT": 0.35 },
		{ "NAME": "detailLevels", "LABEL": "Fine ripples", "TYPE": "long", "VALUES": [1, 2, 3, 4, 5], "LABELS": ["None", "1", "2", "3", "4"], "DEFAULT": 3 },
		{ "NAME": "warpOfWarp", "LABEL": "Push", "TYPE": "float", "MIN": 0.0, "MAX": 2.0, "DEFAULT": 0.8 },
		{ "NAME": "splitColours", "LABEL": "Colour split", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.3 },
		{ "NAME": "amount", "LABEL": "Amount", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 1.0 }
	]
}*/

// --- shared: noise (begin: a copy of assets/shaders/shared/noise.glsl, changed only by tools/shaders/sync_shared_blocks.py) ---
// Smooth noise in three dimensions (the third is usually the time, so that the noise changes shape instead of sliding): a number from 0 to 1 for every point of space, made by mixing the
// numbers of the eight corners of the little cube that the point is in. The number of a corner is made by whole-number arithmetic, which every graphics card does the same (a hash made with
// `sin` is not: the card's sine of a large number is not the same on every make of card, and the noise shows it in blocks).

// A whole number mixed until its bits look random: each bit of the result depends on every bit of the number.
uint mixed_bits(uint bits)
{
	bits ^= bits >> 16u;
	bits *= 2146121005u;
	bits ^= bits >> 15u;
	bits *= 2221713035u;
	bits ^= bits >> 16u;
	return bits;
}

// The number, from 0 to 1, that the corner of a cell has. (A corner has whole numbers for its three places.)
float noise_corner(vec3 corner)
{
	ivec3 whole = ivec3(floor(corner + 0.5));
	uint bits = mixed_bits(uint(whole.x) + mixed_bits(uint(whole.y) + mixed_bits(uint(whole.z) + 83u)));
	return float(bits >> 8u) / 16777215.0;
}

float smooth_noise(vec3 point)
{
	vec3 cell = floor(point);
	vec3 inside = fract(point);
	vec3 eased = inside * inside * (3.0 - 2.0 * inside);
	float low = mix(
		mix(noise_corner(cell), noise_corner(cell + vec3(1.0, 0.0, 0.0)), eased.x),
		mix(noise_corner(cell + vec3(0.0, 1.0, 0.0)), noise_corner(cell + vec3(1.0, 1.0, 0.0)), eased.x),
		eased.y);
	float high = mix(
		mix(noise_corner(cell + vec3(0.0, 0.0, 1.0)), noise_corner(cell + vec3(1.0, 0.0, 1.0)), eased.x),
		mix(noise_corner(cell + vec3(0.0, 1.0, 1.0)), noise_corner(cell + vec3(1.0, 1.0, 1.0)), eased.x),
		eased.y);
	return mix(low, high, eased.z);
}

// Several layers of smooth noise, each half as strong and twice as fine as the one before (up to six): the sum is from 0 to about 1.
float layered_noise(vec3 point, int layers)
{
	float sum = 0.0;
	float strength = 0.5;
	for (int layer = 0; layer < 6; layer++) {
		if (layer >= layers) {
			break;
		}
		sum += strength * smooth_noise(point);
		point = point * 2.03 + vec3(17.3, 9.1, 4.7);
		strength *= 0.5;
	}
	return sum;
}
// --- shared: noise (end) ---

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
	vec2 uv = isf_FragNormCoord;
	float aspect = RENDERSIZE.x / RENDERSIZE.y;
	vec2 point = (uv - 0.5) * vec2(aspect, 1.0) * fineness;
	float moving = TIME * flowSpeed;
	// The first noise, two numbers a point: how far to push across and up.
	vec2 first = vec2(
		layered_noise(vec3(point, moving), detailLevels),
		layered_noise(vec3(point + vec2(5.2, 1.3), moving + 3.1), detailLevels)) - 0.5;
	// A second noise, read at the points that the first pushes about.
	vec2 pushed_point = point + first * warpOfWarp * 2.0;
	vec2 second = vec2(
		layered_noise(vec3(pushed_point + vec2(1.7, 9.2), moving * 1.3), detailLevels),
		layered_noise(vec3(pushed_point + vec2(8.3, 2.8), moving * 1.3 + 7.0), detailLevels)) - 0.5;
	vec2 push = (first + warpOfWarp * second) * 2.0 * amountOfWarp / vec2(aspect, 1.0);
	vec4 middle = IMG_NORM_PIXEL(inputImage, mirrored_repeat(uv + push));
	float red = IMG_NORM_PIXEL(inputImage, mirrored_repeat(uv + push * (1.0 + splitColours))).r;
	float blue = IMG_NORM_PIXEL(inputImage, mirrored_repeat(uv + push * (1.0 - splitColours))).b;
	vec4 original = IMG_THIS_NORM_PIXEL(inputImage);
	gl_FragColor = mix(original, vec4(red, middle.g, blue, middle.a), amount);
}
