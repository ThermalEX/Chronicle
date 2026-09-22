use serde_json::Value;

pub(crate) fn tray_labels(settings: &Value) -> (&'static str, &'static str) {
    if settings.pointer("/app/language").and_then(Value::as_str) == Some("en") {
        ("Show Chronicle", "Quit Chronicle")
    } else {
        ("显示 Chronicle", "退出 Chronicle")
    }
}

pub(crate) struct TrayMenu {
    pub show: tauri::menu::MenuItem<tauri::Wry>,
    pub exit: tauri::menu::MenuItem<tauri::Wry>,
}

pub(crate) fn update_tray(app: &tauri::AppHandle, settings: &Value) {
    use tauri::Manager;
    if let Some(menu) = app.try_state::<TrayMenu>() {
        let (show, exit) = tray_labels(settings);
        // Persistence succeeded; a tray rendering failure must not report a failed save.
        if let Err(error) = menu
            .show
            .set_text(show)
            .and_then(|()| menu.exit.set_text(exit))
        {
            eprintln!("Unable to update tray language: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_language_uses_saved_preference_and_defaults_to_chinese() {
        assert_eq!(
            tray_labels(&serde_json::json!({})),
            ("显示 Chronicle", "退出 Chronicle")
        );
        assert_eq!(
            tray_labels(&serde_json::json!({"app":{"language":"en"}})),
            ("Show Chronicle", "Quit Chronicle")
        );
        assert_eq!(
            tray_labels(&serde_json::json!({"app":{"language":"fr"}})),
            ("显示 Chronicle", "退出 Chronicle")
        );
    }
}
