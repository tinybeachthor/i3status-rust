use std::fmt;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use uuid::Uuid;

use crate::blocks::Update;
use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::subprocess::spawn_child_async;
use crate::widget::{I3BarWidget, State};
use crate::widgets::button::ButtonWidget;

enum PomodoroState {
    Started(Instant),
    Stopped,
    Paused(Duration),
    OnBreak(Instant),
}

impl PomodoroState {
    fn elapsed(&self) -> Duration {
        match self {
            PomodoroState::Started(start) => Instant::now().duration_since(start.to_owned()),
            PomodoroState::Stopped => unreachable!(),
            PomodoroState::Paused(duration) => duration.to_owned(),
            PomodoroState::OnBreak(start) => Instant::now().duration_since(start.to_owned()),
        }
    }
}

impl fmt::Display for PomodoroState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PomodoroState::Stopped => write!(f, "0:00"),
            PomodoroState::Started(_) => write!(
                f,
                "{}:{:02}",
                self.elapsed().as_secs() / 60,
                self.elapsed().as_secs() % 60
            ),
            PomodoroState::OnBreak(_) => write!(
                f,
                "{}:{:02}",
                self.elapsed().as_secs() / 60,
                self.elapsed().as_secs() % 60
            ),
            PomodoroState::Paused(duration) => write!(
                f,
                "{}:{:02}",
                duration.as_secs() / 60,
                duration.as_secs() % 60
            ),
        }
    }
}

pub struct Pomodoro {
    id: String,
    time: ButtonWidget,
    state: PomodoroState,
    length: Duration,
    break_length: Duration,
    update_interval: Duration,
    message: String,
    break_message: String,
    count: usize,
    use_nag: bool,
    nag_path: std::path::PathBuf,
}

impl Pomodoro {
    fn set_text(&mut self) {
        self.time
            .set_text(format!("{} | {}", self.count, self.state));
        self.time
            .set_state(self.compute_state());
    }

    fn compute_state(&self) -> State {
        match self.state {
            PomodoroState::Started(_) => State::Info,
            PomodoroState::Stopped => State::Idle,
            PomodoroState::Paused(_) => State::Warning,
            PomodoroState::OnBreak(_) => State::Critical,
        }
    }

    fn nag(&self, message: &str, level: &str) {
        spawn_child_async(
            self.nag_path.to_str().unwrap(),
            &["-t", level, "-m", message],
        )
        .expect("Failed to start i3-nagbar");
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct PomodoroConfig {
    #[serde(default = "PomodoroConfig::default_length")]
    pub length: u64,
    #[serde(default = "PomodoroConfig::default_break_length")]
    pub break_length: u64,
    #[serde(default = "PomodoroConfig::default_message")]
    pub message: String,
    #[serde(default = "PomodoroConfig::default_break_message")]
    pub break_message: String,
    #[serde(default = "PomodoroConfig::default_use_nag")]
    pub use_nag: bool,
    #[serde(default = "PomodoroConfig::default_nag_path")]
    pub nag_path: std::path::PathBuf,
}

impl PomodoroConfig {
    fn default_length() -> u64 {
        25
    }

    fn default_break_length() -> u64 {
        5
    }

    fn default_message() -> String {
        "Pomodoro over! Take a break!".to_owned()
    }

    fn default_break_message() -> String {
        "Break over! Time to work!".to_owned()
    }

    fn default_use_nag() -> bool {
        false
    }

    fn default_nag_path() -> std::path::PathBuf {
        std::path::PathBuf::from("i3-nagbar")
    }
}

impl ConfigBlock for Pomodoro {
    type Config = PomodoroConfig;

    fn new(block_config: Self::Config, config: Config, _send: Sender<Task>) -> Result<Self> {
        let id: String = Uuid::new_v4().to_simple().to_string();

        Ok(Pomodoro {
            id: id.clone(),
            time: ButtonWidget::new(config, &id),
            state: PomodoroState::Stopped,
            length: Duration::from_secs(block_config.length * 60), // convert to minutes
            break_length: Duration::from_secs(block_config.break_length * 60), // convert to minutes
            update_interval: Duration::from_millis(1000),
            message: block_config.message,
            break_message: block_config.break_message,
            use_nag: block_config.use_nag,
            count: 0,
            nag_path: block_config.nag_path,
        })
    }
}

impl Block for Pomodoro {
    fn id(&self) -> &str {
        &self.id
    }

    fn update(&mut self) -> Result<Option<Update>> {
        self.set_text();
        match &self.state {
            PomodoroState::Started(_) => {
                if self.state.elapsed() >= self.length {
                    if self.use_nag {
                        self.nag(&self.message, "error");
                    }

                    self.state = PomodoroState::OnBreak(Instant::now());
                }
            }
            PomodoroState::OnBreak(_) => {
                if self.state.elapsed() >= self.break_length {
                    if self.use_nag {
                        self.nag(&self.break_message, "warning");
                    }
                    self.state = PomodoroState::Stopped;
                    self.count += 1;
                }
            }
            _ => {}
        }

        Ok(Some(self.update_interval.into()))
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = event.name {
            if name.as_str() == self.id {
                match event.button {
                    MouseButton::Right => {
                        self.state = PomodoroState::Stopped;
                        self.count = 0;
                    }
                    _ => match &self.state {
                        PomodoroState::Stopped => {
                            self.state = PomodoroState::Started(Instant::now());
                        }
                        PomodoroState::Started(_) => {
                            self.state = PomodoroState::Paused(self.state.elapsed());
                        }
                        PomodoroState::Paused(duration) => {
                            self.state = PomodoroState::Started(
                                Instant::now().checked_sub(duration.to_owned()).unwrap(),
                            );
                        }
                        PomodoroState::OnBreak(_) => {
                            self.state = PomodoroState::Started(Instant::now());
                            self.count += 1;
                        }
                    },
                }
            }
        }

        self.set_text();
        Ok(())
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.time]
    }
}
