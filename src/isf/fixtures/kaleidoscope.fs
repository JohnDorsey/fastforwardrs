/*{
	"DESCRIPTION": "mirrors the picture around a point",
	"ISFVSN": "2.0",
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "segments", "TYPE": "float", "MIN": 2.0, "MAX": 24.0, "DEFAULT": 6.0 },
		{ "NAME": "angle", "TYPE": "float", "MIN": 0.0, "MAX": 6.2832, "DEFAULT": 0.3 },
		{ "NAME": "center", "TYPE": "point2D", "DEFAULT": [960.0, 540.0] }
	]
}*/

#define PI 3.14159265358979

mat2 rotation(float a) {
	float s = sin(a);
	float c = cos(a);
	return mat2(c, -s, s, c);
}

void main() {
	vec2 p = (isf_FragNormCoord - center / RENDERSIZE) * vec2(RENDERSIZE.x / RENDERSIZE.y, 1.0);
	float r = length(p);
	float a = atan(p.y, p.x) + angle;
	float slice = 2.0 * PI / segments;
	a = mod(a, slice);
	a = abs(a - slice / 2.0);
	vec2 q = r * vec2(cos(a), sin(a));
	q = rotation(-angle) * q;
	q = q / vec2(RENDERSIZE.x / RENDERSIZE.y, 1.0) + center / RENDERSIZE;
	gl_FragColor = IMG_NORM_PIXEL(inputImage, q);
}
