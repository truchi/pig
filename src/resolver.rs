use crate::PigResult;
use openapiv3::OpenAPI;
use serde_json::{Map, Value as Json};
use serde_yaml::Value as Yaml;
use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    str::FromStr,
};

#[derive(Clone, Eq, PartialEq, Debug)]
struct Reference {
    file: PathBuf,
    keys: Vec<String>,
}

impl Reference {
    fn new<T: AsRef<Path>>(current: T, str: &str) -> PigResult<Self> {
        let current = current.as_ref();
        debug_assert!(current == current.canonicalize()?);
        debug_assert!(current.is_file());

        let (file, keys) = {
            let mut split = str.split('#');

            (
                split.next().expect("Empty reference"),
                split.next().expect("Empty reference"),
            )
        };

        let file = file.trim();
        let file = if file.is_empty() {
            current.to_path_buf()
        } else {
            let base = current.parent().unwrap();
            let file: &Path = file.as_ref();

            file.is_relative()
                .then(|| base.join(file))
                .unwrap_or_else(|| file.to_path_buf())
        }
        .canonicalize()?;

        let keys = keys
            .split('/')
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(Into::into)
            .collect();

        Ok(Self { file, keys })
    }

    fn display(&self, end: usize) -> String {
        format!("{}#/{}", self.file.display(), self.keys[..end].join("/"))
    }
}

impl std::fmt::Display for Reference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}#/{}",
            self.file.display(),
            self.keys.join("/").as_str(),
        )
    }
}

#[derive(Default, Debug)]
pub struct Resolver {
    file: PathBuf,
    files: HashMap<PathBuf, Json>,
}

impl Resolver {
    pub fn new<T: AsRef<Path>>(file: T) -> PigResult<Self> {
        let mut resolver = Self {
            file: file.as_ref().canonicalize()?,
            files: HashMap::new(),
        };
        resolver.load(resolver.file.clone())?;

        Ok(resolver)
    }

    pub fn resolve(mut self) -> PigResult<Json> {
        fn resolve(
            resolver: &mut Resolver,
            value: &mut Json,
            references: &mut Vec<Reference>,
        ) -> PigResult<()> {
            match value {
                Json::Null | Json::Bool(_) | Json::Number(_) | Json::String(_) => {}
                Json::Array(values) => {
                    for value in values {
                        resolve(resolver, value, references)?;
                    }
                }
                Json::Object(object) => {
                    if let Some(reference) = object.get("$ref") {
                        assert!(
                            object.len() == 1,
                            "Invalid $ref object: contains more keys ({})",
                            object
                                .keys()
                                .map(String::as_str)
                                .filter(|key| *key != "$ref")
                                .collect::<Vec<_>>()
                                .join(", "),
                        );

                        let reference = Reference::new(
                            references
                                .last()
                                .map(|reference| &reference.file)
                                .unwrap_or(&resolver.file),
                            reference.as_str().expect("$ref is not a string"),
                        )?;

                        if references.contains(&reference) {
                            references.push(reference);
                            panic!(
                                "Circular reference detected: {}",
                                references
                                    .iter()
                                    .map(ToString::to_string)
                                    .collect::<Vec<_>>()
                                    .join(" -> ")
                            );
                        }

                        let extension = Map::from_iter([
                            ("$ref".into(), Json::String(reference.to_string())),
                            (
                                "$file".into(),
                                Json::String(reference.file.display().to_string()),
                            ),
                            (
                                "$keys".into(),
                                Json::Array(
                                    reference
                                        .keys
                                        .iter()
                                        .map(|key| key.as_str().into())
                                        .collect(),
                                ),
                            ),
                            (
                                "$name".into(),
                                Json::String(
                                    reference
                                        .keys
                                        .last()
                                        .expect("Empty reference keys")
                                        .to_string(),
                                ),
                            ),
                        ]);

                        *value = {
                            let mut value = resolver.load(&reference.file)?;

                            for (i, key) in reference.keys.iter().enumerate() {
                                value = value.get(key).unwrap_or_else(|| {
                                    panic!("$ref not found: {}", reference.display(i + 1))
                                });
                            }

                            let mut value = value.clone();

                            references.push(reference);
                            resolve(resolver, &mut value, references)?;
                            references.pop();

                            let mut object =
                                value.as_object_mut().expect("$ref is not a YAML object");

                            for key in object.keys() {
                                if extension.contains_key(key) {
                                    panic!("Reference contains {key}");
                                }
                            }

                            object.extend(extension);

                            value
                        };
                    } else {
                        for value in object.values_mut() {
                            resolve(resolver, value, references)?;
                        }
                    }
                }
            }

            Ok(())
        }

        let mut output = self.files.get(&self.file).unwrap().clone();
        resolve(&mut self, &mut output, &mut Vec::new())?;

        Ok(output)
    }
}

impl Resolver {
    fn load<T: AsRef<Path>>(&mut self, file: T) -> PigResult<&Json> {
        let file = file.as_ref();
        let file = if file.is_relative() {
            self.file.parent().unwrap().join(file)
        } else {
            file.to_path_buf()
        }
        .canonicalize()?;

        if !self.files.contains_key(&file) {
            // After the main file is loaded, we will get the OpenAPI version
            let value = if let Some(openapi) = {
                self.files
                    .get(&self.file)
                    .and_then(|value| value.get("openapi"))
                    .and_then(|version| version.as_str())
            } {
                let value = serde_yaml::from_reader::<_, Json>(File::open(&file)?)?;

                // We allow omitting the mandatory fields in other files
                {
                    let mut value = value.clone();

                    if let Some(object) = value.as_object_mut() {
                        object.insert("openapi".into(), openapi.into());
                        object.insert(
                            "info".into(),
                            Json::from_iter([
                                ("title".to_string(), String::new()),
                                ("version".to_string(), String::new()),
                            ]),
                        );
                        object.insert("paths".into(), Json::Object(Default::default()));
                    }

                    // Make sure the file deserializes correctly into OpenAPI
                    serde_json::from_value::<OpenAPI>(value)?;
                }

                value
            } else {
                // Make sure the file deserializes correctly into OpenAPI
                let value = serde_yaml::from_reader::<_, OpenAPI>(File::open(&file)?)?;

                serde_json::to_value(value)?
            };

            self.files.insert(file.clone(), value);
        }

        Ok(self.files.get(&file).unwrap())
    }
}
