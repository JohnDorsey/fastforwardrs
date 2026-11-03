// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

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
