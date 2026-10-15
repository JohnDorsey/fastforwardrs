/*{
	"DESCRIPTION": "fewer levels per channel; a long input with values and labels",
	"ISFVSN": "2.0",
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "levels", "TYPE": "long", "VALUES": [2, 4, 8, 16], "LABELS": ["2", "4", "8", "16"], "DEFAULT": 4 },
		{ "NAME": "dither", "TYPE": "bool", "DEFAULT": true }
	]
}*/

float checker(vec2 p) {
	return mod(floor(p.x) + floor(p.y), 2.0);
}

void main() {
	vec4 c = IMG_THIS_PIXEL(inputImage);
	float n = float(levels);
	vec3 rgb = c.rgb;
	if (dither) {
		rgb += (checker(gl_FragCoord.xy) - 0.5) / (n * 2.0);
	}
	rgb = floor(rgb * n + 0.5) / n;
	gl_FragColor = vec4(rgb, c.a);
}
