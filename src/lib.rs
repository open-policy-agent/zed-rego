mod dap;

use std::fs;

use zed_extension_api::{
    self as zed, serde_json, DebugAdapterBinary, DebugConfig, DebugScenario, DebugTaskDefinition,
    Result, StartDebuggingRequestArgumentsRequest,
};

struct Rego {
    cached_binary_path: Option<String>,
}

impl Rego {
    fn regal_binary(
        &mut self,
        language_server_id: Option<&zed::LanguageServerId>,
        worktree: &zed::Worktree,
    ) -> Result<String> {
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
        let command = match user_provided_debug_adapter_path {
            Some(path) => path,
            None => self.regal_binary(None, worktree)?,
        };

        dap::binary(&adapter_name, config, command, &worktree.root_path())
    }

    fn dap_request_kind(
        &mut self,
        _adapter_name: String,
        config: serde_json::Value,
    ) -> Result<StartDebuggingRequestArgumentsRequest> {
        dap::request_kind(&config)
    }

    fn dap_config_to_scenario(&mut self, config: DebugConfig) -> Result<DebugScenario> {
        dap::config_to_scenario(config)
    }
}

zed::register_extension!(Rego);
