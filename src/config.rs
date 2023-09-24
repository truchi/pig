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
    pub file: PathBuf,
    pub entries: Vec<ConfigEntry>,
}

impl Config {
    const FILE: &'static str = "pig.yaml";

    pub fn new() -> PigResult<Self> {
        Self::read(Self::find()?)
    }

    pub fn find() -> PigResult<PathBuf> {
        let mut args = std::env::args();
        let (_, path) = (args.next(), args.next());

        if let Some(path) = path.map(PathBuf::from) {
            if path.is_file() {
                Ok(path)
            } else {
                Err(PigError::NotAFile(path))
            }
        } else {
            let mut path = std::env::current_dir()?.join(Self::FILE);

            while !path.exists() {
                if let Some(parent) = path.parent().and_then(|parent| parent.parent()) {
                    path = parent.to_path_buf().join(Self::FILE);
                } else {
                    return Err(PigError::ConfigNotFound(Self::FILE.into()));
                }
            }

            Ok(path)
        }
    }

    pub fn read<T: AsRef<Path>>(file: T) -> PigResult<Self> {
        let file = file.as_ref();
        let config = std::fs::read_to_string(file);

        match config {
            Ok(config) => Ok(Self {
                file: file.canonicalize()?,
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
