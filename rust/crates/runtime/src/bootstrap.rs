#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapPhase {
    CliEntry,
    FastPathVersion,
    StartupProfiler,
    SystemPromptFastPath,
    ChromeMcpFastPath,
    DaemonWorkerFastPath,
    BridgeFastPath,
    DaemonFastPath,
    BackgroundSessionFastPath,
    TemplateFastPath,
    EnvironmentRunnerFastPath,
    MainRuntime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapPlan {
    phases: Vec<BootstrapPhase>,
}

impl BootstrapPlan {
    #[must_use]
    pub fn default_bootstrap() -> Self {
        Self::from_phases(vec![
            BootstrapPhase::CliEntry,
            BootstrapPhase::FastPathVersion,
            BootstrapPhase::StartupProfiler,
            BootstrapPhase::SystemPromptFastPath,
            BootstrapPhase::ChromeMcpFastPath,
            BootstrapPhase::DaemonWorkerFastPath,
            BootstrapPhase::BridgeFastPath,
            BootstrapPhase::DaemonFastPath,
            BootstrapPhase::BackgroundSessionFastPath,
            BootstrapPhase::TemplateFastPath,
            BootstrapPhase::EnvironmentRunnerFastPath,
            BootstrapPhase::MainRuntime,
        ])
    }

    #[must_use]
    pub fn from_phases(phases: Vec<BootstrapPhase>) -> Self {
        let mut deduped = Vec::new();
        for phase in phases {
            if !deduped.contains(&phase) {
                deduped.push(phase);
            }
        }
        Self { phases: deduped }
    }

    #[must_use]
    pub fn phases(&self) -> &[BootstrapPhase] {
        &self.phases
    }
}
