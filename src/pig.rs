use crate::{config::Config, PigResult};
use openapiv3::OpenAPI;
use tera::Tera;

#[derive(Debug)]
pub struct Pig {
    config: Config,
}

impl Pig {
    const JINJA: &'static str = ".jinja";

    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn run(&self) -> PigResult<()> {
        for config in &self.config.entries {
            let openapi =
                serde_yaml::from_reader::<_, OpenAPI>(std::fs::File::open(&config.openapi)?)?;
            dbg!(&openapi.paths.paths);
            let context = tera::Context::from_serialize(&openapi)?;
            dbg!(&context);
            let tera = Tera::new(
                &config
                    .input
                    .as_path()
                    .join(format!("**/*{}", Self::JINJA))
                    .display()
                    .to_string(),
            )?;

            for template in tera.get_template_names() {
                let path = config.output.as_path().join({
                    let len = template.len();
                    &template[..len - Self::JINJA.len()]
                });

                std::fs::create_dir_all(path.parent().unwrap())?;
                tera.render_to(template, &context, std::fs::File::create(path)?)?;
            }
        }

        Ok(())
    }
}
