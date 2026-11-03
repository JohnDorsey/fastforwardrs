// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

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
