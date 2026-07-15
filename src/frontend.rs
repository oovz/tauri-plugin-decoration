use crate::lifecycle::FrontendTarget;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Platform {
    #[cfg(any(target_os = "macos", test))]
    Macos,
    #[cfg(any(target_os = "windows", test))]
    Windows,
    #[cfg(any(target_os = "linux", test))]
    Linux,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub(crate) struct ControlLayout {
    pub(crate) left: Vec<&'static str>,
    pub(crate) right: Vec<&'static str>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub(crate) struct FrontendOptions {
    pub(crate) controls: Vec<&'static str>,
    pub(crate) icons: BTreeMap<&'static str, String>,
    pub(crate) layout: Option<ControlLayout>,
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
pub(crate) fn dispatch_script<T: Serialize>(
    target: FrontendTarget,
    event: &'static str,
    payload: &T,
) -> Result<String, serde_json::Error> {
    let window_generation = serde_json::to_string(&target.window.get().to_string())?;
    let document_token = serde_json::to_string(&target.document.get().to_string())?;
    let event = serde_json::to_string(event)?;
    let payload = serde_json::to_string(payload)?;
    Ok(format!(
        "void window.__TAURI_PLUGIN_DECORATION__?.dispatch({window_generation},{document_token},{event},{payload});"
    ))
}

pub(crate) fn prepare_script(
    target: FrontendTarget,
    platform: Platform,
    options: &FrontendOptions,
) -> Result<String, serde_json::Error> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct InstallerConfig<'a> {
        window_generation: String,
        document_token: String,
        platform: Platform,
        controls: &'a [&'static str],
        icons: &'a BTreeMap<&'static str, String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        layout: Option<&'a ControlLayout>,
    }

    let config = serde_json::to_string(&InstallerConfig {
        window_generation: target.window.get().to_string(),
        document_token: target.document.get().to_string(),
        platform,
        controls: &options.controls,
        icons: &options.icons,
        layout: options.layout.as_ref(),
    })?;
    let adapter = match platform {
        #[cfg(any(target_os = "macos", test))]
        Platform::Macos => "",
        #[cfg(any(target_os = "windows", test))]
        Platform::Windows => include_str!("js/controls.js"),
        #[cfg(any(target_os = "linux", test))]
        Platform::Linux => include_str!("js/linux-controls.js"),
    };
    let core = include_str!("js/titlebar.js");
    let mut script = String::with_capacity(core.len() + adapter.len() + config.len() + 64);
    script.push_str(core);
    script.push('\n');
    script.push_str(adapter);
    script.push_str("\nvoid window.__TAURI_PLUGIN_DECORATION__.install(");
    script.push_str(&config);
    script.push_str(");");
    Ok(script)
}

pub(crate) fn cancel_script(target: FrontendTarget) -> Result<String, serde_json::Error> {
    let window_generation = serde_json::to_string(&target.window.get().to_string())?;
    let document_token = serde_json::to_string(&target.document.get().to_string())?;
    Ok(format!(
        "void window.__TAURI_PLUGIN_DECORATION__?.cancel({window_generation},{document_token});"
    ))
}

#[cfg(test)]
mod tests {
    use super::{cancel_script, dispatch_script, prepare_script, FrontendOptions, Platform};
    use crate::lifecycle::FrontendTarget;

    #[test]
    fn windows_script_preserves_full_width_tokens_and_uses_only_its_adapter() {
        let target = FrontendTarget::from_values(u64::MAX, u64::MAX - 1).unwrap();
        let script = prepare_script(
            target,
            Platform::Windows,
            &FrontendOptions {
                controls: vec!["minimize", "maximize", "close"],
                ..FrontendOptions::default()
            },
        )
        .unwrap();

        assert!(script.contains("__TAURI_PLUGIN_DECORATION__"));
        assert!(script.contains("registerPlatform(\"windows\""));
        assert!(!script.contains("registerPlatform(\"linux\""));
        assert!(script.contains("\"windowGeneration\":\"18446744073709551615\""));
        assert!(script.contains("\"documentToken\":\"18446744073709551614\""));
        assert!(script.contains("void window.__TAURI_PLUGIN_DECORATION__.install("));
        assert!(!script.contains(".listen("));
    }

    #[test]
    fn linux_dynamic_values_remain_json_data() {
        let target = FrontendTarget::from_values(1, 2).unwrap();
        let mut options = FrontendOptions {
            controls: vec!["close"],
            ..FrontendOptions::default()
        };
        options.icons.insert(
            "close",
            "data:image/png;base64,YCI7Z2xvYmFsVGhpcy5wd25lZD10cnVlOy8v".to_owned(),
        );

        let script = prepare_script(target, Platform::Linux, &options).unwrap();
        assert!(script.contains("registerPlatform(\"linux\""));
        assert!(!script.contains("@win-"));
        assert!(!script.contains("innerHTML"));
        assert!(!script.contains("globalThis.pwned"));
    }

    #[test]
    fn macos_script_has_no_custom_control_adapter() {
        let target = FrontendTarget::from_values(1, 2).unwrap();
        let script = prepare_script(target, Platform::Macos, &FrontendOptions::default()).unwrap();

        assert!(!script.contains("registerPlatform(\"windows\""));
        assert!(!script.contains("registerPlatform(\"linux\""));
        assert!(script.contains("\"platform\":\"macos\""));
    }

    #[test]
    fn native_events_dispatch_only_to_the_exact_frontend_target() {
        let target = FrontendTarget::from_values(u64::MAX, u64::MAX - 1).unwrap();
        let script = dispatch_script(target, "snap-mousemove", &(12, -4)).unwrap();

        assert_eq!(
            script,
            "void window.__TAURI_PLUGIN_DECORATION__?.dispatch(\"18446744073709551615\",\"18446744073709551614\",\"snap-mousemove\",[12,-4]);"
        );
        assert!(!script.contains("emit"));
    }

    #[test]
    fn cancellation_targets_only_the_exact_frontend_installation() {
        let target = FrontendTarget::from_values(u64::MAX, u64::MAX - 1).unwrap();
        assert_eq!(
            cancel_script(target).unwrap(),
            "void window.__TAURI_PLUGIN_DECORATION__?.cancel(\"18446744073709551615\",\"18446744073709551614\");"
        );
    }
}
