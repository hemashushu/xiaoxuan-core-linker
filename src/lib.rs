// Copyright (c) 2024 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions,
// more details in file LICENSE, LICENSE.additional and CONTRIBUTING.

pub mod merger;
pub mod linker;
pub mod object_reader;

use std::fmt::Display;

#[derive(Debug)]
pub struct LinkerError {
    pub error_type: LinkErrorType,
}

#[derive(Debug, PartialEq, Clone)]
pub enum LinkErrorType {
    CannotLoadMoudle(/* module name */ String, /* message */ String),

    /// Modules/libraries with the same name but different types.
    DependentNameConflict(/* module/library name */ String),

    /// Modules/libraries that lack version information (such as "local" and "remote")
    /// cannot be merged if only the names are the same but the sources
    /// (e.g. file paths and commit/tags) do not match.
    DependentSourceConflict(/* module/library name */ String),

    /// Modules/libraries with the same name cannot be merged if
    /// their versions conflict.
    DependentVersionConflict(/* module/library name */ String),

    /// The imported function canot be found.
    UnresolvedFunctionName(/* function name */ String),

    /// The imported data cannot be found.
    UnresolvedDataName(/* data name */ String),
}

impl LinkerError {
    pub fn new(error_type: LinkErrorType) -> Self {
        Self { error_type }
    }
}

impl Display for LinkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self .error_type {
            LinkErrorType::CannotLoadMoudle(module_name, message) => write!(f, "Failed to load module \"{module_name}\", message: \"{message}\""),
            LinkErrorType::DependentNameConflict(module_name) =>write!(f, "Dependent modules \"{module_name}\" cannot be merged because there are different types."),
            LinkErrorType::DependentSourceConflict(module_name) => write!(f, "Dependent modules \"{module_name}\" cannot be merged because the sources are different."),
            LinkErrorType::DependentVersionConflict(module_name) => write!(f, "Dependent modules \"{module_name}\" cannot be merged because the major versions are different."),
            LinkErrorType::UnresolvedFunctionName(function_name) => write!(f, "The imported function \"{function_name}\" cannot be found."),
            LinkErrorType::UnresolvedDataName(data_name) => write!(f, "The imported data \"{data_name}\" cannot be found."),
        }
    }
}

impl std::error::Error for LinkerError {}
