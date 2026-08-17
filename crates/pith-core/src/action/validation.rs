use pith_diag::{Diag, EngineCode, Span};

use super::{ActionProgram, ActionSpec, NetworkPolicy, PlatformRequirement};

impl ActionSpec {
    /// Validate the complete declared contract.
    ///
    /// # Errors
    /// Returns `E-1105` when a path, argument, environment entry, platform,
    /// capability, or network host is invalid or ambiguous.
    pub fn validate(&self) -> Result<(), Diag> {
        self.validate_arguments()?;
        self.validate_program()?;
        self.validate_toolchain()?;
        self.validate_input_and_output_paths()?;
        self.validate_environment()?;
        self.validate_platform()?;
        self.validate_capabilities()?;
        self.validate_network()
    }

    fn validate_arguments(&self) -> Result<(), Diag> {
        if self
            .arguments
            .iter()
            .any(|argument| argument.contains('\0'))
        {
            return Err(invalid_action_spec("action argument contains a NUL byte"));
        }
        Ok(())
    }

    fn validate_program(&self) -> Result<(), Diag> {
        if let ActionProgram::HostPath(path) = &self.executable
            && !is_valid_host_path(path)
        {
            return Err(invalid_action_spec(format!(
                "invalid action executable path `{path}`"
            )));
        }
        Ok(())
    }

    fn validate_toolchain(&self) -> Result<(), Diag> {
        for (position, path) in self.toolchain.iter().enumerate() {
            if !is_valid_host_path(path) {
                return Err(invalid_action_spec(format!(
                    "invalid action toolchain path `{path}`"
                )));
            }
            if self
                .toolchain
                .iter()
                .take(position)
                .any(|previous| previous == path)
            {
                return Err(invalid_action_spec(format!(
                    "duplicate action toolchain path `{path}`"
                )));
            }
        }
        Ok(())
    }

    fn validate_input_and_output_paths(&self) -> Result<(), Diag> {
        validate_paths(self.inputs.iter().map(|input| input.path.as_ref()), "input")?;
        validate_paths(
            self.outputs.iter().map(|output| output.path.as_ref()),
            "output",
        )?;
        for input in &self.inputs {
            for output in &self.outputs {
                if paths_overlap(&input.path, &output.path) {
                    return Err(invalid_action_spec(format!(
                        "action input `{}` overlaps output `{}`",
                        input.path, output.path
                    )));
                }
            }
        }
        Ok(())
    }

    fn validate_environment(&self) -> Result<(), Diag> {
        for (position, variable) in self.environment.iter().enumerate() {
            if variable.name.is_empty()
                || variable.name.contains('=')
                || variable.name.contains('\0')
            {
                return Err(invalid_action_spec(format!(
                    "invalid action environment variable name `{}`",
                    variable.name
                )));
            }
            if variable.value.contains('\0') {
                return Err(invalid_action_spec(format!(
                    "action environment variable `{}` contains a NUL byte",
                    variable.name
                )));
            }
            if self
                .environment
                .iter()
                .take(position)
                .any(|previous| previous.name == variable.name)
            {
                return Err(invalid_action_spec(format!(
                    "duplicate action environment variable `{}`",
                    variable.name
                )));
            }
        }
        Ok(())
    }

    fn validate_platform(&self) -> Result<(), Diag> {
        if let PlatformRequirement::Exact {
            operating_system,
            architecture,
        } = &self.platform
            && (operating_system.is_empty() || architecture.is_empty())
        {
            return Err(invalid_action_spec(
                "exact action platform requires an operating system and architecture",
            ));
        }
        Ok(())
    }

    fn validate_capabilities(&self) -> Result<(), Diag> {
        for (position, capability) in self.capabilities.iter().enumerate() {
            if capability.name.is_empty()
                || capability.scope.is_empty()
                || capability.name.contains('\0')
                || capability.scope.contains('\0')
            {
                return Err(invalid_action_spec(
                    "action capability requires non-empty NUL-free name and scope",
                ));
            }
            if self
                .capabilities
                .iter()
                .take(position)
                .any(|previous| previous == capability)
            {
                return Err(invalid_action_spec(format!(
                    "duplicate action capability `{}` scoped to `{}`",
                    capability.name, capability.scope
                )));
            }
        }
        Ok(())
    }

    fn validate_network(&self) -> Result<(), Diag> {
        if let NetworkPolicy::AllowHosts(hosts) = &self.network {
            for (position, host) in hosts.iter().enumerate() {
                if host.is_empty() || host.contains('\0') {
                    return Err(invalid_action_spec(
                        "allowed network host must be non-empty and NUL-free",
                    ));
                }
                if hosts.iter().take(position).any(|previous| previous == host) {
                    return Err(invalid_action_spec(format!(
                        "duplicate allowed network host `{host}`"
                    )));
                }
            }
        }
        Ok(())
    }
}

fn validate_paths<'path>(paths: impl Iterator<Item = &'path str>, kind: &str) -> Result<(), Diag> {
    let paths: Vec<_> = paths.collect();
    for (position, path) in paths.iter().enumerate() {
        if !is_valid_action_path(path) {
            return Err(invalid_action_spec(format!(
                "invalid action {kind} path `{path}`"
            )));
        }
        if let Some(previous) = paths
            .iter()
            .take(position)
            .find(|previous| paths_overlap(previous, path))
        {
            return Err(invalid_action_spec(format!(
                "action {kind} path `{path}` overlaps `{previous}`"
            )));
        }
    }
    Ok(())
}

fn is_valid_action_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.ends_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && path
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn is_valid_host_path(path: &str) -> bool {
    !path.is_empty()
        && path.starts_with('/')
        && !path.contains('\0')
        && !path.contains('\\')
        && path
            .split('/')
            .skip(1)
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn paths_overlap(left: &str, right: &str) -> bool {
    left == right
        || right
            .strip_prefix(left)
            .is_some_and(|suffix| suffix.starts_with('/'))
        || left
            .strip_prefix(right)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn invalid_action_spec(message: impl Into<Box<str>>) -> Diag {
    Diag::engine(EngineCode::InvalidActionSpec, Span::none(), message)
}
