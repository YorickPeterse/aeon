//! Functions for interacting with the Inko VM.
use crate::error::Error;
use libinko::config::Config;
use libinko::image::Image;
use libinko::vm::machine::Machine;

pub fn start(path: &str, arguments: &[String]) -> Result<i32, Error> {
    let mut config = Config::new();

    config.populate_from_env();

    let image = Image::load_file(config, path).map_err(|e| {
        Error::generic(format!("Failed to parse image {}: {}", path, e))
    })?;

    Machine::boot(image, arguments)
        .map(|state| state.current_exit_status())
        .map_err(Error::generic)
}
