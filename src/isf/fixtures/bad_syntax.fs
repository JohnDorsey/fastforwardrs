/*{
	"DESCRIPTION": "a missing semicolon at the end of line 11 (the compiler notices it at the start of line 12), and an unknown name on line 13",
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "amount", "TYPE": "float", "DEFAULT": 0.5 }
	]
}*/

void main() {
	vec4 c = IMG_THIS_NORM_PIXEL(inputImage);
	float k = amount * 2.0
	c.rgb *= k;
	gl_FragColor = c * not_declared_anywhere;
}
