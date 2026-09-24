use std::fs;
use std::net::Ipv4Addr;

use zed_extension_api::{
    self as zed, serde_json, DebugAdapterBinary, DebugConfig, DebugRequest, DebugScenario,
    DebugTaskDefinition, Result, StartDebuggingRequestArguments,
    StartDebuggingRequestArgumentsRequest,
};

const DEBUG_ADAPTER_NAME: &str = "Regal";

struct Rego {
    cached_binary_path: Option<String>,
}

impl Rego {
    fn regal_binary(
        &mut self,
        language_server_id: Option<&zed::LanguageServerId>,
        worktree: &zed::Worktree,
    ) -> Result<String>  {
        if let Some(path) = worktree.which("regal") {
            return Ok(path);
        }

        if let Some(path) = &self.cached_binary_path {
            if fs::metadata(path).is_ok_and(|stat| stat.is_file()) {
                return Ok(path.clone());
            }
        }

        let set_status = |status: zed::LanguageServerInstallationStatus| {
            if let Some(id) = language_server_id {
                zed::set_language_server_installation_status(id, &status);
            }
        };

        set_status(zed::LanguageServerInstallationStatus::CheckingForUpdate);

        let release = zed::latest_github_release(
            "open-policy-agent/regal",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let version_dir = format!("rego-{}", release.version);
        fs::create_dir_all(&version_dir).map_err(|e| format!("failed to create directory: {e}"))?;

        let (platform, arch) = zed::current_platform();
        let asset_name = format!(
            "regal_{os}_{arch}{extension}",
            os = match platform {
                zed::Os::Mac => "Darwin",
                zed::Os::Linux => "Linux",
                zed::Os::Windows => "Windows",
            },
            arch = match arch {
                zed::Architecture::Aarch64 => "arm64",
                zed::Architecture::X86 => "x86",
                zed::Architecture::X8664 => "x86_64",
            },
            extension = match platform {
                zed::Os::Mac | zed::Os::Linux => "",
                zed::Os::Windows => ".exe",
            }
        );

        let binary_path = format!("{version_dir}/{asset_name}");

        if !fs::metadata(&binary_path).is_ok_and(|stat| stat.is_file()) {
            set_status(zed::LanguageServerInstallationStatus::Downloading);

            let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("no asset found matching {asset_name:?}"))?;

            zed::download_file(
                &asset.download_url,
                &binary_path,
                zed::DownloadedFileType::Uncompressed,
            )
            .map_err(|e| format!("failed to download file: {e}"))?;

            zed::make_file_executable(&binary_path)?;

            let entries =
                fs::read_dir(".").map_err(|e| format!("failed to list working directory {e}"))?;
            for entry in entries {
                let entry = entry.map_err(|e| format!("failed to load directory entry {e}"))?;
                if entry.file_name().to_str() != Some(&version_dir) {
                    fs::remove_dir_all(entry.path()).ok();
                }
            }
        }

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

/// Fills in the same launch defaults as the OPA VS Code extension, so that a minimal
/// `{"request": "launch"}` configuration debugs the whole workspace.
fn with_launch_defaults(
    mut config: serde_json::Value,
    worktree: &zed::Worktree,
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
        map.insert("bundlePaths".into(), vec![worktree.root_path()].into());
    }

    Ok(config)
}

impl zed::Extension for Rego {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        Ok(zed::Command {
            command: self.regal_binary(Some(language_server_id), worktree)?,
            args: vec!["language-server".to_string()],
            env: Default::default(),
        })
    }

    fn get_dap_binary(
        &mut self,
        adapter_name: String,
        config: DebugTaskDefinition,
        user_provided_debug_adapter_path: Option<String>,
        worktree: &zed::Worktree,
    ) -> Result<DebugAdapterBinary> {
        if adapter_name != DEBUG_ADAPTER_NAME {
            return Err(format!("unknown debug adapter: {adapter_name}"));
        }

        let configuration: serde_json::Value = serde_json::from_str(&config.config)
            .map_err(|e| format!("invalid Regal debug configuration: {e}"))?;
        let request = self.dap_request_kind(adapter_name, configuration.clone())?;
        let configuration = with_launch_defaults(configuration, worktree)?;

        let command = match user_provided_debug_adapter_path {
            Some(path) => path,
            None => self.regal_binary(None, worktree)?,
        };

        let mut arguments = vec!["debug".to_string()];
        let connection = match config.tcp_connection {
            Some(template) => {
                let tcp = zed::resolve_tcp_template(template)?;
                arguments.extend([
                    "--server".to_string(),
                    "--address".to_string(),
                    format!("{}:{}", Ipv4Addr::from(tcp.host), tcp.port),
                ]);
                Some(tcp)
            }
            None => None,
        };

        Ok(DebugAdapterBinary {
            command: Some(command),
            arguments,
            envs: Default::default(),
            cwd: Some(worktree.root_path()),
            connection,
            request_args: StartDebuggingRequestArguments {
                configuration: configuration.to_string(),
                request,
            },
        })
    }

    fn dap_request_kind(
        &mut self,
        _adapter_name: String,
        config: serde_json::Value,
    ) -> Result<StartDebuggingRequestArgumentsRequest> {
        match config.get("request").and_then(|r| r.as_str()) {
            Some("launch") => Ok(StartDebuggingRequestArgumentsRequest::Launch),
            Some("attach") => Err("the Regal debugger does not support attach requests".into()),
            Some(other) => Err(format!("unexpected `request` value in Regal debug configuration: {other}")),
            None => Err("missing `request` field in Regal debug configuration".into()),
        }
    }

    fn dap_config_to_scenario(&mut self, config: DebugConfig) -> Result<DebugScenario> {
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
}

zed::register_extension!(Rego);
