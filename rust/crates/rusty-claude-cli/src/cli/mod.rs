//! CLI module - refactored from main.rs for modularity.

pub mod doctor;
pub mod model;
pub mod parse;

pub use doctor::{
    check_auth_health, check_config_health, check_install_source_health, check_sandbox_health,
    check_system_health, check_workspace_health, parse_git_status_metadata,
    parse_git_workspace_summary, render_doctor_report, run_doctor, DiagnosticCheck,
    DiagnosticLevel, DoctorReport, GitWorkspaceSummary, StatusContext, BUILD_TARGET,
    DEPRECATED_INSTALL_COMMAND, OFFICIAL_REPO_SLUG, OFFICIAL_REPO_URL,
};
pub use model::{
    config_model_for_current_dir, resolve_model_alias, validate_model_syntax, ModelProvenance,
    ModelSource,
};
pub use parse::{
    default_permission_mode, is_help_flag, normalize_allowed_tools, normalize_permission_mode,
    parse_args, parse_permission_mode_arg, permission_mode_from_label,
    permission_mode_from_resolved, ranked_suggestions, resolve_model_alias_with_config,
    AllowedToolSet, CliAction, CliOutputFormat, CLI_OPTION_SUGGESTIONS, LATEST_SESSION_REFERENCE,
    LocalHelpTopic,
};
