use std::net::Ipv4Addr;

use zed_extension_api::{
    self as zed, serde_json, DebugAdapterBinary, DebugConfig, DebugRequest, DebugScenario,
    DebugTaskDefinition, Result, StartDebuggingRequestArguments,
    StartDebuggingRequestArgumentsRequest, TcpArguments,
};

pub const DEBUG_ADAPTER_NAME: &str = "Regal";

/// Builds the command used to start `regal debug`, and the launch request to send to it.
pub fn binary(
    adapter_name: &str,
    config: DebugTaskDefinition,
    command: String,
    worktree_root: &str,
) -> Result<DebugAdapterBinary> {
    if adapter_name != DEBUG_ADAPTER_NAME {
        return Err(format!("unknown debug adapter: {adapter_name}"));
    }

    let configuration: serde_json::Value = serde_json::from_str(&config.config)
        .map_err(|e| format!("invalid Regal debug configuration: {e}"))?;
    let request = request_kind(&configuration)?;
    let configuration = with_launch_defaults(configuration, worktree_root)?;

    let connection = config
        .tcp_connection
        .map(zed::resolve_tcp_template)
        .transpose()?;

    Ok(DebugAdapterBinary {
        command: Some(command),
        arguments: arguments(connection.as_ref()),
        envs: Default::default(),
        cwd: Some(worktree_root.to_string()),
        connection,
        request_args: StartDebuggingRequestArguments {
            configuration: configuration.to_string(),
            request,
        },
    })
}

pub fn request_kind(config: &serde_json::Value) -> Result<StartDebuggingRequestArgumentsRequest> {
    match config.get("request").and_then(|r| r.as_str()) {
        Some("launch") => Ok(StartDebuggingRequestArgumentsRequest::Launch),
        Some("attach") => Err("the Regal debugger does not support attach requests".into()),
        Some(other) => Err(format!("unexpected `request` value in Regal debug configuration: {other}")),
        None => Err("missing `request` field in Regal debug configuration".into()),
    }
}

pub fn config_to_scenario(config: DebugConfig) -> Result<DebugScenario> {
    let DebugRequest::Launch(launch) = config.request else {
        return Err("the Regal debugger does not support attach requests".into());
    };

    // The "program" entered in the new session modal is treated as the policy (bundle) path to evaluate.
    let mut scenario = serde_json::json!({
        "request": "launch",
        "command": "eval",
        "query": "data",
        "bundlePaths": [launch.program],
    });
    if let Some(stop_on_entry) = config.stop_on_entry {
        scenario["stopOnEntry"] = stop_on_entry.into();
    }

    Ok(DebugScenario {
        adapter: config.adapter,
        label: config.label,
        build: None,
        config: scenario.to_string(),
        tcp_connection: None,
    })
}

/// Arguments for `regal`: stdio by default, or server mode when a TCP connection is configured.
fn arguments(connection: Option<&TcpArguments>) -> Vec<String> {
    let mut arguments = vec!["debug".to_string()];
    if let Some(tcp) = connection {
        arguments.extend([
            "--server".to_string(),
            "--address".to_string(),
            format!("{}:{}", Ipv4Addr::from(tcp.host), tcp.port),
        ]);
    }
    arguments
}

/// Fills in the same launch defaults as the OPA VS Code extension, so that a minimal
/// `{"request": "launch"}` configuration debugs the whole workspace.
fn with_launch_defaults(
    mut config: serde_json::Value,
    worktree_root: &str,
) -> Result<serde_json::Value> {
    let map = config
        .as_object_mut()
        .ok_or("Regal debug configuration must be a JSON object")?;

    map.entry("command").or_insert("eval".into());
    map.entry("query").or_insert("data".into());
    map.entry("stopOnEntry").or_insert(true.into());
    map.entry("stopOnResult").or_insert(true.into());
    map.entry("enablePrint").or_insert(true.into());
    if !map.contains_key("bundlePaths") && !map.contains_key("dataPaths") {
        map.insert("bundlePaths".into(), vec![worktree_root].into());
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use zed_extension_api::{AttachRequest, LaunchRequest};

    const ROOT: &str = "/workspace";

    fn task(config: serde_json::Value) -> DebugTaskDefinition {
        DebugTaskDefinition {
            label: "test".into(),
            adapter: DEBUG_ADAPTER_NAME.into(),
            config: config.to_string(),
            tcp_connection: None,
        }
    }

    fn launch_config(binary: &DebugAdapterBinary) -> serde_json::Value {
        serde_json::from_str(&binary.request_args.configuration).unwrap()
    }

    #[test]
    fn minimal_launch_gets_defaults() {
        let binary = binary(DEBUG_ADAPTER_NAME, task(json!({"request": "launch"})), "regal".into(), ROOT).unwrap();

        assert_eq!(binary.command.as_deref(), Some("regal"));
        assert_eq!(binary.arguments, vec!["debug"]);
        assert_eq!(binary.cwd.as_deref(), Some(ROOT));
        assert!(binary.connection.is_none());
        assert_eq!(binary.request_args.request, StartDebuggingRequestArgumentsRequest::Launch);
        assert_eq!(
            launch_config(&binary),
            json!({
                "request": "launch",
                "command": "eval",
                "query": "data",
                "stopOnEntry": true,
                "stopOnResult": true,
                "enablePrint": true,
                "bundlePaths": [ROOT],
            })
        );
    }

    #[test]
    fn user_values_are_kept() {
        let config = json!({
            "request": "launch",
            "query": "data.policy.allow",
            "stopOnEntry": false,
            "stopOnResult": false,
            "enablePrint": false,
            "bundlePaths": [],
            "input": {"user": "bob"},
        });
        let binary = binary(DEBUG_ADAPTER_NAME, task(config), "regal".into(), ROOT).unwrap();
        let launch = launch_config(&binary);

        assert_eq!(launch["query"], "data.policy.allow");
        assert_eq!(launch["stopOnEntry"], false);
        assert_eq!(launch["stopOnResult"], false);
        assert_eq!(launch["enablePrint"], false);
        assert_eq!(launch["bundlePaths"], json!([]));
        assert_eq!(launch["input"], json!({"user": "bob"}));
    }

    #[test]
    fn data_paths_disable_default_bundle() {
        let config = json!({"request": "launch", "dataPaths": ["policy"]});
        let binary = binary(DEBUG_ADAPTER_NAME, task(config), "regal".into(), ROOT).unwrap();

        assert!(launch_config(&binary).get("bundlePaths").is_none());
    }

    #[test]
    fn invalid_configurations_are_rejected() {
        let err = |config: serde_json::Value| {
            binary(DEBUG_ADAPTER_NAME, task(config), "regal".into(), ROOT).unwrap_err()
        };

        assert!(err(json!({})).contains("missing `request`"));
        assert!(err(json!({"request": "attach"})).contains("does not support attach"));
        assert!(err(json!({"request": "restart"})).contains("unexpected `request` value"));
        assert!(err(json!(["launch"])).contains("missing `request`"));
        assert!(binary("other", task(json!({"request": "launch"})), "regal".into(), ROOT)
            .unwrap_err()
            .contains("unknown debug adapter"));
    }

    #[test]
    fn server_mode_arguments() {
        let tcp = TcpArguments {
            host: Ipv4Addr::LOCALHOST.into(),
            port: 4712,
            timeout: None,
        };

        assert_eq!(arguments(None), vec!["debug"]);
        assert_eq!(arguments(Some(&tcp)), vec!["debug", "--server", "--address", "127.0.0.1:4712"]);
    }

    #[test]
    fn scenario_from_launch_request() {
        let scenario = config_to_scenario(DebugConfig {
            label: "Debug policy".into(),
            adapter: DEBUG_ADAPTER_NAME.into(),
            request: DebugRequest::Launch(LaunchRequest {
                program: "policy/".into(),
                cwd: None,
                args: vec![],
                envs: vec![],
            }),
            stop_on_entry: Some(false),
        })
        .unwrap();

        assert_eq!(scenario.label, "Debug policy");
        assert_eq!(scenario.adapter, DEBUG_ADAPTER_NAME);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&scenario.config).unwrap(),
            json!({
                "request": "launch",
                "command": "eval",
                "query": "data",
                "bundlePaths": ["policy/"],
                "stopOnEntry": false,
            })
        );
    }

    #[test]
    fn scenario_from_attach_request_is_rejected() {
        let result = config_to_scenario(DebugConfig {
            label: "Attach".into(),
            adapter: DEBUG_ADAPTER_NAME.into(),
            request: DebugRequest::Attach(AttachRequest { process_id: None }),
            stop_on_entry: None,
        });

        assert!(result.is_err());
    }
}
