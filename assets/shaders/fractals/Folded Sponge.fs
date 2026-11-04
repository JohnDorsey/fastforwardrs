// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

/*{
	"DESCRIPTION": "A shape that is made by folding space again and again, so that the same few cuts are seen at every size: the Menger sponge or the Sierpinski tetrahedron. A twist of the folds turns them into bending, spiralling relatives, and it moves with the time. Drawn by ray marching, with light, fog and a glow; the camera goes round it. It asks a great deal of the graphics card: lower the ray steps or raise the coarseness for a quick look. Speeds and rates are in turns a second, unless said otherwise.",
	"CREDIT": "fastforwardrs",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Fractal"],
	"INPUTS": [
		{ "NAME": "shape", "LABEL": "Shape", "TYPE": "long", "VALUES": [0, 1], "LABELS": ["Menger sponge", "Sierpinski tetrahedron"], "DEFAULT": 0 },
		{ "NAME": "foldCount", "LABEL": "Folds", "TYPE": "float", "MIN": 1.0, "MAX": 10.0, "DEFAULT": 5.0 },
		{ "NAME": "twistAmount", "LABEL": "Twist °", "TYPE": "float", "MIN": -30.0, "MAX": 30.0, "DEFAULT": 0.0 },
		{ "NAME": "twistSpeed", "LABEL": "Twist (°/s)", "TYPE": "float", "MIN": -10.0, "MAX": 10.0, "DEFAULT": 1.5 },
		{ "NAME": "cameraTurn", "LABEL": "Cam. turn °", "TYPE": "float", "MIN": -180.0, "MAX": 180.0, "DEFAULT": 35.0 },
		{ "NAME": "cameraTilt", "LABEL": "Cam. tilt °", "TYPE": "float", "MIN": -85.0, "MAX": 85.0, "DEFAULT": 25.0 },
		{ "NAME": "cameraDistance", "LABEL": "Distance", "TYPE": "float", "MIN": 1.0, "MAX": 10.0, "DEFAULT": 3.6 },
		{ "NAME": "cameraSpin", "LABEL": "Orbit (°/s)", "TYPE": "float", "MIN": -90.0, "MAX": 90.0, "DEFAULT": 10.0 },
		{ "NAME": "fieldOfView", "LABEL": "View angle", "TYPE": "float", "MIN": 15.0, "MAX": 110.0, "DEFAULT": 50.0 },
		{ "NAME": "maxSteps", "LABEL": "Ray steps", "TYPE": "float", "MIN": 30.0, "MAX": 300.0, "DEFAULT": 100.0 },
		{ "NAME": "detail", "LABEL": "Coarseness", "TYPE": "float", "MIN": 0.2, "MAX": 4.0, "DEFAULT": 1.0 },
		{ "NAME": "lightTurn", "LABEL": "Light angle °", "TYPE": "float", "MIN": -180.0, "MAX": 180.0, "DEFAULT": 40.0 },
		{ "NAME": "glowAmount", "LABEL": "Glow", "TYPE": "float", "MIN": 0.0, "MAX": 2.0, "DEFAULT": 0.5 },
		{ "NAME": "fogAmount", "LABEL": "Fog", "TYPE": "float", "MIN": 0.0, "MAX": 3.0, "DEFAULT": 0.5 },
		{ "NAME": "backgroundTop", "LABEL": "Sky top", "TYPE": "color", "DEFAULT": [0.01, 0.03, 0.1, 1.0] },
		{ "NAME": "backgroundBottom", "LABEL": "Sky bottom", "TYPE": "color", "DEFAULT": [0.05, 0.2, 0.28, 1.0] },
		{ "NAME": "palette", "LABEL": "Palette", "TYPE": "long", "VALUES": [0, 1, 2, 3, 4, 5, 6, 7], "LABELS": ["Fire", "Ice", "Rainbow", "Acid", "Twilight", "Gold", "Grey", "Two colours"], "DEFAULT": 3 },
		{ "NAME": "colourA", "LABEL": "Colour A", "TYPE": "color", "DEFAULT": [0.05, 0.1, 0.5, 1.0] },
		{ "NAME": "colourB", "LABEL": "Colour B", "TYPE": "color", "DEFAULT": [1.0, 0.8, 0.3, 1.0] },
		{ "NAME": "colourScale", "LABEL": "Bands", "TYPE": "float", "MIN": 0.05, "MAX": 8.0, "DEFAULT": 1.0 },
		{ "NAME": "colourOffset", "LABEL": "Colour shift", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "colourCycle", "LABEL": "Cycle speed", "TYPE": "float", "MIN": -2.0, "MAX": 2.0, "DEFAULT": 0.03 },
		{ "NAME": "samples", "LABEL": "Edge smoothing", "TYPE": "long", "VALUES": [1, 2], "LABELS": ["None", "4 samples a pixel"], "DEFAULT": 1 }
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

#define SCENE_RADIUS 1.9

vec2 turned(vec2 v, float angle)
{
	float across = cos(angle);
	float up = sin(angle);
	return vec2(across * v.x - up * v.y, up * v.x + across * v.y);
}

// How far `point` is from a cube that reaches 1 each way from the middle.
float cube_distance(vec3 point)
{
	vec3 outside = abs(point) - vec3(1.0);
	return min(max(outside.x, max(outside.y, outside.z)), 0.0) + length(max(outside, vec3(0.0)));
}

// How far `point` is from the tetrahedron whose corners are (1, 1, 1), (1, -1, -1), (-1, 1, -1) and (-1, -1, 1): the farthest of the four faces.
float tetrahedron_distance(vec3 point)
{
	float facing_away = max(max(-point.x - point.y - point.z, -point.x + point.y + point.z), max(point.x - point.y + point.z, point.x + point.y - point.z));
	return (facing_away - 1.0) / 1.7320508;
}

// How far `point` is from the shape, and how near the folded point came to the middle (squared), which the colour is made from.
float scene_distance(vec3 point, out float trap)
{
	vec3 z = point;
	float shrunk = 1.0;
	float nearest = 1.0e10;
	float twist = radians(twistAmount + twistSpeed * TIME);
	int wanted = int(foldCount);
	for (int fold = 0; fold < 10; fold++) {
		if (fold >= wanted) {
			break;
		}
		if (shape == 0) {
			// The sponge: the three coordinates are put in order, biggest first, and the space is made three times as large about the corner (1, 1, 1); what falls in the empty column is
			// moved to the one next to it.
			z = abs(z);
			if (z.x < z.y) {
				z.xy = z.yx;
			}
			if (z.x < z.z) {
				z.xz = z.zx;
			}
			if (z.y < z.z) {
				z.yz = z.zy;
			}
			z = z * 3.0 - vec3(2.0);
			if (z.z < -1.0) {
				z.z += 2.0;
			}
			shrunk *= 3.0;
		} else {
			// The tetrahedron: what is behind the three planes through the middle is mirrored into the corner (1, 1, 1), and the space is made twice as large about that corner.
			if (z.x + z.y < 0.0) {
				z.xy = -z.yx;
			}
			if (z.x + z.z < 0.0) {
				z.xz = -z.zx;
			}
			if (z.y + z.z < 0.0) {
				z.zy = -z.yz;
			}
			z = z * 2.0 - vec3(1.0);
			shrunk *= 2.0;
		}
		z.xy = turned(z.xy, twist);
		z.yz = turned(z.yz, twist * 0.7);
		nearest = min(nearest, dot(z, z));
	}
	trap = nearest;
	float body = shape == 0 ? cube_distance(z) : tetrahedron_distance(z);
	return body / shrunk;
}

// --- shared: raymarch (begin: a copy of assets/shaders/shared/raymarch.glsl, changed only by tools/shaders/sync_shared_blocks.py) ---
// A ray marcher for the shaders that make a shape in space. The shader that has this block defines, before it:
//
//   #define SCENE_RADIUS r      how far from the middle the shape reaches: rays are only followed inside the sphere of that radius
//   float scene_distance(vec3 point, out float trap)
//                               how far `point` is from the shape (an estimate that is never more than the real distance), and a number from 0 up, `trap`, that tells the parts of the
//                               shape apart (how near the point's orbit came to the middle, or the like) and that the colour is made from
//
// and the inputs cameraTurn, cameraTilt, cameraDistance, cameraSpin, fieldOfView, maxSteps, detail, lightTurn, glowAmount, fogAmount, backgroundTop, backgroundBottom, colourScale,
// colourOffset, colourCycle, palette and samples (and colourA and colourB, for the palettes). This block has the main function.

// The direction that the light comes from.
vec3 light_direction()
{
	float angle = radians(lightTurn);
	return normalize(vec3(cos(angle) * 0.7, 0.8, sin(angle) * 0.7));
}

// The way the surface faces at `point`: the shape is looked at from the four corners of a small tetrahedron, and the corner that is farthest out is the way away from the shape.
vec3 surface_facing(vec3 point, float reach_of_the_look)
{
	vec2 corner = vec2(1.0, -1.0) * 0.57735 * reach_of_the_look;
	float trap;
	return normalize(
		corner.xyy * scene_distance(point + corner.xyy, trap)
		+ corner.yyx * scene_distance(point + corner.yyx, trap)
		+ corner.yxy * scene_distance(point + corner.yxy, trap)
		+ corner.xxx * scene_distance(point + corner.xxx, trap));
}

// What a ray from `eye` towards `direction` sees: the colour of the shape where it hits it, in the light, with fog, and otherwise the sky with a glow round the shape where the ray went near it.
// `pixel_angle` is how wide a pixel is, as an angle: the shape is let to be this far from the ray, in the angle of a pixel, and the ray stops there.
vec3 colour_seen_by_the_ray(vec3 eye, vec3 direction, float pixel_angle, float sky_height, float colour_time)
{
	vec3 sky = mix(backgroundBottom.rgb, backgroundTop.rgb, clamp(sky_height * 0.5 + 0.5, 0.0, 1.0));
	// Only the part of the ray inside the sphere that holds the shape is followed.
	float along_the_middle = dot(eye, direction);
	float inside_the_sphere = along_the_middle * along_the_middle - (dot(eye, eye) - SCENE_RADIUS * SCENE_RADIUS);
	if (inside_the_sphere < 0.0) {
		return sky;
	}
	float root = sqrt(inside_the_sphere);
	float travelled = max(-along_the_middle - root, 0.0);
	float last_place = -along_the_middle + root;
	float trap = 0.0;
	float nearest_pass = 1.0e10;
	float used_steps = 0.0;
	bool hit = false;
	for (int step_number = 0; step_number < 400; step_number++) {
		if (step_number >= maxSteps) {
			break;
		}
		float distance_to_the_shape = scene_distance(eye + direction * travelled, trap);
		float allowed = pixel_angle * travelled * detail + 1.0e-5;
		nearest_pass = min(nearest_pass, distance_to_the_shape / max(travelled, 0.01));
		if (distance_to_the_shape < allowed) {
			hit = true;
			used_steps = float(step_number);
			break;
		}
		// (A little less than the whole of the estimate, which is not exact: the shape is not stepped into.)
		travelled += distance_to_the_shape * 0.9;
		if (travelled > last_place) {
			break;
		}
	}
	// The glow round the shape: stronger the nearer the ray passed to it, and fading away towards the edge of the sphere that holds the shape (where rays are no longer followed).
	float nearest_to_the_middle = sqrt(max(dot(eye, eye) - along_the_middle * along_the_middle, 0.0)) / SCENE_RADIUS;
	vec3 halo = palette_colour(palette, 0.35 + colourOffset + colour_time) * glowAmount * exp(-nearest_pass * 14.0) * (1.0 - smoothstep(0.45, 1.0, nearest_to_the_middle));
	if (!hit) {
		return sky + halo;
	}
	vec3 place = eye + direction * travelled;
	vec3 facing = surface_facing(place, pixel_angle * travelled * detail * 0.5 + 1.0e-5);
	vec3 to_the_light = light_direction();
	float lit = max(dot(facing, to_the_light), 0.0);
	vec3 half_way = normalize(to_the_light - direction);
	float shine = pow(max(dot(facing, half_way), 0.0), 40.0);
	// Crevices need more steps to get into, so the number of steps says how closed in a place is.
	float open = 1.0 - clamp(used_steps / float(maxSteps), 0.0, 1.0);
	float shadowing = open * open * 0.8 + 0.2;
	vec3 paint = palette_colour(palette, sqrt(max(trap, 0.0)) * colourScale + colourOffset + colour_time);
	vec3 seen = paint * (0.18 + 0.9 * lit) * shadowing + vec3(shine * 0.35 * shadowing);
	// Fog: what is far away is the colour of the sky.
	float fog = 1.0 - exp(-travelled * fogAmount * 0.08);
	return mix(seen, sky, clamp(fog, 0.0, 1.0)) + halo * 0.25;
}

void main()
{
	float turn = radians(cameraTurn + cameraSpin * TIME);
	float tilt = radians(clamp(cameraTilt, -89.0, 89.0));
	vec3 eye = cameraDistance * vec3(cos(tilt) * sin(turn), sin(tilt), cos(tilt) * cos(turn));
	vec3 forward = normalize(-eye);
	vec3 right = normalize(cross(forward, vec3(0.0, 1.0, 0.0)));
	vec3 up = cross(right, forward);
	float focal = 1.0 / tan(radians(fieldOfView) * 0.5);
	float pixel_angle = 2.0 / (focal * RENDERSIZE.y);
	float colour_time = colourCycle * TIME;
	int across = samples;
	vec3 total = vec3(0.0);
	// (One loop over the samples, and not one in another: the software adapter of Windows stops on a shader of this kind that has two.)
	for (int sample_number = 0; sample_number < 4; sample_number++) {
		if (sample_number >= across * across) {
			break;
		}
		vec2 offset = (vec2(float(sample_number - (sample_number / across) * across), float(sample_number / across)) + 0.5) / float(across) - 0.5;
		vec2 screen = (gl_FragCoord.xy + offset - 0.5 * RENDERSIZE.xy) / (0.5 * RENDERSIZE.y);
		vec3 direction = normalize(forward * focal + right * screen.x + up * screen.y);
		total += colour_seen_by_the_ray(eye, direction, pixel_angle, screen.y, colour_time);
	}
	gl_FragColor = vec4(total / float(across * across), 1.0);
}
// --- shared: raymarch (end) ---
