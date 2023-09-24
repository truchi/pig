use super::{INFO, WARN};
use crate::{
    config::{Config, ConfigEntry},
    resolver::Resolver,
    PigResult,
};
use colored::Colorize;
use notify::{event::DataChange, RecommendedWatcher, RecursiveMode, Watcher as _};
use openapiv3::OpenAPI;
use serde_json::Value as Json;
use std::{
    fs::{create_dir_all, write, File},
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};
use tera::{Context, Tera};

#[derive(Debug)]
pub enum Pig {}

impl Pig {
    const JINJA: &'static str = ".jinja";

    pub fn oink() -> PigResult<()> {
        let config = Config::new()?;

        if config.watch {
            Pig::watch(config)
        } else {
            Pig::run(config)
        }
    }

    fn run(config: Config) -> PigResult<()> {
        for config in &config.entries {
            let context = Pig::context(config)?;
            let tera = Pig::tera(config)?;

            Pig::render(config, &tera, &context)?;
        }

        Ok(())
    }

    fn watch(config: Config) -> PigResult<()> {
        Watcher::new(config)?.watch()
    }

    fn context(config: &ConfigEntry) -> PigResult<Context> {
        let openapi = Resolver::new(&config.openapi)?.resolve()?;

        write(
            config.output.as_path().join(".pig.context.json"),
            serde_json::to_string_pretty(&openapi)?,
        )?;
        write(
            config.output.as_path().join(".pig.context.yaml"),
            serde_yaml::to_string(&openapi)?,
        )?;

        Ok(Context::from_value(openapi)?)
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
            let path = config
                .output
                .join((|len| &template[..len])(template.len() - Pig::JINJA.len()));

            create_dir_all(path.parent().unwrap())?;
            tera.render_to(template, &context, File::create(path)?)?;
        }

        Ok(())
    }
}

#[derive(Debug)]
enum Event {
    ConfigModifyData(DataChange),
    OpenapiModifyData(usize, DataChange),
    InputModifyData(usize, DataChange),
}

pub struct Watcher {
    config: Config,
    config_watcher: RecommendedWatcher,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    entries: Vec<WatcherEntry>,
}

impl Watcher {
    pub fn new(config: Config) -> PigResult<Self> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut config_watcher = RecommendedWatcher::new(
            Watcher::handler(sender.clone(), Event::ConfigModifyData),
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
            sender,
            receiver,
            entries,
        })
    }

    fn handler(
        sender: Sender<Event>,
        modify_data_event: impl Fn(DataChange) -> Event,
    ) -> impl Fn(Result<notify::Event, notify::Error>) {
        move |notify_event: Result<notify::Event, notify::Error>| match notify_event {
            Ok(notify_event) => match notify_event.kind {
                notify::EventKind::Any => {}
                notify::EventKind::Access(_) => {}
                notify::EventKind::Create(_) => {}
                notify::EventKind::Modify(modify) => match modify {
                    notify::event::ModifyKind::Any => {}
                    notify::event::ModifyKind::Data(data) => {
                        sender.send(modify_data_event(data)).unwrap()
                    }
                    notify::event::ModifyKind::Metadata(_) => {}
                    notify::event::ModifyKind::Name(_) => {}
                    notify::event::ModifyKind::Other => {}
                },
                notify::EventKind::Remove(_) => {}
                notify::EventKind::Other => {}
            },
            Err(error) => println!("Error: {error:?}"),
        }
    }

    fn config() -> notify::Config {
        notify::Config::default().with_poll_interval(Duration::from_millis(200))
    }

    fn watch(mut self) -> PigResult<()> {
        for entry in &self.entries {
            entry.render()?;
        }

        self.config_watcher
            .watch(self.config.file.as_path(), RecursiveMode::Recursive)?;

        for entry in &mut self.entries {
            entry.watch()?;
        }

        for event in self.receiver {
            match event {
                Event::ConfigModifyData(_) => return Self::new(Config::new()?)?.watch(),
                Event::OpenapiModifyData(i, _) => {
                    self.entries[i].context()?;
                    self.entries[i].render()?;
                }
                Event::InputModifyData(i, _) => {
                    self.entries[i].tera()?;
                    self.entries[i].render()?;
                }
            }
        }

        Ok(())
    }
}

pub struct WatcherEntry {
    index: usize,
    config: ConfigEntry,
    openapi_watcher: RecommendedWatcher,
    input_watcher: RecommendedWatcher,
    context: Context,
    tera: Tera,
}

impl WatcherEntry {
    fn new(config: ConfigEntry, index: usize, sender: Sender<Event>) -> PigResult<Self> {
        Ok(Self {
            index,
            openapi_watcher: RecommendedWatcher::new(
                Watcher::handler(sender.clone(), move |event| {
                    Event::OpenapiModifyData(index, event)
                }),
                Watcher::config(),
            )?,
            input_watcher: RecommendedWatcher::new(
                Watcher::handler(sender.clone(), move |event| {
                    Event::InputModifyData(index, event)
                }),
                Watcher::config(),
            )?,
            context: Pig::context(&config)?,
            tera: Pig::tera(&config)?,
            config,
        })
    }

    fn render(&self) -> PigResult<()> {
        Pig::render(&self.config, &self.tera, &self.context)?;

        Ok(())
    }

    fn context(&mut self) -> PigResult<()> {
        self.context = Pig::context(&self.config)?;

        Ok(())
    }

    fn tera(&mut self) -> PigResult<()> {
        self.tera = Pig::tera(&self.config)?;

        Ok(())
    }

    fn watch(&mut self) -> PigResult<()> {
        self.openapi_watcher
            .watch(self.config.openapi.as_path(), RecursiveMode::Recursive)?;
        self.input_watcher
            .watch(self.config.input.as_path(), RecursiveMode::Recursive)?;

        Ok(())
    }
}
