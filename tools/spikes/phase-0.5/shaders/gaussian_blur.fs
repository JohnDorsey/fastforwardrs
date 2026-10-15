/*{
	"DESCRIPTION": "a two-pass blur: the first pass blurs along x into a buffer, the second blurs that along y",
	"ISFVSN": "2.0",
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "radius", "TYPE": "float", "MIN": 0.0, "MAX": 30.0, "DEFAULT": 8.0 }
	],
	"PASSES": [
		{ "TARGET": "bufferHorizontal" },
		{ }
	]
}*/

const float weights[5] = float[5](0.2270270270, 0.1945945946, 0.1216216216, 0.0540540541, 0.0162162162);

void main() {
	vec2 step_size = vec2(radius / 4.0) / RENDERSIZE;
	vec4 sum = vec4(0.0);
	if (PASSINDEX == 0) {
		sum = IMG_NORM_PIXEL(inputImage, isf_FragNormCoord) * weights[0];
		for (int i = 1; i < 5; i++) {
			vec2 offset = vec2(float(i) * step_size.x, 0.0);
			sum += IMG_NORM_PIXEL(inputImage, isf_FragNormCoord + offset) * weights[i];
			sum += IMG_NORM_PIXEL(inputImage, isf_FragNormCoord - offset) * weights[i];
		}
	} else {
		sum = IMG_NORM_PIXEL(bufferHorizontal, isf_FragNormCoord) * weights[0];
		for (int i = 1; i < 5; i++) {
			vec2 offset = vec2(0.0, float(i) * step_size.y);
			sum += IMG_NORM_PIXEL(bufferHorizontal, isf_FragNormCoord + offset) * weights[i];
			sum += IMG_NORM_PIXEL(bufferHorizontal, isf_FragNormCoord - offset) * weights[i];
		}
	}
	gl_FragColor = sum;
}
