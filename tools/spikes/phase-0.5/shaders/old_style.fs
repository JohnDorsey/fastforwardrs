/*
{
	"DESCRIPTION": "the old idioms: #version, precision, varying, vv_FragNormCoord, texture2D, texture2DRect, IMG_PIXEL, a macro function, gl_FragColor parts",
	"ISFVSN": 2,
	"ISFVSN": "2.0",
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "amount", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 1.0 }
	]
}
*/
#version 120
precision highp float;
varying vec2 left_over_varying;

#define LUMA(c) dot((c).rgb, vec3(0.299, 0.587, 0.114))

void main() {
	vec2 uv = vv_FragNormCoord;
	vec4 a = texture2D(inputImage, uv);
	vec4 b = texture2DRect(inputImage, uv * IMG_SIZE(inputImage));
	vec4 c = IMG_PIXEL(inputImage, gl_FragCoord.xy + vec2(1.0, 0.0));
	float l = LUMA(a);
	gl_FragColor.rgb = mix(a.rgb, vec3(l), amount) * 0.5 + b.rgb * 0.25 + c.rgb * 0.25;
	gl_FragColor.a = 1;
}
