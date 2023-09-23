#![allow(unused)]

mod config;
mod pig;
mod resolver;

use crate::{config::Config, pig::Pig};
use colored::Colorize;
use resolver::Resolver;
use std::path::PathBuf;

const INFO: &'static str = "💡";
const WARN: &'static str = "🚧";
const ERROR: &'static str = "🚨";

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

    #[error("Config not found: {0}")]
    ConfigNotFound(PathBuf),

    #[error("Not a file: {0}")]
    NotAFile(PathBuf),

    #[error("Not a directory: {0}")]
    NotADirectory(PathBuf),
}

fn main() {
    let resolved = Resolver::new(".ignore/openapi.yaml")
        .unwrap()
        .resolve()
        .unwrap();

    dbg!(&resolved);

    if let PigResult::Err(err) = (|| Ok(Pig::new()?.run()?))() {
        println!("{ERROR} {}", err.to_string().red());
        std::process::exit(1);
    }
}
