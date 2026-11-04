// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "The picture repainted in a palette: how light each place is picks its colour, so that a face is made of fire or ice or all the colours of the rainbow, and the colours run through the picture as waves and cycle with the time. Solarize it, posterize it (the number is how many steps of lightness there are, and under 1 is none), and keep as much of the real colours as you like. Speeds and rates are in turns a second, unless said otherwise.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Trippy"],
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "palette", "LABEL": "Palette", "TYPE": "long", "VALUES": [0, 1, 2, 3, 4, 5, 6, 7], "LABELS": ["Fire", "Ice", "Rainbow", "Acid", "Twilight", "Gold", "Grey", "Two colours"], "DEFAULT": 3 },
		{ "NAME": "colourA", "LABEL": "Colour A", "TYPE": "color", "DEFAULT": [0.05, 0.02, 0.4, 1.0] },
		{ "NAME": "colourB", "LABEL": "Colour B", "TYPE": "color", "DEFAULT": [1.0, 0.85, 0.3, 1.0] },
		{ "NAME": "bands", "LABEL": "Repeats", "TYPE": "float", "MIN": 0.2, "MAX": 8.0, "DEFAULT": 1.5 },
		{ "NAME": "shift", "LABEL": "Colour shift", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "cycle", "LABEL": "Cycle speed", "TYPE": "float", "MIN": -2.0, "MAX": 2.0, "DEFAULT": 0.15 },
		{ "NAME": "waves", "LABEL": "Waves", "TYPE": "float", "MIN": 0.0, "MAX": 4.0, "DEFAULT": 0.6 },
		{ "NAME": "waveSpeed", "LABEL": "Wave speed", "TYPE": "float", "MIN": -2.0, "MAX": 2.0, "DEFAULT": 0.2 },
		{ "NAME": "solarize", "LABEL": "Solarize", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "posterize", "LABEL": "Posterize", "TYPE": "float", "MIN": 0.0, "MAX": 16.0, "DEFAULT": 0.0 },
		{ "NAME": "keepColour", "LABEL": "Keep colour", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.2 },
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

void main()
{
	vec4 original = IMG_THIS_NORM_PIXEL(inputImage);
	float aspect = RENDERSIZE.x / RENDERSIZE.y;
	float light = dot(original.rgb, vec3(0.299, 0.587, 0.114));
	if (solarize > 0.0) {
		light = mix(light, abs(light * 2.0 - 1.0), solarize);
	}
	if (posterize >= 1.0) {
		light = floor(light * posterize + 0.5) / posterize;
	}
	// A wave of colour that runs through the picture: the colour of a place is moved along the palette by the wave that is there.
	vec2 across_the_picture = (isf_FragNormCoord - 0.5) * vec2(aspect, 1.0);
	float wave = waves * (across_the_picture.x * 0.7 + across_the_picture.y * 0.4 + 0.5 * length(across_the_picture));
	float along = light * bands + shift + cycle * TIME + wave + waveSpeed * TIME * waves;
	vec3 painted = palette_colour(palette, along);
	vec3 colour = mix(painted, painted * (0.3 + 1.4 * original.rgb), keepColour);
	gl_FragColor = mix(original, vec4(colour, original.a), amount);
}
