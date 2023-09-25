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
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};
use tera::{Context, Tera};

#[derive(Debug)]
pub enum Pig {}

impl Pig {
    const JINJA: &'static str = ".jinja";

    pub fn oink(config: Config) -> PigResult<()> {
        if config.watch {
            Pig::watch(config)
        } else {
            Pig::run(config)
        }
    }

    fn run(config: Config) -> PigResult<()> {
        for config in &config.entries {
            let (_, context) = Pig::context(config)?;
            let tera = Pig::tera(config)?;

            Pig::render(config, &tera, &context)?;
        }

        Ok(())
    }

    fn watch(config: Config) -> PigResult<()> {
        Watcher::new(config)?.watch()
    }

    fn context(config: &ConfigEntry) -> PigResult<(HashSet<PathBuf>, Context)> {
        let (dependencies, openapi) = Resolver::new(&config.openapi)?.resolve()?;

        write(
            config.output.as_path().join(".pig.context.json"),
            serde_json::to_string_pretty(&openapi)?,
        )?;
        write(
            config.output.as_path().join(".pig.context.yaml"),
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

    fn render(config: &ConfigEntry, tera: &Tera, context: &Context) -> PigResult<()> {
        for template in tera.get_template_names() {
            let path = config.output.join({
                let len = template.len() - Pig::JINJA.len();
                &template[..len]
            });

            create_dir_all(path.parent().unwrap())?;
            tera.render_to(template, context, File::create(path)?)?;
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

    fn watch(mut self) -> PigResult<()> {
        for entry in &mut self.entries {
            entry.init()?;
        }

        self.config_watcher
            .watch(self.config.file.as_path(), RecursiveMode::Recursive)?;

        for event in self.receiver {
            match event {
                Event::Config(_) => return Self::new(Config::new(Args::parse())?)?.watch(),
                Event::Openapi(i, _) => self.entries[i].on_openapi()?,
                Event::Input(i, _) => self.entries[i].on_input()?,
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

    fn init(&mut self) -> PigResult<()> {
        (self.dependencies, self.context) = Pig::context(&self.config)?;
        self.tera = Pig::tera(&self.config)?;

        Pig::render(&self.config, &self.tera, &self.context)?;

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

        Pig::render(&self.config, &self.tera, &self.context)?;

        Ok(())
    }

    fn on_input(&mut self) -> PigResult<()> {
        self.tera = Pig::tera(&self.config)?;

        Pig::render(&self.config, &self.tera, &self.context)?;

        Ok(())
    }
}
