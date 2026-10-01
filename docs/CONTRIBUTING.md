# Contributing

We'd love to have you contribute to the Zed Rego extension!

## Development

To install the extension in development mode, first make sure to have [Rust](https://www.rust-lang.org/tools/install)
installed.

Next, clone this repository. Now from the Zed "Extensions" menu, choose "Install Dev Extension" and select the directory
where you cloned this repository.

## Syntax highlighting

The zed-rego extension uses the tree-sitter grammar provided by the
[tree-sitter-rego](https://github.com/FallenAngel97/tree-sitter-rego) project, and contributions to imprive syntax
highlighting and other basic language features should be made there. If you have had code merged into that project,
make sure to submit a PR to us updating the [extension.toml](../extension.toml) file in this repo to point at the
latest commit in the tree-sitter-rego project.

## Language server features

The language server features in this extension are provided by
[Regal](https://www.openpolicyagent.org/projects/regal), and new
features and fixes should be submitted to that project directly. See the Regal docs on
[contributing](https://github.com/open-policy-agent/regal/blob/main/docs/CONTRIBUTING.md) for how to get started!

## Releasing

Releases are published to the [Zed extensions](https://github.com/zed-industries/extensions) repository by the
[release workflow](../.github/workflows/release.yaml), using
[zed-extension-action](https://github.com/huacnlee/zed-extension-action). To cut a release:

1. Bump `version` in both [extension.toml](../extension.toml) and [Cargo.toml](../Cargo.toml), and get that merged.
2. Push a tag matching the new version, e.g. `git tag v0.0.3 && git push origin v0.0.3`.

The workflow opens a PR against the Zed extensions repository, which is released once merged. It requires the
`ZED_EXTENSIONS_FORK` repository variable (a fork of `zed-industries/extensions`, e.g. `some-user/extensions`) and the
`COMMITTER_TOKEN` secret (a personal access token with `repo` and `workflow` scopes that can push to that fork).

## Community

Finally, if you're interested in discussing a feature, bug, or just development in general, please join us in the
OPA community on [Slack](https://slack.openpolicyagent.org)!
