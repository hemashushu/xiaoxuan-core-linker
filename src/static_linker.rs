// Copyright (c) 2024 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions,
// more details in file LICENSE, LICENSE.additional and CONTRIBUTING.

use anc_image::{
    entry::{
        ExportDataEntry, ExportFunctionEntry, ExternalFunctionEntry, ExternalLibraryEntry,
        FunctionEntry, ImageCommonEntry, ImportDataEntry, ImportFunctionEntry, ImportModuleEntry,
        InitedDataEntry, LocalVariableListEntry, RelocateListEntry, TypeEntry, UninitDataEntry,
    },
    module_image::{ImageType, RelocateType},
};
use anc_isa::{
    DataSectionType, EffectiveVersion, ExternalLibraryDependency, ModuleDependency,
    VersionCompatibility,
};

use crate::{LinkErrorType, LinkerError};

/// Map the index in a module to the new index in the merged module
///
/// e.g.
///
/// | pub index in an original module | index in the merged module |
/// |---------------------------------|----------------------------|
/// | 0                               | 0                          |
/// | 1                               | 2                          |
/// | 2                               | 6                          |
/// | 3                               | 1                          |
/// | N                               | X                          |
pub type RemapIndices = Vec<usize>;

pub struct RemapTable<'a> {
    pub type_remap_indices: &'a RemapIndices,
    pub data_public_remap_indices: &'a RemapIndices,
    pub function_public_remap_indices: &'a RemapIndices,
    pub local_variable_list_remap_indices: &'a RemapIndices,
    pub external_function_remap_indices: &'a RemapIndices,
}

/// Merges submodules or modules.
///
/// When statically linking different modules (non-submodules), if they
/// both reference the same "Local" module but use different paths (relative paths),
/// the link will fail. Also, if a "Remote" module is referenced but from
/// different source, the link will also fail.
/// So when statically linking different modules, it is recommended to use only
/// "Share" and "Runtime" type dependencies. "Local" and "Remote" dependencies
/// should only be considered for internal development and testing purposes.
pub fn static_link(
    target_module_name: &str,
    target_module_version: &EffectiveVersion,

    // Used to check that all internal function and data references
    // (i.e. `import fn/data module::...`) are resolved.
    // When the link target is a shared module (instead of an object file),
    // all internal functon and data references need to be resolved.
    finalize_internal_functions_reference: bool,
    submodule_entries: &[ImageCommonEntry],
) -> Result<ImageCommonEntry, LinkerError> {
    // merge type entries
    let type_entries_list = submodule_entries
        .iter()
        .map(|item| item.type_entries.as_slice())
        .collect::<Vec<_>>();
    let (type_entries, type_remap_indices_list) = merge_type_entries(&type_entries_list);

    // merge local variable list entries
    let local_variable_list_entries_list = submodule_entries
        .iter()
        .map(|item| item.local_variable_list_entries.as_slice())
        .collect::<Vec<_>>();
    let (local_variable_list_entries, local_variable_list_remap_indices_list) =
        merge_local_variable_list_entries(&local_variable_list_entries_list);

    // merge import module entries
    let import_module_entries_list = submodule_entries
        .iter()
        .map(|item| item.import_module_entries.as_slice())
        .collect::<Vec<_>>();
    let (import_module_entries, import_module_remap_indices_list) =
        merge_import_module_entries(&import_module_entries_list)?;

    // merge export data entries and data entries
    let export_data_entries_list = submodule_entries
        .iter()
        .map(|item| item.export_data_entries.as_slice())
        .collect::<Vec<_>>();

    let read_only_data_entries_list = submodule_entries
        .iter()
        .map(|item| item.read_only_data_entries.as_slice())
        .collect::<Vec<_>>();

    let read_write_data_entries_list = submodule_entries
        .iter()
        .map(|item| item.read_write_data_entries.as_slice())
        .collect::<Vec<_>>();

    let uninit_data_entries_list = submodule_entries
        .iter()
        .map(|item| item.uninit_data_entries.as_slice())
        .collect::<Vec<_>>();

    // the data public index is mixed the following items:
    // - imported read-only data items
    // - imported read-write data items
    // - imported uninitilized data items
    // - internal read-only data items
    // - internal read-write data items
    // - internal uninitilized data items
    let (
        export_data_entries,
        read_only_data_entries,
        read_write_data_entries,
        uninit_data_entries,
        internal_data_remap_indices_list,
    ) = merge_data_entries(
        &export_data_entries_list,
        &read_only_data_entries_list,
        &read_write_data_entries_list,
        &uninit_data_entries_list,
    );

    // merge import data
    let import_data_entries_list = submodule_entries
        .iter()
        .map(|item| item.import_data_entries.as_slice())
        .collect::<Vec<_>>();

    // the data public index is mixed the following items:
    // - imported read-only data items
    // - imported read-write data items
    // - imported uninitilized data items
    // - internal read-only data items
    // - internal read-write data items
    // - internal uninitilized data items
    let (import_data_entries, data_public_remap_indices_list) = merge_import_data_entries(
        &export_data_entries,
        &internal_data_remap_indices_list,
        &import_module_remap_indices_list,
        &import_data_entries_list,
    )?;

    // merge external libraries
    let external_library_entries_list = submodule_entries
        .iter()
        .map(|item| item.external_library_entries.as_slice())
        .collect::<Vec<_>>();
    let (external_library_entries, external_library_remap_indices_list) =
        merge_external_library_entries(&external_library_entries_list)?;

    // merge external functions
    let external_function_entries_list = submodule_entries
        .iter()
        .map(|item| item.external_function_entries.as_slice())
        .collect::<Vec<_>>();
    let (external_function_entries, external_function_remap_indices_list) =
        merge_external_function_entries(
            &external_library_remap_indices_list,
            &type_remap_indices_list,
            &external_function_entries_list,
        );

    // merge function name entries
    let mut export_function_entries: Vec<ExportFunctionEntry> = vec![];
    let mut internal_function_remap_indices_list: Vec<RemapIndices> = vec![];

    for submodule_entry in submodule_entries {
        let indices = (export_function_entries.len()
            ..export_function_entries.len() + submodule_entry.export_function_entries.len())
            .collect::<Vec<_>>();
        internal_function_remap_indices_list.push(indices);
        export_function_entries.extend(submodule_entry.export_function_entries.to_vec());
    }

    // merge import function entries
    let import_function_entries_list = submodule_entries
        .iter()
        .map(|item| item.import_function_entries.as_slice())
        .collect::<Vec<_>>();
    let (import_function_entries, function_public_remap_indices_list) =
        merge_import_function_entries(
            &export_function_entries,
            &internal_function_remap_indices_list,
            &import_module_remap_indices_list,
            &type_remap_indices_list,
            &import_function_entries_list,
        )?;

    // merge relocate list entries
    let relocate_list_entries_list = submodule_entries
        .iter()
        .map(|item| item.relocate_list_entries.as_slice())
        .collect::<Vec<_>>();

    let function_entries_list = submodule_entries
        .iter()
        .map(|item| item.function_entries.as_slice())
        .collect::<Vec<_>>();

    let mut remap_table_list = vec![];

    for submodule_index in 0..submodule_entries.len() {
        let remap_table = RemapTable {
            type_remap_indices: &type_remap_indices_list[submodule_index],
            local_variable_list_remap_indices: &local_variable_list_remap_indices_list
                [submodule_index],
            function_public_remap_indices: &function_public_remap_indices_list[submodule_index],
            data_public_remap_indices: &data_public_remap_indices_list[submodule_index],
            external_function_remap_indices: &external_function_remap_indices_list[submodule_index],
        };
        remap_table_list.push(remap_table);
    }

    let (function_entries, relocate_list_entries) = merge_function_entries(
        &relocate_list_entries_list,
        &function_entries_list,
        &remap_table_list,
    );

    // Check that the internally referenced functions and data have all been resolved.
    if finalize_internal_functions_reference {
        let the_current_module = ImportModuleEntry::self_reference_entry();
        let pos_opt = import_module_entries
            .iter()
            .position(|item| item == &the_current_module);

        if let Some(pos) = pos_opt {
            for import_function_entry in &import_function_entries {
                if import_function_entry.import_module_index == pos {
                    return Err(LinkerError::new(LinkErrorType::FunctionNotFound(
                        import_function_entry.full_name.to_owned(),
                    )));
                }
            }

            for import_data_entry in &import_data_entries {
                if import_data_entry.import_module_index == pos {
                    return Err(LinkerError::new(LinkErrorType::DataNotFound(
                        import_data_entry.full_name.to_owned(),
                    )));
                }
            }
        }
    }

    let image_type = if finalize_internal_functions_reference {
        ImageType::SharedModule
    } else {
        ImageType::ObjectFile
    };

    let merged_image_common_entry = ImageCommonEntry {
        name: target_module_name.to_owned(),
        version: *target_module_version,
        image_type,
        import_module_entries,
        import_function_entries,
        import_data_entries,
        type_entries,
        local_variable_list_entries,
        function_entries,
        read_only_data_entries,
        read_write_data_entries,
        uninit_data_entries,
        export_function_entries,
        export_data_entries,
        relocate_list_entries,
        external_library_entries,
        external_function_entries,
    };

    Ok(merged_image_common_entry)
}

fn merge_type_entries(
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

fn merge_local_variable_list_entries(
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

fn merge_import_module_entries(
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

                    let dependency_source = entry_source.module_dependency.as_ref();
                    let dependency_merged = entry_merged.module_dependency.as_ref();

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
                                    match EffectiveVersion::from_str(&share_source.version)
                                        .compatible(&EffectiveVersion::from_str(
                                            &share_merged.version,
                                        )) {
                                        VersionCompatibility::Equals
                                        | VersionCompatibility::LessThan => {
                                            // keep:
                                            // the target (merged) item is newer than or equals to the source one.
                                        }
                                        VersionCompatibility::GreaterThan => {
                                            // replace:
                                            // the target (merged) item is older than the source one
                                            entries_merged[pos_merged] = entry_source.clone()
                                        }
                                        VersionCompatibility::Conflict => {
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
                            ModuleDependency::Module => {
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

fn merge_import_function_entries(
    export_function_entries: &[ExportFunctionEntry],
    internal_function_remap_indices_list: &[RemapIndices],
    import_module_remap_indices_list: &[RemapIndices],
    type_remap_indices_list: &[RemapIndices],
    import_function_entries_list: &[&[ImportFunctionEntry]],
) -> Result<
    (
        /* import_data_entries */ Vec<ImportFunctionEntry>,
        /* data_public_remap_indices_list */ Vec<RemapIndices>,
    ),
    LinkerError,
> {
    // note:
    // - when adding new `ImportFunctionEntry`, the propertries "import_module_index"
    //   and "type_index" need to be updated.
    // - when merging functions, only the "fullname" will be used to determine if
    //   the functions are the same or not, and the module in which the functions
    //   reside will be ignored.

    let mut import_function_entries_merged: Vec<ImportFunctionEntry> = vec![];
    let mut import_function_remap_table_list: Vec<ImportRemapTable> = vec![];

    // merge import function list
    for (submodule_index, import_function_entries_source) in
        import_function_entries_list.iter().enumerate()
    {
        let mut import_remap_table: ImportRemapTable = vec![];

        // check each entry
        for import_function_entry_source in import_function_entries_source.iter() {
            let merged_import_module_index = import_module_remap_indices_list[submodule_index]
                [import_function_entry_source.import_module_index];
            let merged_type_index =
                type_remap_indices_list[submodule_index][import_function_entry_source.type_index];

            // check the internal function list first
            let pos_internal_opt = export_function_entries
                .iter()
                .position(|item| item.full_name == import_function_entry_source.full_name);

            if let Some(pos_internal) = pos_internal_opt {
                // the target is a internal function, instead of imported function
                // let export_function_entry = &export_function_entries[pos_internal];

                // In the case of merged modules, “visibility” does not need to be checked,
                // because all functions and data within the same module
                // (even if the source is from a different module) are visible.

                // todo: check the type

                import_remap_table.push(ImportRemapItem::Internal(pos_internal));
            } else {
                // the target is an imported function

                // check the merged list first
                let pos_merged_opt = import_function_entries_merged
                    .iter()
                    .position(|item| item.full_name == import_function_entry_source.full_name);

                match pos_merged_opt {
                    Some(pos_merged) => {
                        // found exists

                        // check consistance
                        // let import_function_entry_merged = &import_function_entries_merged[pos_merged];

                        // todo:: check the type

                        import_remap_table.push(ImportRemapItem::Import(pos_merged));
                    }
                    None => {
                        // add entry
                        let pos_new = import_function_entries_merged.len();
                        let entry_merged = ImportFunctionEntry::new(
                            import_function_entry_source.full_name.clone(),
                            merged_import_module_index,
                            merged_type_index,
                        );
                        import_function_entries_merged.push(entry_merged);
                        import_remap_table.push(ImportRemapItem::Import(pos_new));
                    }
                }
            }
        }

        import_function_remap_table_list.push(import_remap_table);
    }

    // build the function public index remap list
    let mut function_public_remap_indices_list: Vec<RemapIndices> = vec![];
    let import_function_count = import_function_entries_merged.len();
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

    Ok((
        import_function_entries_merged,
        function_public_remap_indices_list,
    ))
}

/// the data public index is mixed the following items:
/// - imported read-only data items
/// - imported read-write data items
/// - imported uninitilized data items
/// - internal read-only data items
/// - internal read-write data items
/// - internal uninitilized data items
#[allow(clippy::type_complexity)]
fn merge_data_entries(
    export_data_entries_list: &[&[ExportDataEntry]],
    read_only_data_entries_list: &[&[InitedDataEntry]],
    read_write_data_entries_list: &[&[InitedDataEntry]],
    uninit_data_entries_list: &[&[UninitDataEntry]],
) -> (
    /* export_data_entries */ Vec<ExportDataEntry>,
    /* read_only_data_entries */ Vec<InitedDataEntry>,
    /* read_write_data_entries */ Vec<InitedDataEntry>,
    /* uninit_data_entries */ Vec<UninitDataEntry>,
    /* internal_data_remap_indices_list */ Vec<RemapIndices>,
) {
    let mut export_data_entries: Vec<ExportDataEntry> = vec![];
    let mut read_only_data_entries: Vec<InitedDataEntry> = vec![];
    let mut read_write_data_entries: Vec<InitedDataEntry> = vec![];
    let mut uninit_data_entries: Vec<UninitDataEntry> = vec![];

    let mut internal_data_remap_indices_list: Vec<RemapIndices> =
        vec![vec![]; export_data_entries_list.len()];

    let module_count = export_data_entries_list.len();

    // add read-only data
    for submodule_index in 0..module_count {
        let total_data_internal_index_start = export_data_entries.len();
        let module_data_internal_index_start =
            internal_data_remap_indices_list[submodule_index].len();
        let data_entry_count = read_only_data_entries_list[submodule_index].len();

        export_data_entries.extend(
            export_data_entries_list[submodule_index][module_data_internal_index_start
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
        let total_data_internal_index_start = export_data_entries.len();
        let module_data_internal_index_start =
            internal_data_remap_indices_list[submodule_index].len();
        let data_entry_count = read_write_data_entries_list[submodule_index].len();

        export_data_entries.extend(
            export_data_entries_list[submodule_index][module_data_internal_index_start
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
        let total_data_internal_index_start = export_data_entries.len();
        let module_data_internal_index_start =
            internal_data_remap_indices_list[submodule_index].len();
        let data_entry_count = uninit_data_entries_list[submodule_index].len();

        export_data_entries.extend(
            export_data_entries_list[submodule_index][module_data_internal_index_start
                ..module_data_internal_index_start + data_entry_count]
                .to_vec(),
        );
        internal_data_remap_indices_list[submodule_index].extend(
            total_data_internal_index_start..total_data_internal_index_start + data_entry_count,
        );
        uninit_data_entries.extend(uninit_data_entries_list[submodule_index].to_vec());
    }

    (
        export_data_entries,
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
fn merge_import_data_entries(
    export_data_entries: &[ExportDataEntry],
    internal_data_remap_indices_list: &[RemapIndices],
    import_module_remap_indices_list: &[RemapIndices],
    import_data_entries_list: &[&[ImportDataEntry]],
) -> Result<
    (
        /* import_data_entries */ Vec<ImportDataEntry>,
        /* data_public_remap_indices_list */ Vec<RemapIndices>,
    ),
    LinkerError,
> {
    // note:
    // - when adding new `ImportDataEntry`, the propertries "import_module_index"
    //   needs to be updated.
    // - when merging data, only the "fullname" will be used to determine if
    //   the data are the same or not, and the module in which the data
    //   reside will be ignored.

    let mut import_data_entries_merged: Vec<ImportDataEntry> = vec![];
    let mut import_data_remap_table_list: Vec<ImportRemapTable> =
        vec![vec![]; import_data_entries_list.len()];

    // merge import data list by section data
    for data_section_type in [
        DataSectionType::ReadOnly,
        DataSectionType::ReadWrite,
        DataSectionType::Uninit,
    ] {
        // merge import data list
        for (submodule_index, import_data_entries_source) in
            import_data_entries_list.iter().enumerate()
        {
            let mut import_remap_table: ImportRemapTable = vec![];

            // check each entry
            for import_data_entry_source in import_data_entries_source
                .iter()
                .filter(|item| item.data_section_type == data_section_type)
            {
                // check the internal data list first
                let pos_internal_opt = export_data_entries
                    .iter()
                    .position(|item| item.full_name == import_data_entry_source.full_name);

                if let Some(pos_internal) = pos_internal_opt {
                    // the target is a internal function, instead of imported function
                    let export_data_entry = &export_data_entries[pos_internal];

                    // In the case of merged modules, “visibility” does not need to be checked,
                    // because all functions and data within the same module
                    // (even if the source is from a different module) are visible.

                    if import_data_entry_source.data_section_type != export_data_entry.section_type
                    {
                        return Err(LinkerError::new(LinkErrorType::ImportDataSectionMismatch(
                            import_data_entry_source.full_name.to_owned(),
                            import_data_entry_source.data_section_type,
                        )));
                    }

                    // todo: check the type

                    import_remap_table.push(ImportRemapItem::Internal(pos_internal));
                } else {
                    // the target is an imported data

                    // check the merged list first
                    let pos_merged_opt = import_data_entries_merged
                        .iter()
                        .position(|item| item.full_name == import_data_entry_source.full_name);

                    match pos_merged_opt {
                        Some(pos_merged) => {
                            // found exists
                            // check consistance
                            let import_data_entry_merged = &import_data_entries_merged[pos_merged];

                            // check data section type
                            if import_data_entry_source.data_section_type
                                != import_data_entry_merged.data_section_type
                            {
                                return Err(LinkerError::new(
                                    LinkErrorType::ImportDataSectionInconsistant(
                                        import_data_entry_source.full_name.to_owned(),
                                    ),
                                ));
                            }

                            // check the type
                            if import_data_entry_source.memory_data_type
                                != import_data_entry_merged.memory_data_type
                            {
                                return Err(LinkerError::new(
                                    LinkErrorType::ImportDataTypeInconsistant(
                                        import_data_entry_source.full_name.to_owned(),
                                    ),
                                ));
                            }

                            import_remap_table.push(ImportRemapItem::Import(pos_merged));
                        }
                        None => {
                            // add entry
                            let merged_import_module_index = import_module_remap_indices_list
                                [submodule_index][import_data_entry_source.import_module_index];

                            let pos_new = import_data_entries_merged.len();
                            let import_data_entry_merged = ImportDataEntry::new(
                                import_data_entry_source.full_name.clone(),
                                merged_import_module_index,
                                import_data_entry_source.data_section_type,
                                import_data_entry_source.memory_data_type,
                            );

                            import_data_entries_merged.push(import_data_entry_merged);
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
    let import_data_count = import_data_entries_merged.len();
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

    Ok((import_data_entries_merged, data_public_remap_indices_list))
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
                    let library_name = &entry_merged.name;

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
                                            library_name.to_owned(),
                                        ),
                                    ));
                                } else {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentNameConflict(
                                            library_name.to_owned(),
                                        ),
                                    ));
                                }
                            }
                            ExternalLibraryDependency::Remote(_) => {
                                if matches!(dependency_merged, ExternalLibraryDependency::Remote(_))
                                {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentSourceConflict(
                                            library_name.to_owned(),
                                        ),
                                    ));
                                } else {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentNameConflict(
                                            library_name.to_owned(),
                                        ),
                                    ));
                                }
                            }
                            ExternalLibraryDependency::Share(share_source) => {
                                if let ExternalLibraryDependency::Share(share_merged) =
                                    dependency_merged
                                {
                                    // compare version
                                    match EffectiveVersion::from_str(&share_source.version)
                                        .compatible(&EffectiveVersion::from_str(
                                            &share_merged.version,
                                        )) {
                                        VersionCompatibility::Equals
                                        | VersionCompatibility::LessThan => {
                                            // keep:
                                            // the target (merged) item is newer than or equals to the source one.
                                        }
                                        VersionCompatibility::GreaterThan => {
                                            // replace:
                                            // the target (merged) item is older than the source one
                                            entries_merged[pos_merged] = entry_source.clone()
                                        }
                                        VersionCompatibility::Conflict => {
                                            return Err(LinkerError::new(
                                                LinkErrorType::DependentVersionConflict(
                                                    library_name.to_owned(),
                                                ),
                                            ));
                                        }
                                    }
                                } else {
                                    return Err(LinkerError::new(
                                        LinkErrorType::DependentNameConflict(
                                            library_name.to_owned(),
                                        ),
                                    ));
                                }
                            }
                            // ExternalLibraryDependency::Runtime => {
                            //     return Err(LinkerError::new(LinkErrorType::DependentNameConflict(
                            //         library_name.to_owned(),
                            //     )))
                            // }
                            ExternalLibraryDependency::System(_) => {
                                return Err(LinkerError::new(LinkErrorType::DependentNameConflict(
                                    library_name.to_owned(),
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

fn merge_external_function_entries(
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
            let external_library_index_merged = external_library_remap_indices_list
                [submodule_index][entry_source.external_library_index];

            // how to determine if two external functions are the same?
            // Is it just checking the function name like in C/ELF programs,
            // includes the library name?
            let pos_merged_opt = entries_merged.iter().position(|item| {
                item.name == entry_source.name
                    && item.external_library_index == external_library_index_merged
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
                    let type_index_merged =
                        type_remap_indices_list[submodule_index][entry_source.type_index];

                    let entry_merged = ExternalFunctionEntry::new(
                        entry_source.name.clone(),
                        external_library_index_merged,
                        type_index_merged,
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

fn merge_function_entries(
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use pretty_assertions::assert_eq;

    use anc_assembler::assembler::assemble_module_node;
    use anc_image::{
        bytecode_reader::format_bytecode_as_text,
        entry::{
            ExportDataEntry, ExportFunctionEntry, ExternalFunctionEntry, ExternalLibraryEntry,
            ImageCommonEntry, ImportModuleEntry, InitedDataEntry, LocalVariableEntry,
            LocalVariableListEntry, RelocateEntry, RelocateListEntry, TypeEntry, UninitDataEntry,
        },
        module_image::{RelocateType, Visibility},
    };
    use anc_isa::{
        DataSectionType, DependencyCondition, DependencyShare, EffectiveVersion,
        ExternalLibraryDependency, ModuleDependency, OperandDataType,
    };
    use anc_parser_asm::parser::parse_from_str;

    use crate::{
        static_linker::{merge_import_module_entries, static_link},
        LinkErrorType, LinkerError,
    };

    fn assemble_submodules(
        submodules: &[(/* fullname */ &str, /* source */ &str)],
        import_module_entries: &[ImportModuleEntry],
        external_library_entries: &[ExternalLibraryEntry],
    ) -> Vec<ImageCommonEntry> {
        let mut common_entries = vec![];

        for (full_name, source_code) in submodules {
            let module_node = match parse_from_str(source_code) {
                Ok(node) => node,
                Err(parser_error) => {
                    panic!("{}", parser_error.with_source(source_code));
                }
            };

            let image_common_entry = assemble_module_node(
                &module_node,
                full_name,
                import_module_entries,
                external_library_entries,
            )
            .unwrap();

            common_entries.push(image_common_entry);
        }

        common_entries
    }

    #[test]
    fn test_merge_type_and_local_variable_list_entries() {
        let submodule0 = (
            "hello",
            r#"
fn main()->i32 [a:i32] {                    // type 1, local 1
    block (
        m:i32=imm_i32(0x11),
        n:i32=imm_i32(0x13)
        )->i32                              // type 2, local 2
        [x:i32] {
        nop()
    }

    when [v:i32]                            // local 1
        eqz_i32(imm_i32(0x17))
        nop()
}
"#,
        );

        let submodule1 = (
            "hello::world",
            r#"
fn add(left:i32, right:i32) -> i32 {        // type 2, local 3
    if -> i32                               // type 1, local 0
        eqz_i32(imm_i32(0x19))
        imm_i32(0x23)
        imm_i32(0x29)

    when [p:i32, q:i32]                     // local 3
        eqz_i32(imm_i32(0x31))
        nop()

    block (
        a:i32=imm_i32(0x37)
        ) -> (i32, i32)                     // type 3, local 2
        [x:i32, y:i32]
    {
        nop()
    }
}
"#,
        );

        let submodules = vec![submodule0, submodule1];
        let submodule_entries = assemble_submodules(&submodules, &[], &[]);
        let linked_module = static_link(
            "merged",
            &EffectiveVersion::new(0, 0, 0),
            true,
            &submodule_entries,
        )
        .unwrap();

        // type
        assert_eq!(
            linked_module.type_entries,
            vec![
                TypeEntry::new(vec![], vec![]),
                TypeEntry::new(vec![], vec![OperandDataType::I32]),
                TypeEntry::new(
                    vec![OperandDataType::I32, OperandDataType::I32],
                    vec![OperandDataType::I32]
                ),
                TypeEntry::new(
                    vec![OperandDataType::I32],
                    vec![OperandDataType::I32, OperandDataType::I32]
                ),
            ]
        );

        // local variable list
        assert_eq!(
            linked_module.local_variable_list_entries,
            vec![
                LocalVariableListEntry::new(vec![]),
                LocalVariableListEntry::new(vec![LocalVariableEntry::from_i32()]),
                LocalVariableListEntry::new(vec![
                    LocalVariableEntry::from_i32(),
                    LocalVariableEntry::from_i32(),
                    LocalVariableEntry::from_i32()
                ]),
                LocalVariableListEntry::new(vec![
                    LocalVariableEntry::from_i32(),
                    LocalVariableEntry::from_i32()
                ]),
            ]
        );

        // functions
        let func0 = &linked_module.function_entries[0];
        assert_eq!(func0.type_index, 1);
        assert_eq!(func0.local_variable_list_index, 1);
        assert_eq!(
            format_bytecode_as_text(&func0.code),
            "\
0x0000  40 01 00 00  11 00 00 00    imm_i32           0x00000011
0x0008  40 01 00 00  13 00 00 00    imm_i32           0x00000013
0x0010  c1 03 00 00  02 00 00 00    block             type:2   local:2
        02 00 00 00
0x001c  00 01                       nop
0x001e  c0 03                       end
0x0020  40 01 00 00  17 00 00 00    imm_i32           0x00000017
0x0028  c0 02                       eqz_i32
0x002a  00 01                       nop
0x002c  c6 03 00 00  01 00 00 00    block_nez         local:1   off:0x10
        10 00 00 00
0x0038  00 01                       nop
0x003a  c0 03                       end
0x003c  c0 03                       end"
        );

        let func1 = &linked_module.function_entries[1];
        assert_eq!(func1.type_index, 2);
        assert_eq!(func1.local_variable_list_index, 3);
        assert_eq!(
            format_bytecode_as_text(&func1.code),
            "\
0x0000  40 01 00 00  19 00 00 00    imm_i32           0x00000019
0x0008  c0 02                       eqz_i32
0x000a  00 01                       nop
0x000c  c4 03 00 00  01 00 00 00    block_alt         type:1   local:0   off:0x20
        00 00 00 00  20 00 00 00
0x001c  40 01 00 00  23 00 00 00    imm_i32           0x00000023
0x0024  c5 03 00 00  12 00 00 00    break_alt         off:0x12
0x002c  40 01 00 00  29 00 00 00    imm_i32           0x00000029
0x0034  c0 03                       end
0x0036  00 01                       nop
0x0038  40 01 00 00  31 00 00 00    imm_i32           0x00000031
0x0040  c0 02                       eqz_i32
0x0042  00 01                       nop
0x0044  c6 03 00 00  03 00 00 00    block_nez         local:3   off:0x10
        10 00 00 00
0x0050  00 01                       nop
0x0052  c0 03                       end
0x0054  40 01 00 00  37 00 00 00    imm_i32           0x00000037
0x005c  c1 03 00 00  03 00 00 00    block             type:3   local:2
        02 00 00 00
0x0068  00 01                       nop
0x006a  c0 03                       end
0x006c  c0 03                       end"
        );

        // relocate list
        assert_eq!(
            linked_module.relocate_list_entries,
            vec![
                RelocateListEntry::new(vec![
                    // block
                    RelocateEntry::new(0x14, RelocateType::TypeIndex),
                    RelocateEntry::new(0x18, RelocateType::LocalVariableListIndex),
                    // block_nez
                    RelocateEntry::new(0x30, RelocateType::LocalVariableListIndex),
                ]),
                RelocateListEntry::new(vec![
                    // block_alt
                    RelocateEntry::new(0x10, RelocateType::TypeIndex),
                    RelocateEntry::new(0x14, RelocateType::LocalVariableListIndex),
                    // block_nez
                    RelocateEntry::new(0x48, RelocateType::LocalVariableListIndex),
                    // block
                    RelocateEntry::new(0x60, RelocateType::TypeIndex),
                    RelocateEntry::new(0x64, RelocateType::LocalVariableListIndex),
                ]),
            ]
        );
    }

    #[test]
    fn test_merge_import_module_entries() {
        let import_module_entries0 = vec![
            ImportModuleEntry::self_reference_entry(),
            ImportModuleEntry::new(
                "network".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    version: "1.0.1".to_owned(),
                    condition: DependencyCondition::True,
                    parameters: HashMap::default(),
                }))),
            ),
            ImportModuleEntry::new(
                "encoding".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    version: "2.1.0".to_owned(),
                    condition: DependencyCondition::True,
                    parameters: HashMap::default(),
                }))),
            ),
        ];

        let import_module_entries1 = vec![
            ImportModuleEntry::self_reference_entry(),
            ImportModuleEntry::new(
                // new item
                "gui".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    version: "1.3.4".to_owned(),
                    condition: DependencyCondition::True,
                    parameters: HashMap::default(),
                }))),
            ),
            ImportModuleEntry::new(
                // updated item
                "encoding".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    version: "2.2.0".to_owned(),
                    condition: DependencyCondition::True,
                    parameters: HashMap::default(),
                }))),
            ),
            ImportModuleEntry::new(
                // identical item
                "network".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    version: "1.0.1".to_owned(),
                    condition: DependencyCondition::True,
                    parameters: HashMap::default(),
                }))),
            ),
        ];

        let import_module_entries_list = vec![
            import_module_entries0.as_slice(),
            import_module_entries1.as_slice(),
        ];
        let (merged_module_entries_list, import_module_remap_indices_list) =
            merge_import_module_entries(&import_module_entries_list).unwrap();

        // check merged entries
        let expected_module_entries_list = vec![
            ImportModuleEntry::self_reference_entry(),
            ImportModuleEntry::new(
                "network".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    version: "1.0.1".to_owned(),
                    condition: DependencyCondition::True,
                    parameters: HashMap::default(),
                }))),
            ),
            // this item should be the version "2.2.0" instead of "2.1.0".
            ImportModuleEntry::new(
                "encoding".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    version: "2.2.0".to_owned(),
                    condition: DependencyCondition::True,
                    parameters: HashMap::default(),
                }))),
            ),
            // this item is new added.
            ImportModuleEntry::new(
                "gui".to_owned(),
                Box::new(ModuleDependency::Share(Box::new(DependencyShare {
                    version: "1.3.4".to_owned(),
                    condition: DependencyCondition::True,
                    parameters: HashMap::default(),
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
    fn test_merge_import_module_entries_with_version_conflict() {
        // todo
    }

    #[test]
    fn test_merge_import_data() {
        let submodule0 = (
            "hello",
            r#"
import readonly data module::middle::d0 type i32
import data module::middle::d2 type i32
import uninit data module::middle::d4 type i32
import readonly data module::base::d1 type i32
import data module::base::d3 type i32
import uninit data module::base::d5 type i32

fn main() {
    data_load_i32_s(d0)
    data_load_i32_s(d1)
    data_load_i32_s(d2)
    data_load_i32_s(d3)
    data_load_i32_s(d4)
    data_load_i32_s(d5)
}"#,
        );

        let submodule1 = (
            "hello::middle",
            r#"
import uninit data module::base::d5 type i32
import readonly data module::base::d1 type i32
import data module::base::d3 type i32

readonly data d0:i32 = 0x11
uninit data d4:i32
data d2:i32 = 0x13

fn foo() {
    data_load_i32_s(d1)
    data_load_i32_s(d3)
    data_load_i32_s(d5)
    data_load_i32_s(d4)
    data_load_i32_s(d2)
    data_load_i32_s(d0)
}"#,
        );

        let submodule2 = (
            "hello::base",
            r#"
data d3:i32 = 0x19
uninit data d5:i32
readonly data d1:i32 = 0x17

fn bar() {
    data_load_i32_s(d1)
    data_load_i32_s(d3)
    data_load_i32_s(d5)
}"#,
        );

        let submodules = vec![submodule0, submodule1, submodule2];
        let submodule_entries = assemble_submodules(&submodules, &[], &[]);
        let linked_module = static_link(
            "merged",
            &EffectiveVersion::new(0, 0, 0),
            true,
            &submodule_entries,
        )
        .unwrap();

        // import modules
        assert_eq!(
            linked_module.import_module_entries,
            vec![ImportModuleEntry::self_reference_entry()]
        );

        // import data
        assert!(linked_module.import_data_entries.is_empty());

        // functions
        assert_eq!(
            format_bytecode_as_text(&linked_module.function_entries[0].code),
            "\
0x0000  c1 01 00 00  00 00 00 00    data_load_i32_s   off:0x00  idx:0
0x0008  c1 01 00 00  01 00 00 00    data_load_i32_s   off:0x00  idx:1
0x0010  c1 01 00 00  02 00 00 00    data_load_i32_s   off:0x00  idx:2
0x0018  c1 01 00 00  03 00 00 00    data_load_i32_s   off:0x00  idx:3
0x0020  c1 01 00 00  04 00 00 00    data_load_i32_s   off:0x00  idx:4
0x0028  c1 01 00 00  05 00 00 00    data_load_i32_s   off:0x00  idx:5
0x0030  c0 03                       end"
        );

        assert_eq!(
            format_bytecode_as_text(&linked_module.function_entries[1].code),
            "\
0x0000  c1 01 00 00  01 00 00 00    data_load_i32_s   off:0x00  idx:1
0x0008  c1 01 00 00  03 00 00 00    data_load_i32_s   off:0x00  idx:3
0x0010  c1 01 00 00  05 00 00 00    data_load_i32_s   off:0x00  idx:5
0x0018  c1 01 00 00  04 00 00 00    data_load_i32_s   off:0x00  idx:4
0x0020  c1 01 00 00  02 00 00 00    data_load_i32_s   off:0x00  idx:2
0x0028  c1 01 00 00  00 00 00 00    data_load_i32_s   off:0x00  idx:0
0x0030  c0 03                       end"
        );

        assert_eq!(
            format_bytecode_as_text(&linked_module.function_entries[2].code),
            "\
0x0000  c1 01 00 00  01 00 00 00    data_load_i32_s   off:0x00  idx:1
0x0008  c1 01 00 00  03 00 00 00    data_load_i32_s   off:0x00  idx:3
0x0010  c1 01 00 00  05 00 00 00    data_load_i32_s   off:0x00  idx:5
0x0018  c0 03                       end"
        );

        // .rodata
        assert_eq!(
            linked_module.read_only_data_entries,
            vec![
                InitedDataEntry::from_i32(0x11),
                InitedDataEntry::from_i32(0x17)
            ]
        );

        // .data
        assert_eq!(
            linked_module.read_write_data_entries,
            vec![
                InitedDataEntry::from_i32(0x13),
                InitedDataEntry::from_i32(0x19)
            ]
        );

        // .bss
        assert_eq!(
            linked_module.uninit_data_entries,
            vec![UninitDataEntry::from_i32(), UninitDataEntry::from_i32(),]
        );

        // data name
        assert_eq!(
            linked_module.export_data_entries,
            vec![
                ExportDataEntry::new(
                    "hello::middle::d0".to_owned(),
                    Visibility::Private,
                    DataSectionType::ReadOnly
                ),
                ExportDataEntry::new(
                    "hello::base::d1".to_owned(),
                    Visibility::Private,
                    DataSectionType::ReadOnly
                ),
                ExportDataEntry::new(
                    "hello::middle::d2".to_owned(),
                    Visibility::Private,
                    DataSectionType::ReadWrite
                ),
                ExportDataEntry::new(
                    "hello::base::d3".to_owned(),
                    Visibility::Private,
                    DataSectionType::ReadWrite
                ),
                ExportDataEntry::new(
                    "hello::middle::d4".to_owned(),
                    Visibility::Private,
                    DataSectionType::Uninit
                ),
                ExportDataEntry::new(
                    "hello::base::d5".to_owned(),
                    Visibility::Private,
                    DataSectionType::Uninit
                ),
            ]
        );

        // relocate
        assert_eq!(
            linked_module.relocate_list_entries,
            vec![
                RelocateListEntry::new(vec![
                    RelocateEntry::new(0x4, RelocateType::DataPublicIndex),
                    RelocateEntry::new(0xc, RelocateType::DataPublicIndex),
                    RelocateEntry::new(0x14, RelocateType::DataPublicIndex),
                    RelocateEntry::new(0x1c, RelocateType::DataPublicIndex),
                    RelocateEntry::new(0x24, RelocateType::DataPublicIndex),
                    RelocateEntry::new(0x2c, RelocateType::DataPublicIndex),
                ]),
                RelocateListEntry::new(vec![
                    RelocateEntry::new(0x4, RelocateType::DataPublicIndex),
                    RelocateEntry::new(0xc, RelocateType::DataPublicIndex),
                    RelocateEntry::new(0x14, RelocateType::DataPublicIndex),
                    RelocateEntry::new(0x1c, RelocateType::DataPublicIndex),
                    RelocateEntry::new(0x24, RelocateType::DataPublicIndex),
                    RelocateEntry::new(0x2c, RelocateType::DataPublicIndex),
                ]),
                RelocateListEntry::new(vec![
                    RelocateEntry::new(0x4, RelocateType::DataPublicIndex),
                    RelocateEntry::new(0xc, RelocateType::DataPublicIndex),
                    RelocateEntry::new(0x14, RelocateType::DataPublicIndex),
                ]),
            ]
        );
    }

    #[test]
    fn test_merge_data() {
        // todo
    }

    #[test]
    fn test_merge_external_libraries() {
        // todo
    }

    #[test]
    fn test_merge_external_libraries_with_name_conflict() {
        // todo
    }

    #[test]
    fn test_merge_external_libraries_with_source_conflict() {
        // todo
    }

    #[test]
    fn test_merge_external_libraries_with_version_conflict() {
        // todo
    }

    #[test]
    fn test_merge_external_function() {
        let submodule0 = (
            "hello",
            r#"
external fn abc::do_something()
external fn def::do_this(i32) -> i32

fn main() {
    extcall(do_something)
    extcall(do_this, imm_i32(0x11))
}
"#,
        );

        let submodule1 = (
            "hello::world",
            r#"
external fn def::do_that(i32,i32) -> i32
external fn abc::do_something()

fn foo(n:i32)->i32 {
    extcall(do_something)
}

fn bar() -> i32 {
    extcall(do_that, imm_i32(0x13), imm_i32(0x17))
}
"#,
        );

        let libabc = ExternalLibraryEntry::new(
            "abc".to_owned(),
            Box::new(ExternalLibraryDependency::System("abc".to_owned())),
        );
        let libdef = ExternalLibraryEntry::new(
            "def".to_owned(),
            Box::new(ExternalLibraryDependency::System("def".to_owned())),
        );

        let submodules = vec![submodule0, submodule1];
        let submodule_entries =
            assemble_submodules(&submodules, &[], &[libabc.clone(), libdef.clone()]);
        let linked_module = static_link(
            "merged",
            &EffectiveVersion::new(0, 0, 0),
            true,
            &submodule_entries,
        )
        .unwrap();

        // types
        assert_eq!(
            linked_module.type_entries,
            vec![
                TypeEntry::new(vec![], vec![]),
                TypeEntry::new(vec![OperandDataType::I32], vec![OperandDataType::I32]),
                TypeEntry::new(
                    vec![OperandDataType::I32, OperandDataType::I32],
                    vec![OperandDataType::I32]
                ),
                TypeEntry::new(vec![], vec![OperandDataType::I32]),
            ]
        );

        // functions
        let func0 = &linked_module.function_entries[0];
        assert_eq!(func0.type_index, 0);
        assert_eq!(
            format_bytecode_as_text(&func0.code),
            "\
0x0000  04 04 00 00  00 00 00 00    extcall           idx:0
0x0008  40 01 00 00  11 00 00 00    imm_i32           0x00000011
0x0010  04 04 00 00  01 00 00 00    extcall           idx:1
0x0018  c0 03                       end"
        );

        let func1 = &linked_module.function_entries[1];
        assert_eq!(func1.type_index, 1);
        assert_eq!(
            format_bytecode_as_text(&func1.code),
            "\
0x0000  04 04 00 00  00 00 00 00    extcall           idx:0
0x0008  c0 03                       end"
        );

        let func2 = &linked_module.function_entries[2];
        assert_eq!(func2.type_index, 3);
        assert_eq!(
            format_bytecode_as_text(&func2.code),
            "\
0x0000  40 01 00 00  13 00 00 00    imm_i32           0x00000013
0x0008  40 01 00 00  17 00 00 00    imm_i32           0x00000017
0x0010  04 04 00 00  02 00 00 00    extcall           idx:2
0x0018  c0 03                       end"
        );

        // relocate list
        assert_eq!(
            linked_module.relocate_list_entries,
            vec![
                RelocateListEntry::new(vec![
                    RelocateEntry::new(0x4, RelocateType::ExternalFunctionIndex),
                    RelocateEntry::new(0x14, RelocateType::ExternalFunctionIndex),
                ]),
                RelocateListEntry::new(vec![RelocateEntry::new(
                    0x4,
                    RelocateType::ExternalFunctionIndex
                ),]),
                RelocateListEntry::new(vec![RelocateEntry::new(
                    0x14,
                    RelocateType::ExternalFunctionIndex
                ),]),
            ]
        );

        // external libraries
        assert_eq!(linked_module.external_library_entries, vec![libabc, libdef]);

        // external functions
        assert_eq!(
            linked_module.external_function_entries,
            vec![
                ExternalFunctionEntry {
                    name: "do_something".to_owned(),
                    external_library_index: 0,
                    type_index: 0
                },
                ExternalFunctionEntry {
                    name: "do_this".to_owned(),
                    external_library_index: 1,
                    type_index: 1
                },
                ExternalFunctionEntry {
                    name: "do_that".to_owned(),
                    external_library_index: 1,
                    type_index: 2
                },
            ]
        );
    }

    #[test]
    fn test_merge_import_function() {
        let submodule0 = (
            "hello",
            r#"
import fn module::base::add(i32,i32)->i32
import fn module::middle::muladd(i32,i32,i32)->i32

fn main()->i32 {
    call(muladd, imm_i32(0x11), imm_i32(0x13), imm_i32(0x17))
    call(add, imm_i32(0x23), imm_i32(0x29))
}"#,
        );

        let submodule1 = (
            "hello::middle",
            r#"
import fn module::base::add(i32,i32)->i32

fn muladd(left:i32, right:i32, factor:i32)->i32 {
    mul_i32(
        call(add,
            local_load_i32_s(left),
            local_load_i32_s(right)),
        local_load_i32_s(factor))
}"#,
        );

        let submodule2 = (
            "hello::base",
            r#"
fn add(left:i32, right:i32)->i32 {
    add_i32(
        local_load_i32_s(left),
        local_load_i32_s(right))
}"#,
        );

        let submodules = vec![submodule0, submodule1, submodule2];
        let submodule_entries = assemble_submodules(&submodules, &[], &[]);
        let linked_module = static_link(
            "merged",
            &EffectiveVersion::new(0, 0, 0),
            true,
            &submodule_entries,
        )
        .unwrap();

        // import modules
        assert_eq!(
            linked_module.import_module_entries,
            vec![ImportModuleEntry::self_reference_entry()]
        );

        // import functions
        assert!(linked_module.import_function_entries.is_empty());

        // types
        assert_eq!(
            linked_module.type_entries,
            vec![
                TypeEntry {
                    params: vec![],
                    results: vec![]
                },
                TypeEntry {
                    params: vec![OperandDataType::I32, OperandDataType::I32],
                    results: vec![OperandDataType::I32]
                },
                TypeEntry {
                    params: vec![
                        OperandDataType::I32,
                        OperandDataType::I32,
                        OperandDataType::I32
                    ],
                    results: vec![OperandDataType::I32]
                },
                TypeEntry {
                    params: vec![],
                    results: vec![OperandDataType::I32]
                },
            ]
        );

        // local variable list
        assert_eq!(
            linked_module.local_variable_list_entries,
            vec![
                LocalVariableListEntry::new(vec![]),
                LocalVariableListEntry::new(vec![
                    LocalVariableEntry::from_i32(),
                    LocalVariableEntry::from_i32(),
                    LocalVariableEntry::from_i32()
                ]),
                LocalVariableListEntry::new(vec![
                    LocalVariableEntry::from_i32(),
                    LocalVariableEntry::from_i32()
                ])
            ]
        );

        // functions

        // idx 0, main
        let func0 = &linked_module.function_entries[0];
        assert_eq!(func0.type_index, 3);
        assert_eq!(func0.local_variable_list_index, 0);
        assert_eq!(
            format_bytecode_as_text(&func0.code),
            "\
0x0000  40 01 00 00  11 00 00 00    imm_i32           0x00000011
0x0008  40 01 00 00  13 00 00 00    imm_i32           0x00000013
0x0010  40 01 00 00  17 00 00 00    imm_i32           0x00000017
0x0018  00 04 00 00  01 00 00 00    call              idx:1
0x0020  40 01 00 00  23 00 00 00    imm_i32           0x00000023
0x0028  40 01 00 00  29 00 00 00    imm_i32           0x00000029
0x0030  00 04 00 00  02 00 00 00    call              idx:2
0x0038  c0 03                       end"
        );

        // idx 1, muladd
        let func1 = &linked_module.function_entries[1];
        assert_eq!(func1.type_index, 2);
        assert_eq!(func1.local_variable_list_index, 1);
        assert_eq!(
            format_bytecode_as_text(&func1.code),
            "\
0x0000  81 01 00 00  00 00 00 00    local_load_i32_s  rev:0   off:0x00  idx:0
0x0008  81 01 00 00  00 00 01 00    local_load_i32_s  rev:0   off:0x00  idx:1
0x0010  00 04 00 00  02 00 00 00    call              idx:2
0x0018  81 01 00 00  00 00 02 00    local_load_i32_s  rev:0   off:0x00  idx:2
0x0020  04 03                       mul_i32
0x0022  c0 03                       end"
        );

        // idx 2, add
        let func2 = &linked_module.function_entries[2];
        assert_eq!(func2.type_index, 1);
        assert_eq!(func2.local_variable_list_index, 2);
        assert_eq!(
            format_bytecode_as_text(&func2.code),
            "\
0x0000  81 01 00 00  00 00 00 00    local_load_i32_s  rev:0   off:0x00  idx:0
0x0008  81 01 00 00  00 00 01 00    local_load_i32_s  rev:0   off:0x00  idx:1
0x0010  00 03                       add_i32
0x0012  c0 03                       end"
        );

        // data
        assert_eq!(linked_module.read_only_data_entries, vec![]);
        assert_eq!(linked_module.read_write_data_entries, vec![]);
        assert_eq!(linked_module.uninit_data_entries, vec![]);

        // function names
        assert_eq!(
            linked_module.export_function_entries,
            vec![
                ExportFunctionEntry::new("hello::main".to_owned(), Visibility::Private),
                ExportFunctionEntry::new("hello::middle::muladd".to_owned(), Visibility::Private),
                ExportFunctionEntry::new("hello::base::add".to_owned(), Visibility::Private),
            ]
        );

        // relocate list
        assert_eq!(
            linked_module.relocate_list_entries,
            vec![
                RelocateListEntry::new(vec![
                    RelocateEntry::new(0x1c, RelocateType::FunctionPublicIndex),
                    RelocateEntry::new(0x34, RelocateType::FunctionPublicIndex)
                ]),
                RelocateListEntry::new(vec![RelocateEntry::new(
                    0x14,
                    RelocateType::FunctionPublicIndex
                ),]),
                RelocateListEntry::new(vec![]),
            ]
        );
    }

    #[test]
    fn test_merge_function() {
        // todo
    }

    #[test]
    fn test_link_with_unresolved_function_reference() {
        let submodule0 = (
            "hello",
            r#"
import fn module::world::do_this()
import fn module::world::do_that()

fn main()->i32 {
    nop()
}"#,
        );

        let submodule1 = (
            "hello::world",
            r#"
fn do_this() {
    nop()
}"#,
        );

        let submodules = vec![submodule0, submodule1];
        let submodule_entries = assemble_submodules(&submodules, &[], &[]);
        let merged_result = static_link(
            "merged",
            &EffectiveVersion::new(0, 0, 0),
            true,
            &submodule_entries,
        );

        assert!(matches!(
            merged_result,
            Err(LinkerError {
                error_type: LinkErrorType::FunctionNotFound(text)
            }) if text == "hello::world::do_that"
        ));
    }

    #[test]
    fn test_link_with_unresolved_data_reference() {
        let submodule0 = (
            "hello",
            r#"
import data module::world::d0 type i32
import data module::world::d1 type i32

fn main()->i32 {
    nop()
}"#,
        );

        let submodule1 = (
            "hello::world",
            r#"
data d0:i32 = 0x11
"#,
        );

        let submodules = vec![submodule0, submodule1];
        let submodule_entries = assemble_submodules(&submodules, &[], &[]);
        let merged_result = static_link(
            "merged",
            &EffectiveVersion::new(0, 0, 0),
            true,
            &submodule_entries,
        );

        assert!(matches!(
            merged_result,
            Err(LinkerError {
                error_type: LinkErrorType::DataNotFound(text)
            }) if text == "hello::world::d1"
        ));
    }
}
