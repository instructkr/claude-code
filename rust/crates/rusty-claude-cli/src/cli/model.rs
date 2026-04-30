//! Model resolution and provenance tracking.

use std::env;

use runtime::ConfigLoader;

use crate::DEFAULT_MODEL;

/// #148: Model provenance for `claw status` JSON/text output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSource {
    /// Explicit `--model` / `--model=` CLI flag.
    Flag,
    /// ANTHROPIC_MODEL environment variable.
    Env,
    /// `model` key in config file.
    Config,
    /// Compiled-in default fallback.
    Default,
}

impl ModelSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelSource::Flag => "flag",
            ModelSource::Env => "env",
            ModelSource::Config => "config",
            ModelSource::Default => "default",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelProvenance {
    /// Resolved model string (after alias expansion).
    pub resolved: String,
    /// Raw user input before alias resolution. None when source is Default.
    pub raw: Option<String>,
    /// Where the model came from.
    pub source: ModelSource,
}

impl ModelProvenance {
    pub fn new(resolved: String, source: ModelSource) -> Self {
        Self {
            resolved,
            raw: None,
            source,
        }
    }

    pub fn default_fallback() -> Self {
        Self {
            resolved: DEFAULT_MODEL.to_string(),
            raw: None,
            source: ModelSource::Default,
        }
    }

    pub fn from_flag(raw: &str) -> Self {
        Self {
            resolved: resolve_model_alias(raw).to_string(),
            raw: Some(raw.to_string()),
            source: ModelSource::Flag,
        }
    }

    pub fn from_env_or_config_or_default(cli_model: &str) -> Self {
        if cli_model != DEFAULT_MODEL {
            return Self {
                resolved: cli_model.to_string(),
                raw: Some(cli_model.to_string()),
                source: ModelSource::Flag,
            };
        }
        if let Some(env_model) = env::var("ANTHROPIC_MODEL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            return Self {
                resolved: resolve_model_alias(&env_model).to_string(),
                raw: Some(env_model),
                source: ModelSource::Env,
            };
        }
        if let Some(config_model) = config_model_for_current_dir() {
            return Self {
                resolved: resolve_model_alias(&config_model).to_string(),
                raw: Some(config_model),
                source: ModelSource::Config,
            };
        }
        Self::default_fallback()
    }

    pub fn default_model(model: String) -> Self {
        Self {
            resolved: model,
            raw: None,
            source: ModelSource::Default,
        }
    }
}

/// Resolve model alias to canonical form.
pub fn resolve_model_alias(model: &str) -> &str {
    match model {
        "opus" | "claude-opus" => "claude-opus-4-6",
        "sonnet" | "claude-sonnet" => "claude-sonnet-4-6",
        "haiku" | "claude-haiku" => "claude-haiku-4-5-20251001",
        _ => model,
    }
}

/// Validate model syntax.
pub fn validate_model_syntax(model: &str) -> Result<(), String> {
    if model.is_empty() {
        return Err("model cannot be empty".into());
    }
    if model.len() > 128 {
        return Err("model name too long".into());
    }
    Ok(())
}

/// Get model from config for current directory.
pub fn config_model_for_current_dir() -> Option<String> {
    let cwd = env::current_dir().ok()?;
    let loader = ConfigLoader::default_for(&cwd);
    let config = loader.load().ok()?;
    config.model().map(ToOwned::to_owned)
}
