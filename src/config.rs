use crate::{PigError, PigResult};
use serde::{Deserialize, Serialize};
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

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
    pub path: PathBuf,
    pub entries: Vec<ConfigEntry>,
}

impl Config {
    const FILE: &'static str = "pig.yaml";

    pub fn from_args() -> PigResult<Self> {
        let mut args = std::env::args();
        let (_, path) = (args.next().unwrap(), args.next());

        if let Some(path) = path {
            Self::from_file(path)
        } else {
            Self::from_cwd()
        }
    }

    fn from_cwd() -> PigResult<Self> {
        let mut path = std::env::current_dir()?.join(Self::FILE);

        while !path.exists() {
            if let Some(parent) = path.parent().and_then(|parent| parent.parent()) {
                path = parent.to_path_buf().join(Self::FILE);
            } else {
                return Err(PigError::ConfigNotFound(Self::FILE.into()));
            }
        }

        Self::from_file(path)
    }

    fn from_file<T: AsRef<Path>>(path: T) -> PigResult<Self> {
        let path = path.as_ref();
        let config = std::fs::read_to_string(path);

        match config {
            Ok(config) => Ok(Self {
                path: path.canonicalize()?,
                entries: serde_yaml::from_str::<Vec<ConfigEntry>>(&config)?,
            }
            .validate()?),
            Err(err) if err.kind() == ErrorKind::NotFound => {
                Err(PigError::ConfigNotFound(path.into()))
            }
            Err(err) => Err(err.into()),
        }
    }

    fn validate(mut self) -> PigResult<Self> {
        let folder = self.path.parent().unwrap();

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
