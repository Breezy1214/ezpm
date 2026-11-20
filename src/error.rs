use std::fmt;

/// What went wrong, which decides the exit code and the `kind` in a JSON error document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// The command line is wrong (exit 2).
    Usage,
    /// The project has errors (exit 1).
    Project,
    /// An external tool is missing or failed (exit 1).
    Tool,
    /// The command already printed what went wrong; exit with this code and print nothing more.
    Reported(i32),
}

#[derive(Debug)]
pub struct EzpmError {
    pub kind: ErrorKind,
    pub message: String,
    pub hint: Option<String>,
}

impl EzpmError {
    fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            hint: None,
        }
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Usage, message)
    }

    pub fn project(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Project, message)
    }

    pub fn tool(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Tool, message)
    }

    /// A failure the command has already reported (diagnostics, a JSON document, a child's output).
    pub fn reported(message: impl Into<String>, exit_code: i32) -> Self {
        Self::new(ErrorKind::Reported(exit_code), message)
    }

    /// Exit with `exit_code` and print nothing more: the command already said everything.
    pub fn exit(exit_code: i32) -> Self {
        Self::reported(String::new(), exit_code)
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn exit_code(&self) -> i32 {
        match self.kind {
            ErrorKind::Usage => 2,
            ErrorKind::Project | ErrorKind::Tool => 1,
            ErrorKind::Reported(code) => code,
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            ErrorKind::Usage => "usage",
            ErrorKind::Project | ErrorKind::Reported(_) => "project",
            ErrorKind::Tool => "tool",
        }
    }
}

impl fmt::Display for EzpmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for EzpmError {}
