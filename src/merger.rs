// Copyright (c) 2024 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions,
// more details in file LICENSE, LICENSE.additional and CONTRIBUTING.

use anc_assembler::entry::ImageCommonEntry;
use anc_image::entry::FunctionNameEntry;

use crate::{
    entry_merger::{
        merge_data_entries, merge_import_data_entries, merge_import_function_entries,
        merge_import_module_entries, merge_local_variable_list_entries, merge_type_entries,
    },
    LinkerError,
};

pub type RemapIndices = Vec<usize>;

pub struct ModuleEntryRemap {
    // pub import_module_remap_indices_list: Vec<RemapIndices>,
    pub type_remap_indices_list: Vec<RemapIndices>,
    pub local_variable_list_remap_indices_list: Vec<RemapIndices>,
    pub data_public_remap_indices_list: Vec<RemapIndices>,
    pub function_public_remap_indices_list: Vec<RemapIndices>,
}

/// note that not only submodules under the same module can be merged.
pub fn merge_modules(submodule_entries: &[ImageCommonEntry]) -> Result<(), LinkerError> {
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

    let (import_data_entries, data_public_remap_indices_list) = merge_import_data_entries(
        &data_name_entries,
        &internal_data_remap_indices_list,
        &import_module_remap_indices_list,
        &import_data_entries_list,
    );

    // todo merge external libraries

    // todo merge external functions

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

    Ok(())
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
