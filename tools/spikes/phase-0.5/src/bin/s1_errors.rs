//! S1, part 2: how good are the error messages, and where do they point? (No GPU.)
use spikes::isf;

fn main() {
    let header = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"amount\", \"TYPE\": \"float\" } ] }*/\n";
    let cases: [(&str, &str); 9] = [
        (
            "unknown name",
            "void main() {\n  vec4 c = IMG_THIS_NORM_PIXEL(inputImage);\n  gl_FragColor = c * not_declared;\n}\n",
        ),
        (
            "type mismatch (vec3 = vec4)",
            "void main() {\n  vec3 c = IMG_THIS_NORM_PIXEL(inputImage);\n  gl_FragColor = vec4(c, 1.0);\n}\n",
        ),
        (
            "unknown function",
            "void main() {\n  gl_FragColor = vec4(frobnicate(amount));\n}\n",
        ),
        ("missing closing brace", "void main() {\n  gl_FragColor = vec4(1.0);\n"),
        (
            "wrong number of arguments",
            "void main() {\n  gl_FragColor = vec4(mix(1.0, 2.0));\n}\n",
        ),
        (
            "## in a macro of the author's",
            "#define PASTE(a, b) a##b\nvoid main() {\n  gl_FragColor = vec4(PASTE(1, 0));\n}\n",
        ),
        ("no main at all", "float f(float x) { return x; }\n"),
        (
            "sampler2D as a function parameter",
            "vec4 get(sampler2D img, vec2 p) { return texture2D(img, p); }\nvoid main() {\n  gl_FragColor = get(inputImage, isf_FragNormCoord);\n}\n",
        ),
        (
            "integer ISFVSN, duplicate keys, a trailing comma in the header are tolerated?",
            "void main() {\n  gl_FragColor = vec4(1.0);\n}\n",
        ),
    ];
    for (name, body) in cases {
        let source = if name.starts_with("integer") {
            format!("/*{{ \"ISFVSN\": 2, \"ISFVSN\": \"2.0\", \"INPUTS\": [ ], }}*/\n{body}")
        } else {
            format!("{header}{body}")
        };
        match isf::parse(&source) {
            Err(error) => println!("[{name}] HEADER: {error}"),
            Ok(parsed) => match isf::compile(&parsed) {
                Ok(_) => println!("[{name}] compiled"),
                Err(problems) => {
                    for problem in problems {
                        let line_text = problem
                            .line
                            .and_then(|l| source.lines().nth(l as usize - 1))
                            .unwrap_or("?");
                        println!(
                            "[{name}] line {:?} col {:?}: {}   <- file line says: {:?}",
                            problem.line, problem.column, problem.message, line_text
                        );
                    }
                }
            },
        }
    }
}
