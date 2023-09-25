use crate::{
    config::{Config, ConfigEntry},
    resolver::Resolver,
    Args, PigResult,
};
use clap::Parser;
use notify::{event::DataChange, RecommendedWatcher, RecursiveMode, Watcher as _};
use std::{
    collections::HashSet,
    fs::{create_dir_all, write, File},
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, Sender},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tera::{Context, Tera};
use walkdir::WalkDir;

#[derive(Debug)]
pub enum Pig {}

impl Pig {
    const JINJA: &'static str = ".jinja";
    const JSON_CONTEXT: &'static str = ".pig.context.json";
    const YAML_CONTEXT: &'static str = ".pig.context.yaml";
    const TRASH: &'static str = ".pig.trash";

    pub fn oink(config: Config) -> PigResult<()> {
        if config.watch {
            Self::watch(config)
        } else {
            Self::run(config)
        }
    }

    fn run(config: Config) -> PigResult<()> {
        let data = config
            .entries
            .iter()
            .map(|entry| {
                let (_, context) = Pig::context(entry)?;
                let tera = Pig::tera(entry)?;

                Ok((entry, tera, context))
            })
            .collect::<PigResult<Vec<_>>>()?;

        Self::clean(
            &config,
            data.iter().map(|(config, tera, _)| (*config, tera)),
        )?;

        for (config, tera, context) in data {
            Self::render(config, &tera, &context)?;
        }

        Ok(())
    }

    fn watch(config: Config) -> PigResult<()> {
        Watcher::new(config)?.watch()
    }

    fn context(config: &ConfigEntry) -> PigResult<(HashSet<PathBuf>, Context)> {
        let (dependencies, openapi) = Resolver::new(&config.openapi)?.resolve()?;

        write(
            config.output.as_path().join(Self::JSON_CONTEXT),
            serde_json::to_string_pretty(&openapi)?,
        )?;
        write(
            config.output.as_path().join(Self::YAML_CONTEXT),
            serde_yaml::to_string(&openapi)?,
        )?;

        Ok((dependencies, Context::from_value(openapi)?))
    }

    fn tera(config: &ConfigEntry) -> PigResult<Tera> {
        Ok(Tera::new(
            &config
                .input
                .as_path()
                .join(format!("**/*{}", Self::JINJA))
                .display()
                .to_string(),
        )?)
    }

    fn output(config: &ConfigEntry, template: &str) -> PathBuf {
        let len = template.len() - Pig::JINJA.len();

        config.output.join(&template[..len])
    }

    fn clean<'a, T: IntoIterator<Item = (&'a ConfigEntry, &'a Tera)>>(
        config: &Config,
        it: T,
    ) -> PigResult<()> {
        let outputs = {
            let mut outputs = HashSet::new();

            for (config, tera) in it {
                for template in tera.get_template_names() {
                    let output = Self::output(config, template);

                    if !outputs.contains(&output) {
                        outputs.insert(output);
                    } else {
                        panic!("Conflicting output file: {}", output.display());
                    }
                }
            }

            outputs
        };

        let mut trash = {
            let trash = config.file.parent().unwrap().join(Self::TRASH).join(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis()
                    .to_string(),
            );
            let mut created = false;

            move |config: &ConfigEntry, path: &Path| {
                if !created {
                    create_dir_all(&trash)?;
                    created = true;
                }

                std::fs::rename(path, trash.join(path.strip_prefix(&config.output).unwrap()))?;

                PigResult::Ok(())
            }
        };

        for config in &config.entries {
            let json_context = config.output.join(Self::JSON_CONTEXT);
            let yaml_context = config.output.join(Self::YAML_CONTEXT);

            for result in WalkDir::new(&config.output).follow_links(true) {
                let entry = result?;

                if !entry.file_type().is_file()
                    || entry.path().starts_with(&json_context)
                    || entry.path().starts_with(&yaml_context)
                {
                    continue;
                }

                if !outputs.contains(entry.path()) {
                    trash(config, entry.path())?;
                }
            }
        }

        Ok(())
    }

    fn render(config: &ConfigEntry, tera: &Tera, context: &Context) -> PigResult<()> {
        for template in tera.get_template_names() {
            let output = Self::output(config, template);

            create_dir_all(output.parent().unwrap())?;
            tera.render_to(template, context, File::create(output)?)?;
        }

        Ok(())
    }
}

#[derive(Debug)]
enum Event {
    Config(DataChange),
    Openapi(usize, DataChange),
    Input(usize, DataChange),
}

pub struct Watcher {
    config: Config,
    config_watcher: RecommendedWatcher,
    receiver: Receiver<Event>,
    entries: Vec<WatcherEntry>,
}

impl Watcher {
    pub fn new(config: Config) -> PigResult<Self> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let config_watcher = RecommendedWatcher::new(
            Watcher::handler(sender.clone(), Event::Config),
            Watcher::config(),
        )?;
        let entries = config
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| WatcherEntry::new(entry.clone(), i, sender.clone()))
            .collect::<PigResult<_>>()?;

        Ok(Self {
            config,
            config_watcher,
            receiver,
            entries,
        })
    }

    fn handler(
        sender: Sender<Event>,
        f: impl Fn(DataChange) -> Event,
    ) -> impl Fn(Result<notify::Event, notify::Error>) {
        move |event: Result<notify::Event, notify::Error>| match event {
            Ok(event) => match event.kind {
                notify::EventKind::Any => {}
                notify::EventKind::Access(_) => {}
                notify::EventKind::Create(_) => {}
                notify::EventKind::Modify(modify) => match modify {
                    notify::event::ModifyKind::Any => {}
                    notify::event::ModifyKind::Data(data) => sender.send(f(data)).unwrap(),
                    notify::event::ModifyKind::Metadata(_) => {}
                    notify::event::ModifyKind::Name(_) => {}
                    notify::event::ModifyKind::Other => {}
                },
                notify::EventKind::Remove(_) => {}
                notify::EventKind::Other => {}
            },
            Err(error) => panic!("Error: {error:?}"),
        }
    }

    fn config() -> notify::Config {
        notify::Config::default().with_poll_interval(Duration::from_millis(200))
    }

    fn clean(&self) -> PigResult<()> {
        Pig::clean(
            &self.config,
            self.entries
                .iter()
                .map(|entry| (&entry.config, &entry.tera)),
        )?;

        Ok(())
    }

    fn watch(mut self) -> PigResult<()> {
        self.config_watcher
            .watch(self.config.file.as_path(), RecursiveMode::Recursive)?;

        for entry in &mut self.entries {
            entry.watch()?;
        }

        self.clean()?;

        for entry in &mut self.entries {
            entry.render()?;
        }

        for event in &self.receiver {
            match event {
                Event::Config(_) => return Self::new(Config::new(Args::parse())?)?.watch(),
                Event::Openapi(i, _) => {
                    self.entries[i].on_openapi()?;
                    self.clean()?;
                    self.entries[i].render()?;
                }
                Event::Input(i, _) => {
                    self.entries[i].on_input()?;
                    self.clean()?;
                    self.entries[i].render()?;
                }
            }
        }

        Ok(())
    }
}

pub struct WatcherEntry {
    config: ConfigEntry,
    openapi_watcher: RecommendedWatcher,
    input_watcher: RecommendedWatcher,
    dependencies: HashSet<PathBuf>,
    context: Context,
    tera: Tera,
}

impl WatcherEntry {
    fn new(config: ConfigEntry, index: usize, sender: Sender<Event>) -> PigResult<Self> {
        Ok(Self {
            config,
            openapi_watcher: RecommendedWatcher::new(
                Watcher::handler(sender.clone(), move |event| Event::Openapi(index, event)),
                Watcher::config(),
            )?,
            input_watcher: RecommendedWatcher::new(
                Watcher::handler(sender.clone(), move |event| Event::Input(index, event)),
                Watcher::config(),
            )?,
            dependencies: Default::default(),
            context: Default::default(),
            tera: Default::default(),
        })
    }

    fn watch(&mut self) -> PigResult<()> {
        (self.dependencies, self.context) = Pig::context(&self.config)?;
        self.tera = Pig::tera(&self.config)?;

        for dependency in &self.dependencies {
            self.openapi_watcher
                .watch(dependency, RecursiveMode::Recursive)?;
        }

        self.input_watcher
            .watch(&self.config.input, RecursiveMode::Recursive)?;

        Ok(())
    }

    fn on_openapi(&mut self) -> PigResult<()> {
        for dependency in &self.dependencies {
            self.openapi_watcher.unwatch(dependency)?;
        }

        (self.dependencies, self.context) = Pig::context(&self.config)?;

        for dependency in &self.dependencies {
            self.openapi_watcher
                .watch(dependency, RecursiveMode::Recursive)?;
        }

        Ok(())
    }

    fn on_input(&mut self) -> PigResult<()> {
        self.tera = Pig::tera(&self.config)?;

        Ok(())
    }

    fn render(&self) -> PigResult<()> {
        Pig::render(&self.config, &self.tera, &self.context)?;

        Ok(())
    }
}
