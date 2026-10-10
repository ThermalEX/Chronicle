use serde_json::Value;
use std::sync::atomic::{AtomicU8, Ordering};

/// The native window callback must never wait for backup or filesystem work.
pub(crate) struct CloseBehavior(AtomicU8);

impl CloseBehavior {
    pub(crate) fn new(settings: &Value) -> Self {
        Self(AtomicU8::new(Self::code(settings)))
    }

    fn code(settings: &Value) -> u8 {
        match settings
            .pointer("/app/closeBehavior")
            .and_then(Value::as_str)
        {
            Some("tray") => 1,
            Some("exit") => 2,
            _ => 0,
        }
    }

    pub(crate) fn update(&self, settings: &Value) {
        self.0.store(Self::code(settings), Ordering::Relaxed);
    }

    pub(crate) fn get(&self) -> &'static str {
        match self.0.load(Ordering::Relaxed) {
            1 => "tray",
            2 => "exit",
            _ => "ask",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn close_policy_uses_saved_values_and_rejects_unknown_modes() {
        let policy = CloseBehavior::new(&json!({"app":{"closeBehavior":"tray"}}));
        assert_eq!(policy.get(), "tray");
        policy.update(&json!({"app":{"closeBehavior":"exit"}}));
        assert_eq!(policy.get(), "exit");
        policy.update(&json!({"app":{"closeBehavior":"invalid"}}));
        assert_eq!(policy.get(), "ask");
    }

    #[test]
    fn close_policy_does_not_wait_for_repository_lock() {
        let repository = std::sync::Mutex::new(());
        let _backup = repository.lock().unwrap();
        let policy = CloseBehavior::new(&json!({"app":{"closeBehavior":"tray"}}));
        std::thread::scope(|scope| {
            assert_eq!(scope.spawn(|| policy.get()).join().unwrap(), "tray");
        });
    }
}
