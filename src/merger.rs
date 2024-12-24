// Copyright (c) 2024 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions,
// more details in file LICENSE, LICENSE.additional and CONTRIBUTING.

use anc_assembler::{
    assembler::create_self_dependent_import_module_entry, entry::ImageCommonEntry,
};
use anc_image::{entry::FunctionNameEntry, module_image::ImageType};

use crate::{
    entry_merger::{
        merge_data_entries, merge_external_function_entries, merge_external_library_entries,
        merge_function_entries, merge_import_data_entries, merge_import_function_entries,
        merge_import_module_entries, merge_local_variable_list_entries, merge_type_entries,
    },
    LinkErrorType, LinkerError,
};

pub type RemapIndices = Vec<usize>;

pub struct RemapTable<'a> {
    pub type_remap_indices: &'a RemapIndices,
    pub data_public_remap_indices: &'a RemapIndices,
    pub function_public_remap_indices: &'a RemapIndices,
    pub local_variable_list_remap_indices: &'a RemapIndices,
    pub external_function_remap_indices: &'a RemapIndices,
}

/// note that not only submodules under the same module can be merged.
pub fn merge_modules(
    target_module_name: &str,
    generate_shared_module: bool,
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

    // merge data name and data entries
    let data_name_entries_list = submodule_entries
        .iter()
        .map(|item| item.data_name_entries.as_slice())
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
        data_name_entries,
        read_only_data_entries,
        read_write_data_entries,
        uninit_data_entries,
        internal_data_remap_indices_list,
    ) = merge_data_entries(
        &data_name_entries_list,
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
        &data_name_entries,
        &internal_data_remap_indices_list,
        &import_module_remap_indices_list,
        &import_data_entries_list,
    );

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
    let mut function_name_entries: Vec<FunctionNameEntry> = vec![];
    let mut internal_function_remap_indices_list: Vec<RemapIndices> = vec![];

    for submodule_entry in submodule_entries {
        let indices = (function_name_entries.len()
            ..function_name_entries.len() + submodule_entry.function_name_entries.len())
            .collect::<Vec<_>>();
        internal_function_remap_indices_list.push(indices);
        function_name_entries.extend(submodule_entry.function_name_entries.to_vec());
    }

    // merge import function entries
    let import_function_entries_list = submodule_entries
        .iter()
        .map(|item| item.import_function_entries.as_slice())
        .collect::<Vec<_>>();
    let (import_function_entries, function_public_remap_indices_list) =
        merge_import_function_entries(
            &function_name_entries,
            &internal_function_remap_indices_list,
            &import_module_remap_indices_list,
            &type_remap_indices_list,
            &import_function_entries_list,
        );

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

    // validate the shared-module
    if generate_shared_module {
        // check imported functons and data,
        // to make sure there are no functions or data
        // imported from the "current" module.

        let the_current_module = create_self_dependent_import_module_entry();
        let pos_opt = import_module_entries
            .iter()
            .position(|item| item == &the_current_module);

        if let Some(pos) = pos_opt {
            for import_function_entry in &import_function_entries {
                if import_function_entry.import_module_index == pos {
                    return Err(LinkerError::new(LinkErrorType::UnresolvedFunctionName(
                        import_function_entry.full_name.to_owned(),
                    )));
                }
            }

            for import_data_entry in &import_data_entries {
                if import_data_entry.import_module_index == pos {
                    return Err(LinkerError::new(LinkErrorType::UnresolvedDataName(
                        import_data_entry.full_name.to_owned(),
                    )));
                }
            }
        }
    }

    let image_type = if generate_shared_module {
        ImageType::SharedModule
    } else {
        ImageType::ObjectFile
    };

    let merged_image_common_entry = ImageCommonEntry {
        name: target_module_name.to_owned(),
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
        function_name_entries,
        data_name_entries,
        relocate_list_entries,
        external_library_entries,
        external_function_entries,
    };

    Ok(merged_image_common_entry)
}

#[cfg(test)]
mod tests {
    use anc_assembler::{
        assembler::{assemble_module_node, create_self_dependent_import_module_entry},
        entry::ImageCommonEntry,
    };
    use anc_image::entry::{ExternalLibraryEntry, ImportModuleEntry};
    use anc_isa::{DependencyShare, ModuleDependency};
    use anc_parser_asm::parser::parse_from_str;

    use crate::{entry_merger::merge_import_module_entries, merger::merge_modules};

    struct SubModule<'a> {
        fullname: &'a str,
        source: &'a str,
    }

    fn assemble_submodules(
        submodules: &[SubModule],
        import_module_entries: &[ImportModuleEntry],
        external_library_entries: &[ExternalLibraryEntry],
    ) -> Vec<ImageCommonEntry> {
        // let mut module_images = vec![];
        let mut common_entries = vec![];

        for submodule in submodules {
            let module_node = match parse_from_str(submodule.source) {
                Ok(node) => node,
                Err(parser_error) => {
                    panic!("{}", parser_error.with_source(submodule.source));
                }
            };

            let image_common_entry = assemble_module_node(
                &module_node,
                &submodule.fullname,
                import_module_entries,
                external_library_entries,
            )
            .unwrap();

            common_entries.push(image_common_entry);
        }

        common_entries
    }

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
    fn test_merge_import_module_entries_with_version_conflict() {
        // todo
    }

    #[test]
    fn test_merge_import_data() {
        // todo
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
    fn test_merge_import_function() {
        let module_main = SubModule {
            fullname: "hello_mod",
            source: "
    import fn module::math::add(i32,i32)->i32

    fn main()->i32 {
        call(add, imm_i32(11), imm_i32(13))
    }",
        };

        let module_math = SubModule {
            fullname: "hello_mod::math",
            source: "
    fn add(left:i32, right:i32)->i32 {
        add_i32(
            local_load_i32_s(left)
            local_load_i32_s(right)
        )
    }",
        };

        let submodules = vec![module_main, module_math];
        let submodule_entries = assemble_submodules(&submodules, &[], &[]);
        let merged_entry = merge_modules("merged", false, &submodule_entries).unwrap();

        println!("{:#?}", merged_entry);
    }

    #[test]
    fn test_merge_function() {
        // todo
    }

    #[test]
    fn test_merge_with_relocate() {
        // todo
    }

    #[test]
    fn test_merge_with_unresolved_function() {
        // todo
    }

    #[test]
    fn test_merge_with_unresolved_data() {
        // todo
    }
}
