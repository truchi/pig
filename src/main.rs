//! 🦀 OpenAPI code generation 🐷
//!
//! # TODO
//! - [x] README
//! - [x] Parse CLI args
//! - [x] Resolve $refs
//! - [x] Render templates
//! - [x] Watch mode
//! - [x] Watch `openapi.yaml` dependencies
//! - [x] Clean output directory
//! - [ ] Run post generation command
//! - [ ] Template functions (cases, dbg, ...)
//! - [ ] Error handling
//! - [ ] Error reporting

mod config;
mod pig;
mod resolver;

use crate::{config::Config, pig::Pig};
use clap::Parser;
use colored::Colorize;
use std::path::PathBuf;

// const INFO: &str = "💡";
// const WARN: &str = "🚧";
const ERROR: &str = "🚨";

pub type PigResult<T> = Result<T, PigError>;

#[derive(thiserror::Error, Debug)]
pub enum PigError {
    #[error("Io: {0}")]
    Io(#[from] std::io::Error),

    #[error("Yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Tera: {0:#?}")]
    Tera(#[from] tera::Error),

    #[error("Walk: {0:#?}")]
    Walkk(#[from] walkdir::Error),

    #[error("Watch: {0:#?}")]
    Watch(#[from] notify::Error),

    #[error("Config not found: {0}")]
    ConfigNotFound(PathBuf),

    #[error("Not a file: {0}")]
    NotAFile(PathBuf),

    #[error("Not a directory: {0}")]
    NotADirectory(PathBuf),
}

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Watch mode
    #[arg(short, long)]
    watch: bool,

    /// Path of the `pig.yaml` file (leave empty to search upwards from the current directory)
    config: Option<PathBuf>,
}

pub fn main() {
    if let Err(err) = (|| Pig::oink(Config::new(Args::parse())?))() {
        println!("{ERROR} {}", err.to_string().red());

        std::process::exit(1);
    }
}
