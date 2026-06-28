#[cfg(target_os = "linux")]
use std::process::Command;

#[cfg(target_os = "linux")]
const BUTTON_LAYOUT_PATH: &str = "/org/gnome/desktop/wm/preferences/button-layout";

#[cfg(target_os = "linux")]
pub fn read_button_layout() -> Option<Vec<String>> {
    let layout = read(BUTTON_LAYOUT_PATH)?;
    let controls = parse_button_layout(&layout);
    (!controls.is_empty()).then_some(controls)
}

#[cfg(target_os = "linux")]
fn read(path: &str) -> Option<String> {
    let output = Command::new("dconf").args(["read", path]).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout)
        .replace('\'', "")
        .replace('"', "")
        .replace('\n', "")
        .trim()
        .to_string();

    (!value.is_empty()).then_some(value)
}

fn parse_button_layout(layout: &str) -> Vec<String> {
    layout
        .split(':')
        .flat_map(|section| section.split(','))
        .filter(|control| matches!(*control, "minimize" | "maximize" | "close"))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_button_layout;

    #[test]
    fn parse_button_layout_ignores_appmenu_and_keeps_window_controls() {
        assert_eq!(
            parse_button_layout("appmenu:minimize,maximize,close"),
            ["minimize", "maximize", "close"]
        );
    }

    #[test]
    fn parse_button_layout_keeps_left_side_controls_when_right_side_is_empty() {
        assert_eq!(
            parse_button_layout("close,minimize,maximize:"),
            ["close", "minimize", "maximize"]
        );
    }

    #[test]
    fn parse_button_layout_returns_empty_for_unusable_values() {
        assert!(parse_button_layout("").is_empty());
        assert!(parse_button_layout("appmenu:").is_empty());
    }
}
