// Copyright (c) 2024 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions,
// more details in file LICENSE, LICENSE.additional and CONTRIBUTING.

use anc_image::{
    entry::{
        DataNameEntry, ExternalFunctionEntry, ExternalLibraryEntry, FunctionEntry,
        FunctionNameEntry, ImportDataEntry, ImportFunctionEntry, ImportModuleEntry,
        InitedDataEntry, LocalVariableListEntry, RelocateListEntry, TypeEntry, UninitDataEntry,
    },
    module_image::RelocateType,
};
use anc_isa::{DataSectionType, ExternalLibraryDependency, ModuleDependency};

use crate::{
    linker::{RemapIndices, RemapTable},
    LinkErrorType, LinkerError,
};

pub fn merge_type_entries(
    type_entries_list: &[&[TypeEntry]],
) -> (
    /* type_entries */ Vec<TypeEntry>,
    /* type_remap_indices_list */ Vec<RemapIndices>,
) {
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
) -> (
    /* local_variable_list_entries */ Vec<LocalVariableListEntry>,
    /* local_variable_list_remap_indices_list */ Vec<RemapIndices>,
) {
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
) -> Result<
    (
        /* import_module_entries */ Vec<ImportModuleEntry>,
        /* import_module_remap_indices_list */ Vec<RemapIndices>,
    ),
    LinkerError,
> {
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
                                        LinkErrorType::DependentSourceConflict(
                                            module_name.to_owned(),
                                        ),
                                    ));
                                } else {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentNameConflict(
                                            module_name.to_owned(),
                                        ),
                                    ));
                                }
                            }
                            ModuleDependency::Remote(_) => {
                                if matches!(dependency_merged, ModuleDependency::Remote(_)) {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentSourceConflict(
                                            module_name.to_owned(),
                                        ),
                                    ));
                                } else {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentNameConflict(
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
                                            // keep:
                                            // the target (merged) item is newer than or equals to the source one.
                                        }
                                        VersionCompareResult::GreaterThan => {
                                            // replace:
                                            // the target (merged) item is older than the source one
                                            entries_merged[pos_merged] = entry_source.clone()
                                        }
                                        VersionCompareResult::Different => {
                                            return Err(LinkerError::new(
                                                LinkErrorType::DependentVersionConflict(
                                                    module_name.to_owned(),
                                                ),
                                            ));
                                        }
                                    }
                                } else {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentNameConflict(
                                            module_name.to_owned(),
                                        ),
                                    ));
                                }
                            }
                            ModuleDependency::Runtime => {
                                return Err(LinkerError::new(LinkErrorType::DependentNameConflict(
                                    module_name.to_owned(),
                                )))
                            }
                            ModuleDependency::Current => {
                                return Err(LinkerError::new(LinkErrorType::DependentNameConflict(
                                    module_name.to_owned(),
                                )))
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

pub fn merge_import_function_entries(
    function_name_entries: &[FunctionNameEntry],
    internal_function_remap_indices_list: &[RemapIndices],
    import_module_remap_indices_list: &[RemapIndices],
    type_remap_indices_list: &[RemapIndices],
    import_function_entries_list: &[&[ImportFunctionEntry]],
) -> (
    /* import_data_entries */ Vec<ImportFunctionEntry>,
    /* data_public_remap_indices_list */ Vec<RemapIndices>,
) {
    // note:
    // - when adding new `ImportFunctionEntry`, the propertries "import_module_index"
    //   and "type_index" need to be updated.
    // - when merging functions, only the "fullname" will be used to determine if
    //   the functions are the same or not, and the module in which the functions
    //   reside will be ignored.

    let mut entries_merged: Vec<ImportFunctionEntry> = vec![];
    let mut import_function_remap_table_list: Vec<ImportRemapTable> = vec![];

    // merge import function list
    for (submodule_index, entries_source) in import_function_entries_list.iter().enumerate() {
        let mut import_remap_table: ImportRemapTable = vec![];

        // check each entry
        for entry_source in entries_source.iter() {
            let merged_import_module_index =
                import_module_remap_indices_list[submodule_index][entry_source.import_module_index];
            let merged_type_index =
                type_remap_indices_list[submodule_index][entry_source.type_index];

            // check the internal function list first
            let pos_internal_opt = function_name_entries
                .iter()
                .position(|item| item.full_name == entry_source.full_name);

            if let Some(pos_internal) = pos_internal_opt {
                // the target is a internal function, instead of imported function
                // todo: validate the declare type and the actual type
                import_remap_table.push(ImportRemapItem::Internal(pos_internal));
            } else {
                // the target is an imported function

                // check the merged list first
                let pos_merged_opt = entries_merged
                    .iter()
                    .position(|item| item.full_name == entry_source.full_name);

                match pos_merged_opt {
                    Some(pos_merged) => {
                        // found exists
                        // todo: check declare type
                        import_remap_table.push(ImportRemapItem::Import(pos_merged));
                    }
                    None => {
                        // add entry
                        let pos_new = entries_merged.len();
                        let entry_merged = ImportFunctionEntry::new(
                            entry_source.full_name.clone(),
                            merged_import_module_index,
                            merged_type_index,
                        );
                        entries_merged.push(entry_merged);
                        import_remap_table.push(ImportRemapItem::Import(pos_new));
                    }
                }
            }
        }

        import_function_remap_table_list.push(import_remap_table);
    }

    // build the function public index remap list
    let mut function_public_remap_indices_list: Vec<RemapIndices> = vec![];
    let import_function_count = entries_merged.len();
    for (remap_items, internal_function_indices) in import_function_remap_table_list
        .iter()
        .zip(internal_function_remap_indices_list.iter())
    {
        let mut indices = vec![];

        // add the "import" part of the current module
        for remap_item in remap_items {
            match remap_item {
                ImportRemapItem::Import(idx) => {
                    indices.push(*idx);
                }
                ImportRemapItem::Internal(idx) => {
                    indices.push(idx + import_function_count);
                }
            }
        }

        // add the "internal" part of the current module
        for function_internal_index in internal_function_indices {
            indices.push(function_internal_index + import_function_count);
        }

        function_public_remap_indices_list.push(indices);
    }

    (entries_merged, function_public_remap_indices_list)
}

/// the data public index is mixed the following items:
/// - imported read-only data items
/// - imported read-write data items
/// - imported uninitilized data items
/// - internal read-only data items
/// - internal read-write data items
/// - internal uninitilized data items
pub fn merge_data_entries(
    data_name_entries_list: &[&[DataNameEntry]],
    read_only_data_entries_list: &[&[InitedDataEntry]],
    read_write_data_entries_list: &[&[InitedDataEntry]],
    uninit_data_entries_list: &[&[UninitDataEntry]],
) -> (
    /* data_name_entries */ Vec<DataNameEntry>,
    /* read_only_data_entries */ Vec<InitedDataEntry>,
    /* read_write_data_entries */ Vec<InitedDataEntry>,
    /* uninit_data_entries */ Vec<UninitDataEntry>,
    /* internal_data_remap_indices_list */ Vec<RemapIndices>,
) {
    let mut data_name_entries: Vec<DataNameEntry> = vec![];
    let mut read_only_data_entries: Vec<InitedDataEntry> = vec![];
    let mut read_write_data_entries: Vec<InitedDataEntry> = vec![];
    let mut uninit_data_entries: Vec<UninitDataEntry> = vec![];

    let mut internal_data_remap_indices_list: Vec<RemapIndices> =
        vec![vec![]; data_name_entries_list.len()];

    let module_count = data_name_entries_list.len();

    // add read-only data
    for submodule_index in 0..module_count {
        let total_data_internal_index_start = data_name_entries.len();
        let module_data_internal_index_start =
            internal_data_remap_indices_list[submodule_index].len();
        let data_entry_count = read_only_data_entries_list[submodule_index].len();

        data_name_entries.extend(
            data_name_entries_list[submodule_index][module_data_internal_index_start
                ..module_data_internal_index_start + data_entry_count]
                .to_vec(),
        );
        internal_data_remap_indices_list[submodule_index].extend(
            total_data_internal_index_start..total_data_internal_index_start + data_entry_count,
        );
        read_only_data_entries.extend(read_only_data_entries_list[submodule_index].to_vec());
    }

    // add read-write data
    for submodule_index in 0..module_count {
        let total_data_internal_index_start = data_name_entries.len();
        let module_data_internal_index_start =
            internal_data_remap_indices_list[submodule_index].len();
        let data_entry_count = read_write_data_entries_list[submodule_index].len();

        data_name_entries.extend(
            data_name_entries_list[submodule_index][module_data_internal_index_start
                ..module_data_internal_index_start + data_entry_count]
                .to_vec(),
        );
        internal_data_remap_indices_list[submodule_index].extend(
            total_data_internal_index_start..total_data_internal_index_start + data_entry_count,
        );
        read_write_data_entries.extend(read_write_data_entries_list[submodule_index].to_vec());
    }

    // add uninit data
    for submodule_index in 0..module_count {
        let total_data_internal_index_start = data_name_entries.len();
        let module_data_internal_index_start =
            internal_data_remap_indices_list[submodule_index].len();
        let data_entry_count = uninit_data_entries_list[submodule_index].len();

        data_name_entries.extend(
            data_name_entries_list[submodule_index][module_data_internal_index_start
                ..module_data_internal_index_start + data_entry_count]
                .to_vec(),
        );
        internal_data_remap_indices_list[submodule_index].extend(
            total_data_internal_index_start..total_data_internal_index_start + data_entry_count,
        );
        uninit_data_entries.extend(uninit_data_entries_list[submodule_index].to_vec());
    }

    (
        data_name_entries,
        read_only_data_entries,
        read_write_data_entries,
        uninit_data_entries,
        internal_data_remap_indices_list,
    )
}

/// the data public index is mixed the following items:
/// - imported read-only data items
/// - imported read-write data items
/// - imported uninitilized data items
/// - internal read-only data items
/// - internal read-write data items
/// - internal uninitilized data items
pub fn merge_import_data_entries(
    data_name_entries: &[DataNameEntry],
    internal_data_remap_indices_list: &[RemapIndices],
    import_module_remap_indices_list: &[RemapIndices],
    import_data_entries_list: &[&[ImportDataEntry]],
) -> (
    /* import_data_entries */ Vec<ImportDataEntry>,
    /* data_public_remap_indices_list */ Vec<RemapIndices>,
) {
    // note:
    // - when adding new `ImportDataEntry`, the propertries "import_module_index"
    //   needs to be updated.
    // - when merging data, only the "fullname" will be used to determine if
    //   the data are the same or not, and the module in which the data
    //   reside will be ignored.

    let mut entries_merged: Vec<ImportDataEntry> = vec![];
    let mut import_data_remap_table_list: Vec<ImportRemapTable> =
        vec![vec![]; import_data_entries_list.len()];

    // merge import data list by section data
    for data_section_type in [
        DataSectionType::ReadOnly,
        DataSectionType::ReadWrite,
        DataSectionType::Uninit,
    ] {
        // merge import data list
        for (submodule_index, entries_source) in import_data_entries_list.iter().enumerate() {
            let mut import_remap_table: ImportRemapTable = vec![];

            // check each entry
            for entry_source in entries_source
                .iter()
                .filter(|item| item.data_section_type == data_section_type)
            {
                // check the internal function list first
                let pos_internal_opt = data_name_entries
                    .iter()
                    .position(|item| item.full_name == entry_source.full_name);

                if let Some(pos_internal) = pos_internal_opt {
                    // the target is a internal function, instead of imported function
                    // todo: validate the declare type and section type
                    import_remap_table.push(ImportRemapItem::Internal(pos_internal));
                } else {
                    // the target is an imported data

                    // check the merged list first
                    let pos_merged_opt = entries_merged
                        .iter()
                        .position(|item| item.full_name == entry_source.full_name);

                    match pos_merged_opt {
                        Some(pos_merged) => {
                            // found exists
                            import_remap_table.push(ImportRemapItem::Import(pos_merged));
                        }
                        None => {
                            // add entry
                            let merged_import_module_index = import_module_remap_indices_list
                                [submodule_index][entry_source.import_module_index];

                            let pos_new = entries_merged.len();
                            let entry_merged = ImportDataEntry::new(
                                entry_source.full_name.clone(),
                                merged_import_module_index,
                                entry_source.data_section_type,
                                entry_source.memory_data_type,
                            );

                            entries_merged.push(entry_merged);
                            import_remap_table.push(ImportRemapItem::Import(pos_new));
                        }
                    }
                }
            }

            import_data_remap_table_list[submodule_index].append(&mut import_remap_table);
        }
    }

    // build the data public index remap list

    let mut data_public_remap_indices_list: Vec<RemapIndices> = vec![];
    let import_data_count = entries_merged.len();
    for (remap_items, internal_data_indices) in import_data_remap_table_list
        .iter()
        .zip(internal_data_remap_indices_list.iter())
    {
        let mut indices = vec![];

        // add the "import" part of the current module
        for remap_item in remap_items {
            match remap_item {
                ImportRemapItem::Import(idx) => {
                    indices.push(*idx);
                }
                ImportRemapItem::Internal(idx) => {
                    indices.push(idx + import_data_count);
                }
            }
        }

        // add the "internal" part of the current module
        for data_internal_index in internal_data_indices {
            indices.push(data_internal_index + import_data_count);
        }

        data_public_remap_indices_list.push(indices);
    }

    (entries_merged, data_public_remap_indices_list)
}

pub fn merge_external_library_entries(
    external_library_entries_list: &[&[ExternalLibraryEntry]],
) -> Result<
    (
        /* external_library_entries */ Vec<ExternalLibraryEntry>,
        /* external_library_remap_indices_list */ Vec<RemapIndices>,
    ),
    LinkerError,
> {
    // copy the first list
    let mut entries_merged = external_library_entries_list[0].to_vec();
    let mut external_library_remap_indices_list = vec![(0..entries_merged.len()).collect()];

    // merge remains
    for entries_source in &external_library_entries_list[1..] {
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
                            ExternalLibraryDependency::Local(_) => {
                                if matches!(dependency_merged, ExternalLibraryDependency::Local(_))
                                {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentSourceConflict(
                                            module_name.to_owned(),
                                        ),
                                    ));
                                } else {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentNameConflict(
                                            module_name.to_owned(),
                                        ),
                                    ));
                                }
                            }
                            ExternalLibraryDependency::Remote(_) => {
                                if matches!(dependency_merged, ExternalLibraryDependency::Remote(_))
                                {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentSourceConflict(
                                            module_name.to_owned(),
                                        ),
                                    ));
                                } else {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentNameConflict(
                                            module_name.to_owned(),
                                        ),
                                    ));
                                }
                            }
                            ExternalLibraryDependency::Share(share_source) => {
                                if let ExternalLibraryDependency::Share(share_merged) =
                                    dependency_merged
                                {
                                    // compare version
                                    match compare_version(
                                        &share_source.version,
                                        &share_merged.version,
                                    ) {
                                        VersionCompareResult::Equals
                                        | VersionCompareResult::LessThan => {
                                            // keep:
                                            // the target (merged) item is newer than or equals to the source one.
                                        }
                                        VersionCompareResult::GreaterThan => {
                                            // replace:
                                            // the target (merged) item is older than the source one
                                            entries_merged[pos_merged] = entry_source.clone()
                                        }
                                        VersionCompareResult::Different => {
                                            return Err(LinkerError::new(
                                                LinkErrorType::DependentVersionConflict(
                                                    module_name.to_owned(),
                                                ),
                                            ));
                                        }
                                    }
                                } else {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentNameConflict(
                                            module_name.to_owned(),
                                        ),
                                    ));
                                }
                            }
                            ExternalLibraryDependency::Runtime => {
                                return Err(LinkerError::new(LinkErrorType::DependentNameConflict(
                                    module_name.to_owned(),
                                )))
                            }
                            ExternalLibraryDependency::System(_) => {
                                return Err(LinkerError::new(LinkErrorType::DependentNameConflict(
                                    module_name.to_owned(),
                                )))
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

        external_library_remap_indices_list.push(indices);
    }

    Ok((entries_merged, external_library_remap_indices_list))
}

pub fn merge_external_function_entries(
    external_library_remap_indices_list: &[RemapIndices],
    type_remap_indices_list: &[RemapIndices],
    external_function_entries_list: &[&[ExternalFunctionEntry]],
) -> (
    /* external_function_entries */ Vec<ExternalFunctionEntry>,
    /* external_function_remap_indices_list */ Vec<RemapIndices>,
) {
    // note:
    // - when adding new `ExternalFunctionEntry`, the propertries "external_library_index"
    //   and "type_index" need to be updated.
    // - when merging external functions, the "name" and the "library" are used to
    //   determine if the functions are the same or not.

    let mut entries_merged: Vec<ExternalFunctionEntry> = vec![];
    let mut external_function_remap_indices_list: Vec<RemapIndices> = vec![];

    // merge external function list
    for (submodule_index, entries_source) in external_function_entries_list.iter().enumerate() {
        let mut indices: Vec<usize> = vec![];

        // check each entry
        for entry_source in entries_source.iter() {
            let merged_external_library_index = external_library_remap_indices_list
                [submodule_index][entry_source.external_library_index];
            let merged_type_index =
                type_remap_indices_list[submodule_index][entry_source.type_index];

            let pos_merged_opt = entries_merged.iter().position(|item| {
                item.name == entry_source.name
                    && item.external_library_index == merged_external_library_index
            });

            match pos_merged_opt {
                Some(pos_merged) => {
                    // found exists
                    // todo: check declare type
                    indices.push(pos_merged);
                }
                None => {
                    // add entry
                    let pos_new = entries_merged.len();
                    let entry_merged = ExternalFunctionEntry::new(
                        entry_source.name.clone(),
                        merged_external_library_index,
                        merged_type_index,
                    );
                    entries_merged.push(entry_merged);
                    indices.push(pos_new);
                }
            }
        }

        external_function_remap_indices_list.push(indices);
    }

    (entries_merged, external_function_remap_indices_list)
}

pub fn merge_function_entries(
    relocate_list_entries_list: &[&[RelocateListEntry]],
    function_entries_list: &[&[FunctionEntry]],
    remap_table_list: &[RemapTable],
) -> (Vec<FunctionEntry>, Vec<RelocateListEntry>) {
    let mut merged_function_entries = vec![];

    for ((function_entries, relocate_list_entries), remap_table) in function_entries_list
        .iter()
        .zip(relocate_list_entries_list.iter())
        .zip(remap_table_list.iter())
    {
        for (function_entry, relocate_list_entry) in
            function_entries.iter().zip(relocate_list_entries.iter())
        {
            let type_index = remap_table.type_remap_indices[function_entry.type_index];
            let local_variable_list_index = remap_table.local_variable_list_remap_indices
                [function_entry.local_variable_list_index];

            let mut code = function_entry.code.clone();

            // update each relocate item
            for relocate_entry in &relocate_list_entry.relocate_entries {
                let code_piece =
                    &mut code[relocate_entry.code_offset..relocate_entry.code_offset + 4];

                let value_ptr = code_piece.as_mut_ptr() as *mut u32;
                let value_source = unsafe { *value_ptr } as usize;

                // let value_source_data: [u8; 4] = code_piece
                //     .try_into()
                //     .unwrap();
                // let value_source = u32::from_le_bytes(value_source_data) as usize;

                let value_relocated = match relocate_entry.relocate_type {
                    RelocateType::TypeIndex => remap_table.type_remap_indices[value_source],
                    RelocateType::LocalVariableListIndex => {
                        remap_table.local_variable_list_remap_indices[value_source]
                    }
                    RelocateType::FunctionPublicIndex => {
                        remap_table.function_public_remap_indices[value_source]
                    }
                    RelocateType::ExternalFunctionIndex => {
                        remap_table.external_function_remap_indices[value_source]
                    }
                    RelocateType::DataPublicIndex => {
                        remap_table.data_public_remap_indices[value_source]
                    }
                };

                // update
                unsafe { *value_ptr = value_relocated as u32 };

                // let value_relocated_data = (value_relocated as u32).to_le_bytes();
                // code_piece = &mut value_relocated_data;
            }

            let function_entry = FunctionEntry::new(type_index, local_variable_list_index, code);
            merged_function_entries.push(function_entry);
        }
    }

    let merged_relocate_list_entries = relocate_list_entries_list
        .iter()
        .flat_map(|item| item.to_vec())
        .collect::<Vec<_>>();

    (merged_function_entries, merged_relocate_list_entries)
}

enum VersionCompareResult {
    Equals,
    GreaterThan,
    LessThan,
    Different,
}

// version conflicts
// -----------------
//
// If a shared module appears multiple times in the dependency tree with
// different versions and the major version numbers differ, the compiler
// will complain. However, if the major version numbers are the same, the
// highest minor version wil be selected.
//
// Note that this implies that in the actual application runtime, the minor
// version of a module might be higher than what the application explicitly
// declares. This is permissible because minor version updates are expected to
// maintain backward compatibility.
//
// For instance, if an application depends on a module with version 1.4.0, the
// actual runtime version of that module could be anywhere from 1.4.0 to 1.99.99.
//
// For the local and remote file-base shared modules and libraries,
// because they lack version information, if their sources
// (e.g., file paths or URLs) do not match, the compilation will fail.
//
// zero major version
// ------------------
// When a shared module is in beta stage, the major version number can
// be set to zero.
// A zero major version indicates that each minor version is incompatible. If an
// application's dependency tree contains minor version inconsistencies in modules
// with a zero major version, compilation will fail.

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
        // major differ
        VersionCompareResult::Different
    } else {
        if left_parts[0] == 0 {
            // zero major
            if left_parts[1] != right_parts[1] {
                // minor differ
                VersionCompareResult::Different
            } else if left_parts[2] > right_parts[2] {
                VersionCompareResult::GreaterThan
            } else if left_parts[2] < right_parts[2] {
                VersionCompareResult::LessThan
            } else {
                VersionCompareResult::Equals
            }
        } else {
            // normal major
            if left_parts[1] > right_parts[1] {
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
    }
}

/// the map table of importing items to the merged items.
///
/// e.g.
///
/// | import item   | index of import items in the merged module or |
/// |               | internal index of items in the merged module  |
/// |---------------|-----------------------------------------------|
/// | hello::foo    | merged_import_items[0]                        |
/// | hello::bar    | merged_import_items[2]                        |
/// | hello::baz    | merged_items[5]                               |
/// | world::abc    | merged_import_items[3]                        |
/// | world::def    | merged_import_items[1]                        |
/// | world::xyz    | merged_items[2]                               |
type ImportRemapTable = Vec<ImportRemapItem>;

#[derive(Debug, PartialEq, Clone)]
enum ImportRemapItem {
    Import(/* the index of merged imported items */ usize),
    Internal(/* the index of merged internal items */ usize),
}
