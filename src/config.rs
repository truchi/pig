use crate::{PigError, PigResult};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Watch mode
    #[arg(short, long)]
    watch: bool,

    /// Path of the `pig.yaml` file (leave empty to search upwards from the current directory)
    config: Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConfigEntry {
    #[serde(rename = "api")]
    pub openapi: PathBuf,
    #[serde(rename = "in")]
    pub input: PathBuf,
    #[serde(rename = "out")]
    pub output: PathBuf,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub file: PathBuf,
    pub watch: bool,
    pub entries: Vec<ConfigEntry>,
}

impl Config {
    const FILE: &'static str = "pig.yaml";

    pub fn new() -> PigResult<Self> {
        let args = Args::parse();

        let file = if let Some(file) = args.config {
            if !file.is_file() {
                return Err(PigError::NotAFile(file));
            }

            file
        } else {
            let mut file = std::env::current_dir()?.join(Self::FILE);

            while !file.exists() {
                if let Some(parent) = file.parent().and_then(|parent| parent.parent()) {
                    file = parent.to_path_buf().join(Self::FILE);
                } else {
                    return Err(PigError::ConfigNotFound(Self::FILE.into()));
                }
            }

            file
        };

        let config = std::fs::read_to_string(&file);

        match config {
            Ok(config) => Ok(Self {
                file: file.canonicalize()?,
                watch: args.watch,
                entries: serde_yaml::from_str::<Vec<ConfigEntry>>(&config)?,
            }
            .validate()?),
            Err(err) if err.kind() == ErrorKind::NotFound => {
                Err(PigError::ConfigNotFound(file.into()))
            }
            Err(err) => Err(err.into()),
        }
    }

    fn read<T: AsRef<Path>>(file: T) -> PigResult<Self> {
        let file = file.as_ref();
        let config = std::fs::read_to_string(file);

        match config {
            Ok(config) => Ok(Self {
                file: file.canonicalize()?,
                watch: false,
                entries: serde_yaml::from_str::<Vec<ConfigEntry>>(&config)?,
            }
            .validate()?),
            Err(err) if err.kind() == ErrorKind::NotFound => {
                Err(PigError::ConfigNotFound(file.into()))
            }
            Err(err) => Err(err.into()),
        }
    }

    fn validate(mut self) -> PigResult<Self> {
        let folder = self.file.parent().unwrap();

        for entry in &mut self.entries {
            entry.openapi = {
                if entry.openapi.is_relative() {
                    entry.openapi = folder.join(&entry.openapi);
                }

                if !entry.openapi.is_file() {
                    return Err(PigError::NotAFile(entry.openapi.clone()));
                }

                entry.openapi.canonicalize()?
            };

            entry.input = {
                if entry.input.is_relative() {
                    entry.input = folder.join(&entry.input);
                }

                if !entry.input.is_dir() {
                    return Err(PigError::NotADirectory(entry.input.clone()));
                }

                entry.input.canonicalize()?
            };

            entry.output = {
                if entry.output.is_relative() {
                    entry.output = folder.join(&entry.output);
                }

                if entry.output.exists() {
                    if !entry.output.is_dir() {
                        return Err(PigError::NotADirectory(entry.output.clone()));
                    }
                } else {
                    std::fs::create_dir_all(&entry.output)?;
                }

                entry.output.canonicalize()?
            };
        }

        Ok(self)
    }
}
