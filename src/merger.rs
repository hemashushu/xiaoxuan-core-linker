// Copyright (c) 2024 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions,
// more details in file LICENSE, LICENSE.additional and CONTRIBUTING.

use anc_assembler::entry::ImageCommonEntry;
use anc_image::{entry::FunctionNameEntry, module_image::ImageType};

use crate::{
    entry_merger::{
        merge_data_entries, merge_external_function_entries, merge_external_library_entries, merge_import_data_entries, merge_import_function_entries, merge_import_module_entries, merge_local_variable_list_entries, merge_type_entries
    },
    LinkerError,
};

pub type RemapIndices = Vec<usize>;

pub struct RemapTable {
    pub type_remap_indices_list: Vec<RemapIndices>,
    pub data_public_remap_indices_list: Vec<RemapIndices>,
    pub function_public_remap_indices_list: Vec<RemapIndices>,
    pub local_variable_list_remap_indices_list: Vec<RemapIndices>,
    pub external_function_remap_indices_list: Vec<RemapIndices>
}

/// note that not only submodules under the same module can be merged.
pub fn merge_modules(target_module_name: &str, generate_shared_module:bool, submodule_entries: &[ImageCommonEntry]) -> Result<ImageCommonEntry, LinkerError> {
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

    // resolve the relocation (remap) data

    let remap_table = RemapTable{
        type_remap_indices_list,
        data_public_remap_indices_list,
        function_public_remap_indices_list,
        local_variable_list_remap_indices_list,
        external_function_remap_indices_list,
    };

    // todo
    let function_entries = vec![];

    if generate_shared_module {
        // check imported functons and data.
        // to ensure there is no imported item from the "current" module.
        // todo
    }

    let merged_image_common_entry =  ImageCommonEntry{
        name: target_module_name.to_owned(),
        image_type: if generate_shared_module {ImageType::SharedModule} else {ImageType::ObjectFile},
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
        external_library_entries,
        external_function_entries,
    };

    Ok(merged_image_common_entry)
}

#[cfg(test)]
mod tests {
    use anc_assembler::{assembler::assemble_module_node, entry::ImageCommonEntry};
    use anc_image::entry::{ExternalLibraryEntry, ImportModuleEntry};
    use anc_parser_asm::parser::parse_from_str;

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
    fn test_merge_functions() {
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
        let common_entries = assemble_submodules(&submodules, &[], &[]);

        println!("{:#?}", common_entries);
    }
}
