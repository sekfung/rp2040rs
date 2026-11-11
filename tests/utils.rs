use std::sync::{Arc, Mutex};

use rp2040rs::utils::logging::{ConsoleLogger, LogLevel, Logger};
use rp2040rs::utils::{bit, s32, u32};

#[test]
fn bit_helpers_use_32_bit_semantics() {
    assert_eq!(bit(0), 1);
    assert_eq!(bit(31), 0x8000_0000);
    assert_eq!(s32(0xffff_ffff), -1);
    assert_eq!(u32(-1), 0xffff_ffff);
}

#[test]
fn console_logger_throws_on_error_when_enabled() {
    let logger = ConsoleLogger::new(LogLevel::Debug, true);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        logger.error("UART", "boom");
    }));
    assert!(result.is_err());
}

#[derive(Default)]
struct RecordingLogger {
    messages: Mutex<Vec<(LogLevel, String, String)>>,
}

impl Logger for RecordingLogger {
    fn debug(&self, component: &str, message: &str) {
        self.messages
            .lock()
            .unwrap()
            .push((LogLevel::Debug, component.into(), message.into()));
    }
    fn info(&self, component: &str, message: &str) {
        self.messages
            .lock()
            .unwrap()
            .push((LogLevel::Info, component.into(), message.into()));
    }
    fn warn(&self, component: &str, message: &str) {
        self.messages
            .lock()
            .unwrap()
            .push((LogLevel::Warn, component.into(), message.into()));
    }
    fn error(&self, component: &str, message: &str) {
        self.messages
            .lock()
            .unwrap()
            .push((LogLevel::Error, component.into(), message.into()));
    }
}

#[test]
fn logger_trait_accepts_a_recorder() {
    let logger = Arc::new(RecordingLogger::default());
    logger.warn("PIO", "stalled");
    let messages = logger.messages.lock().unwrap();
    assert_eq!(messages[0].0, LogLevel::Warn);
    assert_eq!(messages[0].1, "PIO");
}
