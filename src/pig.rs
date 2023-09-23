use super::{INFO, WARN};
use crate::{config::Config, resolver::Resolver, PigResult};
use colored::Colorize;
use openapiv3::OpenAPI;
use std::fs::{create_dir_all, write, File};
use tera::Tera;

#[derive(Debug)]
pub struct Pig {
    config: Config,
}

impl Pig {
    const JINJA: &'static str = ".jinja";

    pub fn new() -> PigResult<Self> {
        Ok(Self {
            config: Config::from_args()?,
        })
    }

    pub fn run(&self) -> PigResult<()> {
        println!(
            "{INFO} {} {}",
            "Using config file:".green(),
            self.config.path.display().to_string().blue(),
        );

        if self.config.entries.is_empty() {
            println!("{WARN} {}", "No entries found in config".yellow());
            return Ok(());
        }

        for config in &self.config.entries {
            // Resolve openapi
            let openapi = Resolver::new(&config.openapi)?.resolve()?;

            // Write context files
            write(
                config.output.as_path().join(".pig.context.json"),
                serde_json::to_string_pretty(&openapi)?,
            )?;
            write(
                config.output.as_path().join(".pig.context.yaml"),
                serde_yaml::to_string(&openapi)?,
            )?;

            // Setup tera
            let context = tera::Context::from_value(openapi)?;
            let tera = Tera::new(
                &config
                    .input
                    .as_path()
                    .join(format!("**/*{}", Self::JINJA))
                    .display()
                    .to_string(),
            )?;

            // Render templates
            for template in tera.get_template_names() {
                let path = config.output.as_path().join({
                    // Remove .jinja extension
                    let len = template.len() - Self::JINJA.len();
                    &template[..len]
                });

                create_dir_all(path.parent().unwrap())?;
                tera.render_to(template, &context, File::create(path)?)?;
            }
        }

        Ok(())
    }
}
