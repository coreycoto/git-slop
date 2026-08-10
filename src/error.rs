use std::fmt;

use serde::Serialize;
use serde_json::{Map, Value};
use thiserror::Error;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Contract,
    Repository,
    ResourceLimit,
    Io,
}

impl ErrorKind {
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Contract => 2,
            Self::Repository | Self::Io => 3,
            Self::ResourceLimit => 4,
        }
    }
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct ClassifiedError {
    pub kind: ErrorKind,
    pub code: &'static str,
    pub pointer: Option<String>,
    pub message: String,
    pub details: Value,
}

impl ClassifiedError {
    pub fn new(kind: ErrorKind, code: &'static str, message: impl fmt::Display) -> Self {
        Self {
            kind,
            code,
            pointer: None,
            message: message.to_string(),
            details: Value::Object(Map::new()),
        }
    }

    pub fn at(mut self, pointer: impl Into<String>) -> Self {
        self.pointer = Some(pointer.into());
        self
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }
}
