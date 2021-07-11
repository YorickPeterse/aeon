//! Command for compiling and running Inko source code or bytecode images.
use crate::compiler;
use crate::config::{BYTECODE_IMAGE_EXT, SOURCE_FILE_EXT};
use crate::error::Error;
use crate::options::print_usage;
use crate::tempfile::Tempfile;
use crate::vm;
use getopts::Options;

const USAGE: &str = "Usage: inko run [OPTIONS] [FILE]

Compiles and runs an Inko source file, or runs an existing bytecode image.

If the file is a source file (its extension is .inko), the file is first
compiled into a bytecode image. If the file is a bytecode image (its extension
is .ibi), the file is run directly.

When compiling a source file, this command will spawn a subprocess to run the
compiler. Once compiled, it will run the resulting bytecode image.

Using source files is meant for development and scripting, and comes with the
overhead of having to run the compiler. For production environments it's
recommended to compile and run your program separately. For example:

    inko build hello.inko    # Produces ./hello.ibi
    inko run hello.ibi       # Runs the program

Examples:

    inko run hello.inko    # Compiles and runs the file hello.inko
    inko run hello.ibi     # Runs the bytecode image directly

Output formats:

    pretty (default)
    json";

/// Compiles and runs Inko source code or a bytecode image.
pub fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Shows this help message");
    options.optopt("e", "eval", "Evaluates the string", "STRING");
    options.optmulti(
        "i",
        "include",
        "Adds the directory to the list of source directories",
        "DIR",
    );

    options.optopt(
        "f",
        "format",
        "The output format to use for diagnostics",
        "FORMAT",
    );

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    if let Some(source) = matches.opt_str("e") {
        run_eval(
            &source,
            matches.opt_strs("i"),
            matches.opt_str("f"),
            &matches.free,
        )
    } else if let Some(input) = matches.free.get(0) {
        run_file(
            input,
            matches.opt_strs("i"),
            matches.opt_str("f"),
            &matches.free[1..],
        )
    } else {
        Err(Error::generic(
            "You must specify an input file or the --eval option".to_string(),
        ))
    }
}

/// Runs an Inko source file or a bytecode image.
fn run_file(
    input: &str,
    include: Vec<String>,
    format: Option<String>,
    arguments: &[String],
) -> Result<i32, Error> {
    if input.ends_with(BYTECODE_IMAGE_EXT) {
        vm::start(input, arguments)
    } else {
        let image = Tempfile::new(BYTECODE_IMAGE_EXT)?;

        compile(input, image.path(), include, format)?;
        vm::start(image.path(), arguments)
    }
}

/// Runs Inko source code that is provided directly, instead of through a file.
///
/// This method is public so the "test" command can reuse it.
pub fn run_eval(
    source: &str,
    include: Vec<String>,
    format: Option<String>,
    arguments: &[String],
) -> Result<i32, Error> {
    let mut input = Tempfile::new(SOURCE_FILE_EXT)?;
    let image = Tempfile::new(BYTECODE_IMAGE_EXT)?;

    input.write(source.as_bytes())?;
    input.flush();

    compile(input.path(), image.path(), include, format)?;
    vm::start(image.path(), arguments)
}

/// Compiles the source code in the given input path, producing a bytecode image
/// stored in the output path.
fn compile(
    input: &str,
    output: &str,
    include: Vec<String>,
    format: Option<String>,
) -> Result<i32, Error> {
    let mut args =
        vec!["-o".to_string(), output.to_string(), input.to_string()];

    if let Some(format) = format {
        args.push("--format".to_string());
        args.push(format);
    }

    for directory in include {
        args.push("-i".to_string());
        args.push(directory);
    }

    compiler::spawn(&args)
}
