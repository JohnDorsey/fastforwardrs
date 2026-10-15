/*{
	"DESCRIPTION": "a generator: no input image, moves with TIME",
	"ISFVSN": "2.0",
	"INPUTS": [
		{ "NAME": "speed", "TYPE": "float", "MIN": 0.0, "MAX": 4.0, "DEFAULT": 1.0 },
		{ "NAME": "scale", "TYPE": "float", "MIN": 1.0, "MAX": 20.0, "DEFAULT": 6.0 },
		{ "NAME": "colorA", "TYPE": "color", "DEFAULT": [0.9, 0.2, 0.1, 1.0] },
		{ "NAME": "colorB", "TYPE": "color", "DEFAULT": [0.1, 0.3, 0.9, 1.0] }
	]
}*/

void wave(in vec2 p, in float t, out float value) {
	value = 0.0;
	for (int i = 1; i < 5; i++) {
		float k = float(i);
		value += sin(p.x * k + t * k) / k + cos(p.y * k - t / k) / k;
	}
}

void main() {
	vec2 p = (gl_FragCoord.xy / RENDERSIZE.xy - 0.5) * scale;
	float t = TIME * speed;
	float v;
	wave(p, t, v);
	v = v * 0.25 + 0.5;
	gl_FragColor = mix(colorA, colorB, clamp(v, 0.0, 1.0));
}
