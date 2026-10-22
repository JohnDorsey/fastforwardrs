/*{
	"DESCRIPTION": "darkens the edges",
	"ISFVSN": "2.0",
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "center", "TYPE": "point2D", "DEFAULT": [960.0, 540.0], "MIN": [0.0, 0.0], "MAX": [1920.0, 1080.0] },
		{ "NAME": "radius", "TYPE": "float", "MIN": 0.0, "MAX": 2.0, "DEFAULT": 0.6 },
		{ "NAME": "softness", "TYPE": "float", "MIN": 0.01, "MAX": 1.0, "DEFAULT": 0.4 },
		{ "NAME": "edgeColor", "TYPE": "color", "DEFAULT": [0.0, 0.0, 0.0, 1.0] }
	]
}*/

void main() {
	vec2 uv = isf_FragNormCoord;
	float aspect = RENDERSIZE.x / RENDERSIZE.y;
	vec2 d = (uv - center / RENDERSIZE) * vec2(aspect, 1.0);
	float dist = length(d);
	float amount = smoothstep(radius, radius + softness, dist);
	vec4 src = IMG_NORM_PIXEL(inputImage, uv);
	gl_FragColor = mix(src, edgeColor, amount * edgeColor.a);
}
