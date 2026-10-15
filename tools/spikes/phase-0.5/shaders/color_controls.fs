/*{
	"DESCRIPTION": "brightness, contrast, saturation, invert and tint (written for the spike, in the style of the Vidvox examples)",
	"CREDIT": "spike",
	"ISFVSN": "2",
	"CATEGORIES": ["Color Adjustment"],
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "brightness", "TYPE": "float", "MIN": -1.0, "MAX": 1.0, "DEFAULT": 0.0 },
		{ "NAME": "contrast", "TYPE": "float", "MIN": 0.0, "MAX": 4.0, "DEFAULT": 1.0 },
		{ "NAME": "saturation", "TYPE": "float", "MIN": 0.0, "MAX": 4.0, "DEFAULT": 1.0 },
		{ "NAME": "invert", "TYPE": "bool", "DEFAULT": false },
		{ "NAME": "tint", "TYPE": "color", "DEFAULT": [1.0, 1.0, 1.0, 1.0] }
	]
}*/

const vec3 luma_weights = vec3(0.2125, 0.7154, 0.0721);

void main() {
	vec4 src = IMG_THIS_NORM_PIXEL(inputImage);
	vec3 rgb = src.rgb;
	rgb = rgb + brightness;
	rgb = (rgb - 0.5) * contrast + 0.5;
	float grey = dot(rgb, luma_weights);
	rgb = mix(vec3(grey), rgb, saturation);
	if (invert) {
		rgb = 1.0 - rgb;
	}
	rgb = rgb * tint.rgb;
	gl_FragColor = vec4(clamp(rgb, 0.0, 1.0), src.a * tint.a);
}
