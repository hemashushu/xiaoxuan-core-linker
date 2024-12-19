// Copyright (c) 2024 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions,
// more details in file LICENSE, LICENSE.additional and CONTRIBUTING.

use anc_image::entry::{
    FunctionNameEntry, ImportFunctionEntry, ImportModuleEntry, LocalVariableListEntry, TypeEntry,
};
use anc_isa::ModuleDependency;

use crate::{merger::RemapIndices, LinkErrorType, LinkerError};

pub fn merge_type_entries(
    type_entries_list: &[&[TypeEntry]],
    // remap_module_list: &mut [ModuleEntryRemap],
) -> (Vec<TypeEntry>, Vec<RemapIndices>) {
    // copy the first list
    let mut entries_merged = type_entries_list[0].to_vec();
    let mut type_remap_indices_list = vec![(0..entries_merged.len()).collect()];

    // merge remains
    for entries_source in &type_entries_list[1..] {
        let mut indices = vec![];

        // check each entry
        for entry_source in *entries_source {
            let pos_merged_opt = entries_merged.iter().position(|item| {
                item.params == entry_source.params && item.results == entry_source.results
            });

            match pos_merged_opt {
                Some(pos_merged) => {
                    // found exists
                    indices.push(pos_merged);
                }
                None => {
                    // add entry
                    let pos_new = entries_merged.len();
                    entries_merged.push(entry_source.to_owned());
                    indices.push(pos_new);
                }
            }
        }

        type_remap_indices_list.push(indices);
    }

    (entries_merged, type_remap_indices_list)
}

pub fn merge_local_variable_list_entries(
    local_variable_list_entries_list: &[&[LocalVariableListEntry]],
    // remap_module_list: &mut [ModuleEntryRemap],
) -> (Vec<LocalVariableListEntry>, Vec<RemapIndices>) {
    // copy the first list
    let mut entries_merged = local_variable_list_entries_list[0].to_vec();
    let mut local_variable_list_remap_indices_list = vec![(0..entries_merged.len()).collect()];

    // merge remains
    for entries_source in &local_variable_list_entries_list[1..] {
        let mut indices = vec![];

        // check each entry
        for entry_source in entries_source.iter() {
            let pos_merged_opt = entries_merged.iter().position(|item| {
                item.local_variable_entries == entry_source.local_variable_entries
            });

            match pos_merged_opt {
                Some(pos_merged) => {
                    // found exists
                    indices.push(pos_merged);
                }
                None => {
                    // add entry
                    let pos_new = entries_merged.len();
                    entries_merged.push(entry_source.to_owned());
                    indices.push(pos_new);
                }
            }
        }

        local_variable_list_remap_indices_list.push(indices);
    }

    (entries_merged, local_variable_list_remap_indices_list)
}

pub fn merge_import_module_entries(
    import_module_entries_list: &[&[ImportModuleEntry]],
    // remap_module_list: &mut [ModuleEntryRemap],
) -> Result<(Vec<ImportModuleEntry>, Vec<RemapIndices>), LinkerError> {
    // copy the first list
    let mut entries_merged = import_module_entries_list[0].to_vec();
    let mut import_module_remap_indices_list = vec![(0..entries_merged.len()).collect()];

    // merge remains
    for entries_source in &import_module_entries_list[1..] {
        let mut indices = vec![];

        // check each entry
        for entry_source in entries_source.iter() {
            let pos_merged_opt = entries_merged
                .iter()
                .position(|item| item.name == entry_source.name);

            match pos_merged_opt {
                Some(pos_merged) => {
                    let entry_merged = &entries_merged[pos_merged];
                    let module_name = &entry_merged.name;

                    let dependency_source = entry_source.value.as_ref();
                    let dependency_merged = entry_merged.value.as_ref();

                    if dependency_source == dependency_merged {
                        // identical
                    } else {
                        // further check
                        match dependency_source {
                            ModuleDependency::Local(_) => {
                                if matches!(dependency_merged, ModuleDependency::Local(_)) {
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
                                if matches!(dependency_merged, ModuleDependency::Remote(_)) {
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
                            ModuleDependency::Share(share_source) => {
                                if let ModuleDependency::Share(share_merged) = dependency_merged {
                                    // compare version
                                    match compare_version(
                                        &share_source.version,
                                        &share_merged.version,
                                    ) {
                                        VersionCompareResult::Equals
                                        | VersionCompareResult::LessThan => {
                                            // keep
                                        }
                                        VersionCompareResult::GreaterThan => {
                                            // replace
                                            entries_merged[pos_merged] = entry_source.clone()
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

                    indices.push(pos_merged);
                }
                None => {
                    // add entry
                    let pos_new = entries_merged.len();
                    entries_merged.push(entry_source.to_owned());
                    indices.push(pos_new);
                }
            }
        }

        // let remap_item = &mut remap_module_list[submodule_index];
        // remap_item.import_module_index = indices;
        import_module_remap_indices_list.push(indices);
    }

    Ok((entries_merged, import_module_remap_indices_list))
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

// #[derive(Debug,PartialEq, Clone)]
// struct ImportFunctionRemap {
//     items: Vec<ImportFunctionRemapItem>,
// }

enum ImportFunctionRemapItem {
    Import(usize),
    Internal(usize),
}

pub fn merge_import_function_entries(
    function_name_entries: &[FunctionNameEntry],
    import_function_entries: &[&[ImportFunctionEntry]],
    // remap_module_list: &mut [ModuleEntryRemap],
    import_module_remap_indices_list: &[RemapIndices],
    type_remap_indices_list: &[RemapIndices],
) -> (Vec<ImportFunctionEntry>, Vec<RemapIndices>) {
    // note:
    // - when adding new `ImportFunctionEntry`, the propertries "import_module_index"
    //   and "type_index" need to be updated.
    // - when merging functions, only the "fullname" will be used to determine if
    //   the functions are the same or not, and the module in which the functions
    //   reside will be ignored.

    let mut entries_merged: Vec<ImportFunctionEntry> = vec![];
    let mut import_function_remap_items_list: Vec<Vec<ImportFunctionRemapItem>> = vec![];

    // merge import function list
    for (submodule_index, entries_source) in import_function_entries.iter().enumerate() {
        let mut indices: Vec<ImportFunctionRemapItem> = vec![];

        // check each entry
        for entry_source in entries_source.iter() {
            // check the internal function list first
            let pos_internal_opt = function_name_entries
                .iter()
                .position(|item| item.full_name == entry_source.full_name);

            if let Some(pos_internal) = pos_internal_opt {
                // the target is a internal function, instead of imported function
                indices.push(ImportFunctionRemapItem::Internal(pos_internal));
            } else {
                // the target is an imported function

                // check the merged list first
                let pos_merged_opt = entries_merged
                    .iter()
                    .position(|item| item.full_name == entry_source.full_name);

                match pos_merged_opt {
                    Some(pos_merged) => {
                        // found exists
                        indices.push(ImportFunctionRemapItem::Import(pos_merged));
                    }
                    None => {
                        // add entry
                        let pos_new = entries_merged.len();
                        let entry_merged = ImportFunctionEntry::new(
                            entry_source.full_name.to_owned(),
                            import_module_remap_indices_list[submodule_index]
                                [entry_source.import_module_index],
                            type_remap_indices_list[submodule_index][entry_source.type_index],
                        );
                        entries_merged.push(entry_merged);
                        indices.push(ImportFunctionRemapItem::Import(pos_new));
                    }
                }
            }
        }

        import_function_remap_items_list.push(indices);
    }

    // todo
    // build the function public index remap list
    let mut function_public_remap_indices_list: Vec<RemapIndices> = vec![];

    (entries_merged, function_public_remap_indices_list)
}

#[cfg(test)]
mod tests {
    use anc_assembler::assembler::create_self_dependent_import_module_entry;
    use anc_image::entry::ImportModuleEntry;
    use anc_isa::{DependencyShare, ModuleDependency};

    use super::merge_import_module_entries;

    #[test]
    fn test_merge_type_entries() {
        //
    }

    #[test]
    fn test_merge_local_variable_list_entries() {
        //
    }

    #[test]
    fn test_merge_import_module_entries() {
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
                // new item
                "gui".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    repository: None,
                    version: "1.3.4".to_owned(),
                    values: None,
                    condition: None,
                }))),
            ),
            ImportModuleEntry::new(
                // updated item
                "encoding".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    repository: None,
                    version: "2.2.0".to_owned(),
                    values: None,
                    condition: None,
                }))),
            ),
            ImportModuleEntry::new(
                // identical item
                "network".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    repository: None,
                    version: "1.0.1".to_owned(),
                    values: None,
                    condition: None,
                }))),
            ),
        ];

        let import_module_entries_list = vec![entries0.as_slice(), entries1.as_slice()];
        let (merged_module_entries_list, import_module_remap_indices_list) =
            merge_import_module_entries(&import_module_entries_list).unwrap();

        // check merged entries
        let expected_module_entries_list = vec![
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
            // this item should be the version "2.2.0" instead of "2.1.0".
            ImportModuleEntry::new(
                "encoding".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    repository: None,
                    version: "2.2.0".to_owned(),
                    values: None,
                    condition: None,
                }))),
            ),
            // this item is new added.
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

        assert_eq!(merged_module_entries_list, expected_module_entries_list);

        // check remap list
        assert_eq!(import_module_remap_indices_list[0], vec![0, 1, 2]);
        assert_eq!(import_module_remap_indices_list[1], vec![0, 3, 2, 1]);
    }

    #[test]
    fn test_merge_import_module_entries_with_name_conflict() {
        // todo
    }

    #[test]
    fn test_merge_import_module_entries_with_source_conflict() {
        // todo
    }

    #[test]
    fn test_merge_import_module_entries_with_major_version_conflict() {
        // todo
    }
}
