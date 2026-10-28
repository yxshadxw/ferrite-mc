use std::path::PathBuf;

/// Errors produced while loading, parsing, or migrating server configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A configuration file could not be read or written.
    #[error("i/o error for {path}: {source}")]
    Io {
        /// The file that failed.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A YAML file is not syntactically valid or does not match the expected shape.
    #[error("invalid YAML in {path}: {source}")]
    Yaml {
        /// The file that failed.
        path: PathBuf,
        /// The underlying parser error.
        #[source]
        source: serde_norway::Error,
    },

    /// A `server.properties` value has the wrong type.
    #[error("invalid value for server.properties key `{key}`: {message}")]
    Property {
        /// The offending key.
        key: String,
        /// Why the value was rejected.
        message: String,
    },

    /// A dotted override path does not point into the configuration model.
    #[error("invalid override `{path}`: {message}")]
    Override {
        /// The dotted path that was supplied.
        path: String,
        /// Why the override was rejected.
        message: String,
    },

    /// A typed configuration could not be serialized.
    #[error("failed to serialize configuration: {message}")]
    Serialize {
        /// Why serialization failed.
        message: String,
    },
}

impl ConfigError {
    /// Attach a path to an I/O error.
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    /// Attach a path to a YAML error.
    pub(crate) fn yaml(path: impl Into<PathBuf>, source: serde_norway::Error) -> Self {
        Self::Yaml {
            path: path.into(),
            source,
        }
    }

    /// Wrap a serialization error.
    pub(crate) fn serialize(message: impl Into<String>) -> Self {
        Self::Serialize {
            message: message.into(),
        }
    }
}
