mod config;
mod pig;

use crate::{config::Config, pig::Pig};
use colored::Colorize;
use std::path::PathBuf;

const INFO: &'static str = "💡";
const WARN: &'static str = "🚧";
const ERROR: &'static str = "🚨";

pub type PigResult<T> = Result<T, PigError>;

#[derive(thiserror::Error, Debug)]
pub enum PigError {
    #[error("Io: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serde: {0}")]
    Serde(#[from] serde_yaml::Error),

    #[error("Tera: {0:#?}")]
    Tera(#[from] tera::Error),

    #[error("Config not found: {0}")]
    ConfigNotFound(PathBuf),

    #[error("Not a file: {0}")]
    NotAFile(PathBuf),

    #[error("Not a directory: {0}")]
    NotADirectory(PathBuf),
}

fn main() {
    if let Err(err) = main2() {
        println!("{ERROR} {}", err.to_string().red());
        std::process::exit(1);
    }
}

fn main2() -> PigResult<()> {
    let config = Config::from_args()?;

    println!(
        "{INFO} {} {}",
        "Using config file:".green(),
        config.path.display().to_string().blue(),
    );

    if config.entries.is_empty() {
        println!("{WARN} {}", "No entries found in config".yellow());
        return Ok(());
    }

    Pig::new(config).run()?;

    Ok(())
}
