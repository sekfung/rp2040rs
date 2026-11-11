use std::fmt;

use crate::utils::time::format_time_now;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

pub trait Logger: 'static {
    fn debug(&self, component: &str, message: &str);
    fn info(&self, component: &str, message: &str);
    fn warn(&self, component: &str, message: &str);
    fn error(&self, component: &str, message: &str);
}

pub struct ConsoleLogger {
    current_level: LogLevel,
    throw_on_error: bool,
}

impl ConsoleLogger {
    pub fn new(current_level: LogLevel, throw_on_error: bool) -> Self {
        Self {
            current_level,
            throw_on_error,
        }
    }

    fn enabled(&self, level: LogLevel) -> bool {
        level >= self.current_level
    }

    fn format_message(&self, component: &str, message: &str) -> String {
        format!("{} [{}] {}", format_time_now(), component, message)
    }
}

impl Logger for ConsoleLogger {
    fn debug(&self, component: &str, message: &str) {
        if self.enabled(LogLevel::Debug) {
            eprintln!("{}", self.format_message(component, message));
        }
    }

    fn info(&self, component: &str, message: &str) {
        if self.enabled(LogLevel::Info) {
            eprintln!("{}", self.format_message(component, message));
        }
    }

    fn warn(&self, component: &str, message: &str) {
        if self.enabled(LogLevel::Warn) {
            eprintln!("{}", self.format_message(component, message));
        }
    }

    fn error(&self, component: &str, message: &str) {
        if self.enabled(LogLevel::Error) {
            eprintln!("{}", self.format_message(component, message));
            if self.throw_on_error {
                panic!("[{}] {}", component, message);
            }
        }
    }
}

impl fmt::Debug for ConsoleLogger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConsoleLogger")
            .field("current_level", &self.current_level)
            .field("throw_on_error", &self.throw_on_error)
            .finish()
    }
}
