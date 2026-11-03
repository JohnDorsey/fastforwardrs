// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "Ink in water, or the veins of marble that is still liquid: smooth noise read at places that are pushed by smooth noise that is pushed by smooth noise, so that the colour folds into itself in slow tongues and swirls and never stops moving. Veins of a lighter colour run through it, if you want them.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Trippy"],
	"INPUTS": [
		{ "NAME": "scale", "LABEL": "Swirl size (higher: smaller)", "TYPE": "float", "MIN": 0.3, "MAX": 10.0, "DEFAULT": 1.6 },
		{ "NAME": "flowSpeed", "LABEL": "Speed of the flow", "TYPE": "float", "MIN": 0.0, "MAX": 3.0, "DEFAULT": 0.3 },
		{ "NAME": "pushing", "LABEL": "Noise pushed by noise", "TYPE": "float", "MIN": 0.0, "MAX": 8.0, "DEFAULT": 3.2 },
		{ "NAME": "detailLevels", "LABEL": "Fine swirls on the big ones", "TYPE": "long", "VALUES": [1, 2, 3, 4, 5], "LABELS": ["None", "1", "2", "3", "4"], "DEFAULT": 4 },
		{ "NAME": "veins", "LABEL": "Veins (0 is none)", "TYPE": "float", "MIN": 0.0, "MAX": 12.0, "DEFAULT": 4.0 },
		{ "NAME": "veinSharpness", "LABEL": "How thin the veins are", "TYPE": "float", "MIN": 1.0, "MAX": 40.0, "DEFAULT": 8.0 },
		{ "NAME": "veinStrength", "LABEL": "How bright the veins are", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.35 },
		{ "NAME": "centerX", "LABEL": "Centre across", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "centerY", "LABEL": "Centre up", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "palette", "LABEL": "Palette", "TYPE": "long", "VALUES": [0, 1, 2, 3, 4, 5, 6, 7], "LABELS": ["Fire", "Ice", "Rainbow", "Acid", "Twilight", "Gold", "Grey", "Two colours"], "DEFAULT": 4 },
		{ "NAME": "colourA", "LABEL": "Colour A (two-colour palette)", "TYPE": "color", "DEFAULT": [0.02, 0.02, 0.25, 1.0] },
		{ "NAME": "colourB", "LABEL": "Colour B (two-colour palette)", "TYPE": "color", "DEFAULT": [0.95, 0.6, 0.9, 1.0] },
		{ "NAME": "colourScale", "LABEL": "Colour bands", "TYPE": "float", "MIN": 0.1, "MAX": 6.0, "DEFAULT": 1.4 },
		{ "NAME": "colourOffset", "LABEL": "Colour shift", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "colourCycle", "LABEL": "Colour cycling (turns a second)", "TYPE": "float", "MIN": -2.0, "MAX": 2.0, "DEFAULT": 0.04 }
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

void main()
{
	float aspect = RENDERSIZE.x / RENDERSIZE.y;
	vec2 spot = (isf_FragNormCoord - vec2(centerX, centerY)) * vec2(aspect, 1.0) * scale;
	float moving = TIME * flowSpeed;
	// Noise read at spots that noise pushes, twice.
	vec2 pushed_once = vec2(
		layered_noise(vec3(spot, moving), detailLevels),
		layered_noise(vec3(spot + vec2(5.2, 1.3), moving + 2.0), detailLevels));
	vec2 pushed_twice = vec2(
		layered_noise(vec3(spot + pushing * pushed_once + vec2(1.7, 9.2), moving * 0.8 + 4.0), detailLevels),
		layered_noise(vec3(spot + pushing * pushed_once + vec2(8.3, 2.8), moving * 0.8 + 6.0), detailLevels));
	float ink = layered_noise(vec3(spot + pushing * pushed_twice, moving * 0.6), detailLevels);
	// Veins: lines where the ink is at a few levels, thin and bright.
	float vein = 0.0;
	if (veins > 0.0) {
		float wave = abs(sin(ink * veins * 3.14159265));
		vein = pow(1.0 - wave, veinSharpness) * veinStrength;
	}
	vec3 colour = palette_colour(palette, ink * colourScale * 2.0 + length(pushed_once - 0.5) * 0.6 + colourOffset + colourCycle * TIME);
	gl_FragColor = vec4(colour + vec3(vein), 1.0);
}
