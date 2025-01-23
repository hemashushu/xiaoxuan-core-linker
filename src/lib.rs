// Copyright (c) 2024 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions,
// more details in file LICENSE, LICENSE.additional and CONTRIBUTING.

pub mod indexer;
pub mod linker;

use std::fmt::Display;

use anc_isa::{DataSectionType, MemoryDataType};

pub const DEFAULT_ENTRY_FUNCTION_NAME: &str = "_start";

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

    /// The specified function canot be found.
    FunctionNotFound(/* function name */ String),

    /// The specified function is not public/exported.
    FunctionNotExported(/* function name */ String),

    ImportFunctionTypeMismatch(/* function name */ String),
    ImportFunctionTypeInconsistant(/* function name */ String),

    /// The specified data cannot be found.
    DataNotFound(/* data name */ String),

    /// The specified data is not public/exported.
    DataNotExported(/* data name */ String),

    ImportDataSectionMismatch(
        /* data name */ String,
        /* expected data section type */ DataSectionType,
    ),

    ImportDataSectionInconsistant(/* data name */ String),

    ImportDataTypeMismatch(
        /* data name */ String,
        /* expected data memory data type */ MemoryDataType,
    ),

    ImportDataTypeInconsistant(/* data name */ String),

    ExternalFunctionTypeInconsistent(/* external function name */ String),
    ExternalDataTypeInconsistent(/* external data name */ String),
    // EntryPointNotFound(/* expected_entry_point_name */ String),
}

impl LinkerError {
    pub fn new(error_type: LinkErrorType) -> Self {
        Self { error_type }
    }
}

impl Display for LinkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.error_type {
            LinkErrorType::CannotLoadMoudle(module_name, message) => write!(f, "Failed to load module \"{module_name}\", message: \"{message}\""),
            LinkErrorType::DependentNameConflict(dependency_name) =>write!(f, "Dependent \"{dependency_name}\" cannot be merged because there are different types."),
            LinkErrorType::DependentSourceConflict(dependency_name) => write!(f, "Dependent \"{dependency_name}\" cannot be merged because the sources are different."),
            LinkErrorType::DependentVersionConflict(dependency_name) => write!(f, "Dependent \"{dependency_name}\" cannot be merged because the major versions are different."),

            LinkErrorType::FunctionNotFound(function_name) => write!(f, "The specified function \"{function_name}\" cannot be found."),
            LinkErrorType::FunctionNotExported(function_name) => write!(f, "The specified function \"{function_name}\" is not exported."),
            LinkErrorType::ImportFunctionTypeMismatch(function_name) => write!(f, "The type of the imported function \"{function_name}\" does not match the actual type."),
            LinkErrorType::ImportFunctionTypeInconsistant(function_name) => write!(f, "The type of the imported function \"{function_name}\" is inconsistant."),

            LinkErrorType::DataNotFound(data_name) => write!(f, "The specified data \"{data_name}\" cannot be found."),
            LinkErrorType::DataNotExported(data_name) => write!(f, "The specified data \"{data_name}\" is not exported."),
            LinkErrorType::ImportDataSectionMismatch(data_name, expected_data_section_type) => write!(f, "The section of imported data \"{data_name}\" expects {expected_data_section_type}."),
            LinkErrorType::ImportDataSectionInconsistant(data_name) => write!(f, "The section of imported data \"{data_name}\" is inconsistant."),
            LinkErrorType::ImportDataTypeMismatch(data_name, expected_memory_data_type) => write!(f, "The expected data type of the imported data \"{data_name}\" is \"{expected_memory_data_type}\"."),
            LinkErrorType::ImportDataTypeInconsistant(data_name) => write!(f, "The data type of of imported data \"{data_name}\" is inconsistant."),

            LinkErrorType::ExternalFunctionTypeInconsistent(external_function_name) => write!(f, "Inconsistent type of the external function \"{external_function_name}\"."),
            LinkErrorType::ExternalDataTypeInconsistent(external_data_name) => write!(f, "Inconsistent type of the external data \"{external_data_name}\"."),

            // LinkErrorType::EntryPointNotFound(expected_entry_point_name) => write!(f, "The entry point function \"{expected_entry_point_name}\" does not found."),
        }
    }
}

impl std::error::Error for LinkerError {}
