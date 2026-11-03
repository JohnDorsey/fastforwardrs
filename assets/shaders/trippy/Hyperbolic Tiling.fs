// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "A tiling of the hyperbolic plane, drawn in a disc, in which the tiles are all the same size and shape and are squeezed smaller and smaller towards the rim, where there is no end of them. The disc is slid about inside itself as the clip plays, so that the tiles stream past and shrink away into the edge.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Trippy"],
	"INPUTS": [
		{ "NAME": "tiling", "LABEL": "Tiling", "TYPE": "long", "VALUES": [0, 1, 2, 3, 4, 5, 6, 7, 8, 9], "LABELS": ["3 sides, 7 at a corner", "3 sides, 8 at a corner", "4 sides, 5 at a corner", "4 sides, 6 at a corner", "5 sides, 4 at a corner", "5 sides, 5 at a corner", "6 sides, 4 at a corner", "7 sides, 3 at a corner", "8 sides, 3 at a corner", "4 sides, 8 at a corner"], "DEFAULT": 4 },
		{ "NAME": "driftSize", "LABEL": "Slide of the disc", "TYPE": "float", "MIN": 0.0, "MAX": 0.9, "DEFAULT": 0.5 },
		{ "NAME": "driftSpeed", "LABEL": "Slide speed (turns a second)", "TYPE": "float", "MIN": -1.0, "MAX": 1.0, "DEFAULT": 0.06 },
		{ "NAME": "discSize", "LABEL": "Size of the disc", "TYPE": "float", "MIN": 0.2, "MAX": 1.2, "DEFAULT": 0.47 },
		{ "NAME": "rotation", "LABEL": "Rotation (degrees)", "TYPE": "float", "MIN": -180.0, "MAX": 180.0, "DEFAULT": 0.0 },
		{ "NAME": "spin", "LABEL": "Spin (degrees a second)", "TYPE": "float", "MIN": -90.0, "MAX": 90.0, "DEFAULT": 5.0 },
		{ "NAME": "foldLimit", "LABEL": "Folds (more: nearer the rim)", "TYPE": "float", "MIN": 6.0, "MAX": 60.0, "DEFAULT": 40.0 },
		{ "NAME": "lineWidth", "LABEL": "Width of the lines", "TYPE": "float", "MIN": 0.0, "MAX": 0.2, "DEFAULT": 0.035 },
		{ "NAME": "lineDarkness", "LABEL": "How dark the lines are", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.8 },
		{ "NAME": "palette", "LABEL": "Palette", "TYPE": "long", "VALUES": [0, 1, 2, 3, 4, 5, 6, 7], "LABELS": ["Fire", "Ice", "Rainbow", "Acid", "Twilight", "Gold", "Grey", "Two colours"], "DEFAULT": 2 },
		{ "NAME": "colourA", "LABEL": "Colour A (two-colour palette)", "TYPE": "color", "DEFAULT": [0.05, 0.05, 0.4, 1.0] },
		{ "NAME": "colourB", "LABEL": "Colour B (two-colour palette)", "TYPE": "color", "DEFAULT": [1.0, 0.7, 0.3, 1.0] },
		{ "NAME": "colourScale", "LABEL": "Colour bands", "TYPE": "float", "MIN": 0.1, "MAX": 6.0, "DEFAULT": 1.0 },
		{ "NAME": "colourCycle", "LABEL": "Colour cycling (turns a second)", "TYPE": "float", "MIN": -2.0, "MAX": 2.0, "DEFAULT": 0.05 },
		{ "NAME": "outsideColour", "LABEL": "Colour outside the disc", "TYPE": "color", "DEFAULT": [0.02, 0.02, 0.06, 1.0] }
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

// --- shared: complex (begin: a copy of assets/shaders/shared/complex.glsl, changed only by tools/shaders/sync_shared_blocks.py) ---
// Complex numbers are vec2: the real part across, the imaginary part up.

vec2 complex_multiply(vec2 a, vec2 b)
{
	return vec2(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}

vec2 complex_divide(vec2 a, vec2 b)
{
	return vec2(a.x * b.x + a.y * b.y, a.y * b.x - a.x * b.y) / max(dot(b, b), 1.0e-30);
}

// z to a real power, by its size and its angle. (The angle is cut where it turns round, so a power that is not a whole number has a seam on the negative side.)
vec2 complex_power(vec2 z, float exponent)
{
	float size_squared = dot(z, z);
	if (size_squared < 1e-30) {
		return vec2(0.0);
	}
	float size = exp(0.5 * exponent * log(size_squared));
	float angle = exponent * atan(z.y, z.x);
	return size * vec2(cos(angle), sin(angle));
}
// --- shared: complex (end) ---

#define HALF_TURN 3.14159265359

// The number of sides of the tiles of the tiling, and the number of them that meet at a corner.
float sides_of_the_tiles()
{
	const float SIDES[10] = float[10](3.0, 3.0, 4.0, 4.0, 5.0, 5.0, 6.0, 7.0, 8.0, 4.0);
	return SIDES[tiling];
}

float tiles_at_a_corner()
{
	const float CORNERS[10] = float[10](7.0, 8.0, 5.0, 6.0, 4.0, 5.0, 4.0, 3.0, 3.0, 8.0);
	return CORNERS[tiling];
}

void main()
{
	float sides = sides_of_the_tiles();
	float corners = tiles_at_a_corner();
	// The tile is cut into 2 * sides triangles by mirrors: two lines through its middle, which is the middle of the disc, with the angle half a turn / sides between them, and a circle
	// (the edge of the tile) whose middle is on the second line and which meets the rim of the disc square on.
	float wedge = HALF_TURN / sides;
	float across_the_corner = cos(HALF_TURN / corners);
	float along_the_line = across_the_corner / sqrt(max(across_the_corner * across_the_corner - sin(wedge) * sin(wedge), 1.0e-4));
	float circle_radius = along_the_line * sin(wedge) / across_the_corner;
	vec2 circle_middle = along_the_line * vec2(cos(wedge), sin(wedge));
	float aspect = RENDERSIZE.x / RENDERSIZE.y;
	vec2 on_the_screen = (isf_FragNormCoord - 0.5) * vec2(aspect, 1.0) / discSize;
	float outside = length(on_the_screen);
	float turn = radians(rotation + spin * TIME);
	vec2 place = vec2(cos(turn) * on_the_screen.x - sin(turn) * on_the_screen.y, sin(turn) * on_the_screen.x + cos(turn) * on_the_screen.y);
	// The disc slid about inside itself: a move of the hyperbolic plane that takes the point `slide` to the middle.
	float slide_angle = TAU * driftSpeed * TIME;
	vec2 slide = driftSize * vec2(cos(slide_angle), sin(slide_angle * 1.3));
	vec2 z = complex_divide(place - slide, vec2(1.0, 0.0) - complex_multiply(vec2(slide.x, -slide.y), place));
	float folds = 0.0;
	int most_folds = int(foldLimit);
	for (int fold_number = 0; fold_number < 60; fold_number++) {
		if (fold_number >= most_folds) {
			break;
		}
		// Into the wedge between the two lines through the middle...
		float angle = atan(z.y, z.x);
		float size = length(z);
		float in_the_wedge = mod(angle, 2.0 * wedge);
		if (in_the_wedge > wedge) {
			in_the_wedge = 2.0 * wedge - in_the_wedge;
		}
		z = size * vec2(cos(in_the_wedge), sin(in_the_wedge));
		// ...and, if it is inside the circle, turned inside out through it, which puts it back towards the middle tile.
		vec2 from_the_circle = z - circle_middle;
		float distance_squared = dot(from_the_circle, from_the_circle);
		if (distance_squared >= circle_radius * circle_radius) {
			break;
		}
		z = circle_middle + from_the_circle * (circle_radius * circle_radius / distance_squared);
		folds += 1.0;
	}
	// How near the point is to the edge of its tile (the circle) and to the lines that cut the tile up (the two lines through the middle).
	float to_the_edge = length(z - circle_middle) - circle_radius;
	float line_amount = 1.0 - smoothstep(0.0, lineWidth, to_the_edge);
	float angle_of_the_point = atan(z.y, z.x);
	float t = folds * 0.13 * colourScale + length(z) * 0.5 * colourScale + angle_of_the_point * 0.4 + colourCycle * TIME;
	vec3 tile = palette_colour(palette, t);
	vec3 colour = tile * (1.0 - lineDarkness * line_amount);
	// The rim: what is outside the disc, and a fade into it where the tiles are too small to be told apart.
	float rim = smoothstep(0.97, 1.0, length(place));
	colour = mix(colour, outsideColour.rgb, max(rim, step(1.0, outside)));
	gl_FragColor = vec4(colour, 1.0);
}
