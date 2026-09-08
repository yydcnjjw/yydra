<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Yydra CLI and template-generation contract research

_Research date: 2026-08-31. Scope: the official documentation linked below. This note records facts and bounded design inferences; it does not choose Yydra's contract._

## Fact table

| Area | Verified fact | Official source |
| --- | --- | --- |
| Cargo installation | `cargo install` builds and installs executable targets into an installation root's `bin` directory. Registry installs can select an exact version; Git installs can select a branch, tag, or revision. `--locked` uses the packaged `Cargo.lock` when available instead of re-resolving dependencies. | [Cargo: `cargo install`](https://doc.rust-lang.org/cargo/commands/cargo-install.html) |
| Cargo external tools | A command such as `cargo foo` invokes an external executable named `cargo-foo`; Cargo searches `$CARGO_HOME/bin` before `PATH` and forwards the remaining arguments. Cargo recommends using its CLI and `cargo metadata` rather than depending on Cargo as a library. | [Cargo: External tools](https://doc.rust-lang.org/cargo/reference/external-tools.html#custom-subcommands) |
| Cargo aliases | `[alias]` entries in Cargo configuration expand a name to a Cargo command and arguments. Aliases cannot replace built-in Cargo commands. | [Cargo: `alias` configuration](https://doc.rust-lang.org/cargo/reference/config.html#alias) |
| `cargo-generate` inputs | `cargo generate` can use a Git repository or local directory as a template. Its documented CLI supports selecting Git branch, tag, or revision, choosing a subfolder, supplying a config file or values file, defining values, and running non-interactively. | [`cargo-generate` introduction](https://cargo-generate.github.io/cargo-generate/) |
| `cargo-generate` placeholders | Templates use Liquid substitution. Template-defined placeholders can specify types, prompts, choices, defaults, and string validation. Values may come from CLI definitions, values files, environment variables, user/favorite configuration, or template defaults, with a documented precedence order; built-in placeholders also expose project and host-derived values. | [`cargo-generate`: template-defined placeholders](https://cargo-generate.github.io/cargo-generate/templates/template_defined_placeholders.html), [`cargo-generate`: built-in placeholders](https://cargo-generate.github.io/cargo-generate/templates/builtin_placeholders.html) |
| `cargo-generate` hooks | Templates may define Rhai `init`, `pre`, and `post` hooks. Hook extensions include environment/date access and system-command execution; commands require interactive approval unless `--allow-commands` is supplied. | [`cargo-generate`: hook types](https://cargo-generate.github.io/cargo-generate/templates/scripting.hook-types.html), [`cargo-generate`: Rhai extensions](https://cargo-generate.github.io/cargo-generate/templates/scripting.rhai-extensions.html) |
| `just` | `just` is a command runner, not a build system. Recipes are project-specific commands; dependencies run before dependents. Ordinary recipe lines run in separate shells, while shebang recipes run as one script, and the shell can be configured. | [`just` manual](https://just.systems/man/en/), [`just`: dependencies](https://just.systems/man/en/dependencies.html), [`just`: shell](https://just.systems/man/en/shell.html) |
| Expo CNG | `npx expo prebuild` generates `android/` and `ios/` from an Expo native template and applies config plugins. `--clean` deletes and regenerates those directories; Expo warns that manual changes there can be lost. EAS Build runs Prebuild when the native directories are absent and skips it when they are present. | [Expo: Continuous Native Generation](https://docs.expo.dev/workflow/continuous-native-generation/) |
| Orval output ownership | `output.clean` is opt-in and, when enabled, cleans the configured `target` and `schemas` locations before generation; array form adds glob rules. `output.formatter` can select a supported formatter for generated output. | [Orval: Output configuration](https://orval.dev/docs/reference/configuration/output/) |
| Orval CLI controls | The Orval CLI accepts a config path and project selection. It exposes `--clean`, `--formatter`, `--fail-on-warnings`, and `--verbose`; `--fail-on-warnings` exits with status 1 when warnings occur. | [Orval: CLI reference](https://orval.dev/docs/reference/cli/) |

## Design inferences, not facts

- A reproducible-generation contract would need to pin the generator/tool version, immutable template identity, selected subtemplate, effective placeholder values and configuration, hook policy, and dependency locks; a template URL alone does not identify all documented inputs.
- Template identity and template trust are separate: pinning a revision fixes source identity, while hooks can still observe environment/date inputs or execute commands.
- An installed Cargo subcommand, a repository Cargo alias, and `just` operate at different scopes. The documentation does not require Yydra to combine them or choose only one.
- Clean regeneration implies explicit ownership boundaries: persistent Expo native changes need declarative inputs outside regenerated directories, and Orval-cleaned paths should not contain hand-maintained files.
- Formatting generated output and failing on generator warnings are independent policy choices.
