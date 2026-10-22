/*{
	"DESCRIPTION": "a filter that wants a second picture of its own, which this program cannot give a shader yet (an input that is an image and none of inputImage, startImage and endImage)",
	"ISFVSN": "2.0",
	"CATEGORIES": ["Color Effect"],
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "maskImage", "TYPE": "image" }
	]
}*/

void main() {
	vec2 uv = isf_FragNormCoord;
	gl_FragColor = IMG_NORM_PIXEL(inputImage, uv) * IMG_NORM_PIXEL(maskImage, uv).r;
}
