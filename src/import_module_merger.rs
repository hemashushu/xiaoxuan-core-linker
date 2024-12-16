// Copyright (c) 2024 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions,
// more details in file LICENSE, LICENSE.additional and CONTRIBUTING.

use anc_image::entry::ImportModuleEntry;
use anc_isa::ModuleDependency;

use crate::{LinkErrorType, LinkerError};

pub fn merge_import_modules(
    import_module_list: &[&Vec<ImportModuleEntry>],
) -> Result<Vec<ImportModuleEntry>, LinkerError> {
    // copy the first list of imported modules.
    let mut merged_entries = import_module_list[0].to_vec();

    for entries in &import_module_list[1..] {
        // merge each list
        for entry_new in *entries {
            // check each entry
            let pos_opt = merged_entries
                .iter()
                .position(|item| item.name == entry_new.name);
            match pos_opt {
                Some(pos) => {
                    let entry_exist = &merged_entries[pos];
                    let module_name = &entry_exist.name;

                    let dependency_new = entry_new.value.as_ref();
                    let dependency_exist = entry_exist.value.as_ref();

                    if dependency_new == dependency_exist {
                        // identical
                    } else {
                        // further check
                        match dependency_new {
                            ModuleDependency::Local(_) => {
                                if matches!(dependency_exist, ModuleDependency::Local(_)) {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentModuleCannotMerge(
                                            module_name.to_owned(),
                                        ),
                                    ));
                                } else {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentModuleNameConflict(
                                            module_name.to_owned(),
                                        ),
                                    ));
                                }
                            }
                            ModuleDependency::Remote(_) => {
                                if matches!(dependency_exist, ModuleDependency::Remote(_)) {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentModuleCannotMerge(
                                            module_name.to_owned(),
                                        ),
                                    ));
                                } else {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentModuleNameConflict(
                                            module_name.to_owned(),
                                        ),
                                    ));
                                }
                            }
                            ModuleDependency::Share(share_new) => {
                                if let ModuleDependency::Share(share_exist) = dependency_exist {
                                    // compare version
                                    match compare_version(&share_new.version, &share_exist.version)
                                    {
                                        VersionCompareResult::Equals
                                        | VersionCompareResult::LessThan => {
                                            // keep
                                        }
                                        VersionCompareResult::GreaterThan => {
                                            // replace
                                            merged_entries[pos] = entry_new.clone()
                                        }
                                        VersionCompareResult::MajorDifferent => {
                                            return Err(LinkerError::new(
                                                LinkErrorType::DependentModuleMajorVersionConflict(
                                                    module_name.to_owned(),
                                                ),
                                            ));
                                        }
                                    }
                                } else {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentModuleNameConflict(
                                            module_name.to_owned(),
                                        ),
                                    ));
                                }
                            }
                            ModuleDependency::Runtime => {
                                return Err(LinkerError::new(
                                    LinkErrorType::DependentModuleNameConflict(
                                        module_name.to_owned(),
                                    ),
                                ))
                            }
                            ModuleDependency::Current => {
                                return Err(LinkerError::new(
                                    LinkErrorType::DependentModuleNameConflict(
                                        module_name.to_owned(),
                                    ),
                                ))
                            }
                        }
                    }
                }
                None => {
                    // add entry
                    merged_entries.push(entry_new.clone());
                }
            }
        }
    }

    Ok(merged_entries)
}

enum VersionCompareResult {
    Equals,
    GreaterThan,
    LessThan,
    MajorDifferent,
}

fn compare_version(left: &str, right: &str) -> VersionCompareResult {
    let left_parts = left
        .split('.')
        .map(|item| u16::from_str_radix(item, 10).unwrap())
        .collect::<Vec<_>>();

    let right_parts = right
        .split('.')
        .map(|item| u16::from_str_radix(item, 10).unwrap())
        .collect::<Vec<_>>();

    if left_parts[0] != right_parts[0] {
        VersionCompareResult::MajorDifferent
    } else if left_parts[1] > right_parts[1] {
        VersionCompareResult::GreaterThan
    } else if left_parts[1] < right_parts[1] {
        VersionCompareResult::LessThan
    } else if left_parts[2] > right_parts[2] {
        VersionCompareResult::GreaterThan
    } else if left_parts[2] < right_parts[2] {
        VersionCompareResult::LessThan
    } else {
        VersionCompareResult::Equals
    }
}

#[cfg(test)]
mod tests {
    use anc_assembler::assembler::create_self_dependent_import_module_entry;
    use anc_image::entry::ImportModuleEntry;
    use anc_isa::{DependencyShare, ModuleDependency};

    use super::merge_import_modules;

    #[test]
    fn test_merge_import_modules() {
        let entries0 = vec![
            create_self_dependent_import_module_entry(),
            ImportModuleEntry::new(
                "network".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    repository: None,
                    version: "1.0.1".to_owned(),
                    values: None,
                    condition: None,
                }))),
            ),
            ImportModuleEntry::new(
                "encoding".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    repository: None,
                    version: "2.1.0".to_owned(),
                    values: None,
                    condition: None,
                }))),
            ),
        ];

        let entries1 = vec![
            create_self_dependent_import_module_entry(),
            ImportModuleEntry::new(
                "network".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    repository: None,
                    version: "1.0.1".to_owned(),
                    values: None,
                    condition: None,
                }))),
            ),
            ImportModuleEntry::new(
                "encoding".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    repository: None,
                    version: "2.2.0".to_owned(),
                    values: None,
                    condition: None,
                }))),
            ),
            ImportModuleEntry::new(
                "gui".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    repository: None,
                    version: "1.3.4".to_owned(),
                    values: None,
                    condition: None,
                }))),
            ),
        ];

        let import_module_list = vec![&entries0, &entries1];
        let merged_module_list = merge_import_modules(&import_module_list).unwrap();

        let expected_module_list = vec![
            create_self_dependent_import_module_entry(),
            ImportModuleEntry::new(
                "network".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    repository: None,
                    version: "1.0.1".to_owned(),
                    values: None,
                    condition: None,
                }))),
            ),
            ImportModuleEntry::new(
                "encoding".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    repository: None,
                    version: "2.2.0".to_owned(),
                    values: None,
                    condition: None,
                }))),
            ),
            ImportModuleEntry::new(
                "gui".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    repository: None,
                    version: "1.3.4".to_owned(),
                    values: None,
                    condition: None,
                }))),
            ),
        ];

        assert_eq!(merged_module_list, expected_module_list);
    }

    #[test]
    fn test_merge_import_modules_with_name_conflict() {
        // todo
    }

    #[test]
    fn test_merge_import_modules_with_source_conflict() {
        // todo
    }

    #[test]
    fn test_merge_import_modules_with_major_version_conflict() {
        // todo
    }
}
