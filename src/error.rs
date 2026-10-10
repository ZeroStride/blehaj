use miette::Diagnostic;
use thiserror::Error;

#[allow(clippy::enum_variant_names)]
#[derive(Diagnostic, Debug, Error)]
pub enum Error {
    #[error("Error: {description}")]
    Actionable {
        description: String,

        #[diagnostic(code())]
        code: Option<String>,

        #[help]
        advice: Option<String>,

        #[source]
        source: Option<Box<dyn std::error::Error + 'static>>,
    },

    #[error(transparent)]
    Zbus(#[from] zbus::Error),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

impl Error {}

unsafe impl Send for Error {}
unsafe impl Sync for Error {}
