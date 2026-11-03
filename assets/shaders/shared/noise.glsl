// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

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
