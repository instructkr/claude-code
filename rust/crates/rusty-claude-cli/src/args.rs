use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Parser, PartialEq, Eq)]
#[command(
    name = "claw-cli",
    version,
    about = "Rust Claude CLI prototype"
)]
pub struct Cli {
    #[arg(long, default_value = "claude-opus-4-6")]
    pub model: String,

    #[arg(long, value_enum, default_value_t = PermissionMode::DangerFullAccess)]
    pub permission_mode: PermissionMode,

    #[arg(long)]
    pub config: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub output_format: OutputFormat,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Clone, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// Read upstream TS sources and print extracted counts
    DumpManifests,
    /// Print the current bootstrap phase skeleton
    BootstrapPlan,
    /// Start the OAuth login flow
    Login,
    /// Clear saved OAuth credentials
    Logout,
    /// Run a non-interactive prompt and exit
    Prompt { prompt: Vec<String> },
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum PermissionMode {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
    Ndjson,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Command, OutputFormat, PermissionMode};

    #[test]
    fn parses_requested_flags() {
        let cli = Cli::parse_from([
            "claw-cli",
            "--model",
            "claude-3-5-haiku",
            "--permission-mode",
            "read-only",
            "--config",
            "/tmp/config.toml",
            "--output-format",
            "ndjson",
            "prompt",
            "hello",
            "world",
        ]);

        assert_eq!(cli.model, "claude-3-5-haiku");
        assert_eq!(cli.permission_mode, PermissionMode::ReadOnly);
        assert_eq!(
            cli.config.as_deref(),
            Some(std::path::Path::new("/tmp/config.toml"))
        );
        assert_eq!(cli.output_format, OutputFormat::Ndjson);
        assert_eq!(
            cli.command,
            Some(Command::Prompt {
                prompt: vec!["hello".into(), "world".into()]
            })
        );
    }

    #[test]
    fn parses_login_and_logout_commands() {
        let login = Cli::parse_from(["claw-cli", "login"]);
        assert_eq!(login.command, Some(Command::Login));

        let logout = Cli::parse_from(["claw-cli", "logout"]);
        assert_eq!(logout.command, Some(Command::Logout));
    }

    #[test]
    fn defaults_to_danger_full_access_permission_mode() {
        let cli = Cli::parse_from(["claw-cli"]);
        assert_eq!(cli.permission_mode, PermissionMode::DangerFullAccess);
    }
}
