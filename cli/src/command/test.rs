//! Command for running unit tests.
use crate::command::run;
use crate::config::{MODULE_SEPARATOR, SOURCE_FILE_EXT};
use crate::error::Error;
use crate::options::print_usage;
use getopts::Options;
use std::env;
use std::path::{Path, PathBuf, MAIN_SEPARATOR};

const USAGE: &str = "Usage: inko test [OPTIONS]

Runs unit tests in a directory.

Unit tests are Inko source files that start with the `test_` prefix. This
command will gather all such files and run them. This command expects tests to
be located in a test/ directory, inside a tests/ direcory known as the tests
root. This tests root directory can be changed using the -d/--directory option,
provided the directory contains a test/ directory.

Examples:

    inko test                          # Runs all unit tests in ./tests/test
    inko test -d foo                   # Runs all unit tests in ./foo/test
    inko test tests/test/test_foo.inko # Only runs ./tests/test/test_foo.inko

Output formats:

    pretty (default)
    json";

/// The default root directory (relative to the current working directory) for
/// running unit tests.
const DEFAULT_TESTS_ROOT: &str = "tests";

/// The name of the directory that contains unit tests.
const TEST_DIRECTORY: &str = "test";

/// The name of the configuration module.
const CONFIG_MODULE: &str = "config";

/// The file used for configuring unit tests.
const CONFIG_FILE: &str = "config.inko";

/// Compiles and runs Inko unit tests.
pub fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Shows this help message");
    options.optopt(
        "d",
        "directory",
        "The path to the directory containing all unit tests",
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

    let cwd = env::current_dir()?;
    let root_dir = if let Some(path) = matches.opt_str("d") {
        cwd.join(path)
    } else {
        cwd.join(DEFAULT_TESTS_ROOT)
    };

    let test_dir = root_dir.join(TEST_DIRECTORY);

    if !test_dir.is_dir() {
        return Err(Error::generic(format!(
            "The directory `{}` must contain a `{}` directory",
            root_dir.to_string_lossy(),
            TEST_DIRECTORY
        )));
    }

    let has_config = test_dir.join(CONFIG_FILE).is_file();

    // If the user provides extra arguments, we'll treat these as specific test
    // modules to run.
    let modules = if !matches.free.is_empty() {
        let mut mods = Vec::with_capacity(matches.free.len());

        for path in &matches.free {
            mods.push(module_name(
                PathBuf::from(path).canonicalize()?,
                &root_dir,
            )?);
        }

        mods
    } else {
        test_modules(&root_dir)?
    };

    let source = generate_source(modules, has_config);

    run::run_eval(
        &source,
        vec![root_dir.to_string_lossy().to_string()],
        matches.opt_str("f"),
        &matches.free,
    )
}

/// Returns the full module names of all unit tests.
fn test_modules(directory: &Path) -> Result<Vec<String>, Error> {
    let mut mods = Vec::new();
    let mut dirs = vec![directory.to_path_buf()];

    while let Some(dir) = dirs.pop() {
        let contents = dir.read_dir()?;

        for entry_res in contents {
            let entry = entry_res?;
            let ftype = entry.file_type()?;

            if ftype.is_dir() {
                dirs.push(entry.path());
                continue;
            }

            if !ftype.is_file() {
                // Symbolic links are not supported, at least for the time
                // being.
                continue;
            }

            let path = entry.path();

            if !path
                .file_name()
                .map(|p| p.to_string_lossy().starts_with("test_"))
                .unwrap_or(false)
            {
                continue;
            }

            if !path
                .extension()
                .map(|e| e == SOURCE_FILE_EXT)
                .unwrap_or(false)
            {
                continue;
            }

            mods.push(module_name(path, directory)?);
        }
    }

    Ok(mods)
}

fn module_name(path: PathBuf, directory: &Path) -> Result<String, Error> {
    let mut mod_path = path
        .strip_prefix(directory)
        .unwrap_or(&path)
        .to_string_lossy()
        .replace(MAIN_SEPARATOR, MODULE_SEPARATOR);

    // This removes the trailing source extension, without the need
    // for allocating another String.
    mod_path.truncate(mod_path.len() - (SOURCE_FILE_EXT.len() + 1));

    Ok(mod_path)
}

/// Generates Inko source code used for running unit tests.
///
/// The resulting source code will look along the lines of the following:
///
///     import std::test::Tests
///     import test::config
///     import test::foo::bar::(self as mod0)
///     import test::foo::baz::(self as mod1)
///
///     let tests = Tests.new
///
///     config.setup(tests)
///     mod0.tests(tests)
///     mod1.tests(tests)
///     tests.run
fn generate_source(modules: Vec<String>, has_config: bool) -> String {
    let mut output = "import std::test::Tests\n".to_string();

    if has_config {
        output.push_str(&format!(
            "import {}{}{}\n",
            TEST_DIRECTORY, MODULE_SEPARATOR, CONFIG_MODULE
        ));
    }

    for (index, module) in modules.iter().enumerate() {
        output.push_str(&format!(
            "import {}{}(self as mod{})\n",
            module, MODULE_SEPARATOR, index
        ));
    }

    output.push_str(&"let tests = Tests.new\n");

    if has_config {
        output.push_str(&format!("{}.setup(tests)\n", CONFIG_MODULE));
    }

    for index in 0..modules.len() {
        output.push_str(&format!("mod{}.tests(tests)\n", index));
    }

    output.push_str("tests.run\n");
    output
}
