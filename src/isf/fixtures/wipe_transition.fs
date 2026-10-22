/*{
	"DESCRIPTION": "a transition: a soft diagonal wipe from startImage to endImage",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Transition"],
	"INPUTS": [
		{ "NAME": "startImage", "TYPE": "image" },
		{ "NAME": "endImage", "TYPE": "image" },
		{ "NAME": "progress", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.0 }
	]
}*/

void main() {
	vec2 uv = isf_FragNormCoord;
	float edge = (uv.x + uv.y) * 0.5;
	float mixAmount = smoothstep(progress - 0.1, progress + 0.1, 1.0 - edge * 1.0);
	vec4 a = vec4(0.0);
	vec4 b = vec4(0.0);
	// Sampling inside branches is the usual thing in ISF files and needs a sampling call that does not depend on derivatives.
	if (mixAmount < 1.0) {
		b = IMG_NORM_PIXEL(endImage, uv);
	}
	if (mixAmount > 0.0) {
		a = IMG_NORM_PIXEL(startImage, uv);
	}
	gl_FragColor = mix(b, a, mixAmount);
}
