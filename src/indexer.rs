// Copyright (c) 2024 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions,
// more details in file LICENSE, LICENSE.additional and CONTRIBUTING.

use std::collections::VecDeque;

use anc_assembler::assembler::create_self_reference_import_module_entry;
use anc_image::{
    entry::{
        DataIndexEntry, DataIndexListEntry, EntryPointEntry, ExternalFunctionEntry,
        ExternalFunctionIndexEntry, ExternalFunctionIndexListEntry, FunctionIndexEntry,
        FunctionIndexListEntry, ImageCommonEntry, ImageIndexEntry, ImportModuleEntry, TypeEntry,
    },
    module_image::Visibility,
};
use anc_isa::{DataSectionType, ModuleDependency};

use crate::{
    linker::RemapIndices, merger::merge_external_library_entries, LinkErrorType, LinkerError,
    DEFAULT_ENTRY_FUNCTION_NAME,
};

/// When an application is loaded, all its dependent modules must also be loaded.
/// When loading these modules, we should follow a specific order:
/// load the modules that are farthest (deepest) from the application module first,
/// and then load the ones that are closer (shallower).
///
/// This function implements sorting all dependent modules.
/// Here's how it works:
/// Consider a dependency tree as shown below:
///
/// ```text
///           [a]
///           /|\
///    /-<---/ | \------>--\
///    |   |   |           |
///    |   |   v           |
///   [b] [c] [d]<-----\  [e]
///    |   |   |       |   |
///    |   |   |---\   |  [f]
///    |   |   |   |   |   |
///    v   \->[g] [h]  \---/
///    |       |   |
///    |       |  [i]
///    |       |   |
///    \------\|/--/
///           [j]
/// ```
///
/// Based on the paths, the level (i.e. the number of nodes you pass through from the
/// application module to the target module) of module 'd' can be either 1 or 3.
/// We should choose the maximum value, which is 3. Similarly, for module 'g', the level
/// can be 2 or 4, and we choose 4.
///
/// Using this approach, we can determine the maximum level for each module. By sorting
/// these modules from the lowest level to the highest, we get the sorted list:
///
/// depth:   0       1         2      3      4        5      6
/// order:  (a) -> (b,c,e) -> (f) -> (d) -> (g,h) -> (i) -> (j)
///
///
/// requires:
/// - the first module should be the application itself.
/// - all dependent modules should be resolved and have no conflict.
pub fn sort_modules(
    mut image_common_entries: Vec<ImageCommonEntry>,
) -> (Vec<ImageCommonEntry>, Vec<ImportModuleEntry>) {
    let mut dependencies: Vec<(ImportModuleEntry, usize)> = vec![]; // all dependencies
    let mut queue: VecDeque<(&ImageCommonEntry, usize)> = VecDeque::new();

    // push the first module, i.e., the application module.
    queue.push_back((&image_common_entries[0], 0));

    // push the current module.
    // note that the name is the actual name of the module instead of "module",
    // this is because the name is used for comparison.
    dependencies.push((
        ImportModuleEntry::new(
            image_common_entries[0].name.clone(),
            Box::new(ModuleDependency::Current),
        ),
        0,
    ));

    let self_reference_module = create_self_reference_import_module_entry();

    while !queue.is_empty() {
        let (parent_module, parent_depth) = queue.pop_front().unwrap();
        let current_depth = parent_depth + 1;

        for dependency_new in &parent_module.import_module_entries {
            // skip the self reference item
            if dependency_new == &self_reference_module {
                continue;
            }

            let index_opt = dependencies
                .iter()
                .position(|item| item.0.name == dependency_new.name);

            if let Some(index) = index_opt {
                if dependencies[index].1 < current_depth {
                    // update the depth
                    dependencies[index].1 = current_depth;
                    // add to queue to re-calculate the depth of each subnode.
                    queue.push_back((
                        image_common_entries
                            .iter()
                            .find(|item| item.name == dependency_new.name)
                            .unwrap(),
                        current_depth,
                    ));
                }
            } else {
                // add import entry
                dependencies.push((dependency_new.to_owned(), current_depth));

                // add to queue to calculate the depth of each subnode.
                queue.push_back((
                    image_common_entries
                        .iter()
                        .find(|item| item.name == dependency_new.name)
                        .unwrap(),
                    current_depth,
                ));
            }
        }
    }

    // sort the dependencies
    dependencies.sort_by(|left, right| left.1.cmp(&right.1));

    // sort the modules
    image_common_entries.sort_by(|left, right| {
        let depth_left = dependencies
            .iter()
            .find(|item| item.0.name == left.name)
            .unwrap()
            .1;
        let depth_right = dependencies
            .iter()
            .find(|item| item.0.name == right.name)
            .unwrap()
            .1;
        depth_left.cmp(&depth_right)
    });

    let mut entries = dependencies
        .into_iter()
        .map(|(entry, _)| entry)
        .collect::<Vec<_>>();

    // replace the self reference module with the name of "module"
    entries[0] = self_reference_module;

    (image_common_entries, entries)
}

pub fn build_indices(
    image_commmon_entries: &[ImageCommonEntry],
    module_entries: &[ImportModuleEntry], // all dependencies
) -> Result<ImageIndexEntry, LinkerError> {
    let mut function_index_list_entries: Vec<FunctionIndexListEntry> = vec![];
    for (source_module_index, source_module_entry) in image_commmon_entries.iter().enumerate() {
        let mut function_index_entries: Vec<FunctionIndexEntry> = vec![];

        // add imported functon indices
        for import_function_entry in source_module_entry.import_function_entries.iter() {
            let target_module_name = &source_module_entry.import_module_entries
                [import_function_entry.import_module_index]
                .name;
            let target_module_index = image_commmon_entries
                .iter()
                .position(|item| &item.name == target_module_name)
                .unwrap();
            let target_module = &image_commmon_entries[target_module_index];

            let expected_full_name = &import_function_entry.full_name;
            let target_function_internal_index_opt = target_module
                .export_function_entries
                .iter()
                .position(|item| &item.full_name == expected_full_name);

            if let Some(target_function_internal_index) = target_function_internal_index_opt {
                // check visibility
                if target_module.export_function_entries[target_function_internal_index].visibility
                    != Visibility::Public
                {
                    return Err(LinkerError::new(LinkErrorType::FunctionNotExported(
                        expected_full_name.to_owned(),
                    )));
                }

                let target_function_entry =
                    &target_module.function_entries[target_function_internal_index];

                // check signature
                let expected_type =
                    &source_module_entry.type_entries[import_function_entry.type_index];
                let actual_type = &target_module.type_entries[target_function_entry.type_index];

                if expected_type != actual_type {
                    return Err(LinkerError::new(LinkErrorType::ImportFunctionTypeMismatch(
                        expected_full_name.to_owned(),
                    )));
                }

                // add index item
                function_index_entries.push(FunctionIndexEntry::new(
                    target_module_index,
                    target_function_internal_index,
                ));
            } else {
                return Err(LinkerError::new(LinkErrorType::FunctionNotFound(
                    expected_full_name.to_owned(),
                )));
            }
        }

        // add internal functon indices
        for function_internal_index in 0..source_module_entry.function_entries.len() {
            function_index_entries.push(FunctionIndexEntry::new(
                source_module_index,
                function_internal_index,
            ));
        }

        // complete one module list
        function_index_list_entries.push(FunctionIndexListEntry::new(function_index_entries));
    }

    let mut data_index_list_entries: Vec<DataIndexListEntry> = vec![];
    for (source_module_index, source_module_entry) in image_commmon_entries.iter().enumerate() {
        let mut data_index_entries: Vec<DataIndexEntry> = vec![];

        // add imported data indices
        for import_data_entry in source_module_entry.import_data_entries.iter() {
            let target_module_name = &source_module_entry.import_module_entries
                [import_data_entry.import_module_index]
                .name;
            let target_module_index = image_commmon_entries
                .iter()
                .position(|item| &item.name == target_module_name)
                .unwrap();
            let target_module = &image_commmon_entries[target_module_index];

            let expected_full_name = &import_data_entry.full_name;
            let target_data_internal_index_opt = target_module
                .export_data_entries
                .iter()
                .position(|item| &item.full_name == expected_full_name);

            if let Some(target_data_internal_index) = target_data_internal_index_opt {
                // check data section type
                let target_export_data_entry =
                    &target_module.export_data_entries[target_data_internal_index];

                if target_export_data_entry.section_type != import_data_entry.data_section_type {
                    return Err(LinkerError::new(LinkErrorType::ImportDataSectionMismatch(
                        expected_full_name.to_owned(),
                        import_data_entry.data_section_type,
                    )));
                }

                // check visibility
                if target_export_data_entry.visibility != Visibility::Public {
                    return Err(LinkerError::new(LinkErrorType::DataNotExported(
                        expected_full_name.to_owned(),
                    )));
                }

                // check type
                // let expected_type = ...;
                // let actual_type = import_data_entry.memory_data_type;
                // if expected_type != actual_type {
                //     return Err(LinkerError::new(LinkErrorType::ImportDataTypeMismatch(
                //         expected_full_name.to_owned(),
                //     )));
                // }

                // add index item
                data_index_entries.push(DataIndexEntry::new(
                    target_module_index,
                    target_data_internal_index,
                    target_export_data_entry.section_type,
                ));
            } else {
                return Err(LinkerError::new(LinkErrorType::DataNotFound(
                    expected_full_name.to_owned(),
                )));
            }
        }

        // add internal data indices, .rodata
        for data_internal_index in 0..source_module_entry.read_only_data_entries.len() {
            data_index_entries.push(DataIndexEntry::new(
                source_module_index,
                data_internal_index,
                DataSectionType::ReadOnly,
            ));
        }

        // add internal data indices, .data
        for data_internal_index in 0..source_module_entry.read_write_data_entries.len() {
            data_index_entries.push(DataIndexEntry::new(
                source_module_index,
                data_internal_index,
                DataSectionType::ReadWrite,
            ));
        }

        // add internal data indices, .bss
        for data_internal_index in 0..source_module_entry.uninit_data_entries.len() {
            data_index_entries.push(DataIndexEntry::new(
                source_module_index,
                data_internal_index,
                DataSectionType::Uninit,
            ));
        }

        // complete one list
        data_index_list_entries.push(DataIndexListEntry::new(data_index_entries));
    }

    // merge external library
    let external_library_entries_list = image_commmon_entries
        .iter()
        .map(|item| item.external_library_entries.as_slice())
        .collect::<Vec<_>>();
    let (external_library_entries, external_library_remap_indices_list) =
        merge_external_library_entries(&external_library_entries_list)?;

    // merge external function and type entries
    let type_entries_list = image_commmon_entries
        .iter()
        .map(|item| item.type_entries.as_slice())
        .collect::<Vec<_>>();

    let external_function_entries_list = image_commmon_entries
        .iter()
        .map(|item| item.external_function_entries.as_slice())
        .collect::<Vec<_>>();

    let (
        type_entries_merged,
        external_function_entries_merged,
        external_function_remap_indices_list,
    ) = build_external_function_and_type_entries(
        &external_library_remap_indices_list,
        &type_entries_list,
        &external_function_entries_list,
    );

    let external_function_index_entries = external_function_remap_indices_list
        .iter()
        .map(|indices| {
            let index_entries = indices
                .iter()
                .map(|index| ExternalFunctionIndexEntry::new(*index))
                .collect::<Vec<_>>();
            ExternalFunctionIndexListEntry::new(index_entries)
        })
        .collect::<Vec<_>>();

    // search the entry points:
    // - 'app_module_name::_start' for the default entry point
    // - 'app_module_name::*::_start' for the execute units
    // - 'app_module_name::*::test_*' for unit tests
    //
    // the entry point names:
    // - empty string for the default entry point (in file "main.anca").
    // - submodule name for the executable units in the "app" folder.
    // - submodule and function name ("test_*") for the unit test (in folder "tests").
    // todo:

    let default_entry_point_full_name = format!(
        "{}::{}",
        image_commmon_entries[0].name, DEFAULT_ENTRY_FUNCTION_NAME
    );
    let default_entry_point_index_opt = image_commmon_entries[0]
        .export_function_entries
        .iter()
        .position(|item| item.full_name == default_entry_point_full_name);

    let default_entry_point_function_public_index = if let Some(idx) = default_entry_point_index_opt
    {
        image_commmon_entries[0].import_function_entries.len() + idx
    } else {
        return Err(LinkerError::new(LinkErrorType::EntryPointNotFound(
            DEFAULT_ENTRY_FUNCTION_NAME.to_owned(),
        )));
    };

    // entry point section
    let entry_point_entries = vec![EntryPointEntry::new(
        "".to_string(), // the name of default entry point is empty string
        default_entry_point_function_public_index,
    )];
    // let (entry_point_items, unit_names_data) =
    //     EntryPointSection::convert_from_entries(&entry_point_entries);
    // let entry_point_section = EntryPointSection {
    //     items: &entry_point_items,
    //     unit_names_data: &unit_names_data,
    // };

    let image_index_entry = ImageIndexEntry {
        function_index_list_entries,
        entry_point_entries,
        data_index_list_entries,
        unified_external_library_entries: external_library_entries,
        unified_external_type_entries: type_entries_merged,
        unified_external_function_entries: external_function_entries_merged,
        external_function_index_entries,
        module_entries: module_entries.to_vec(),
    };

    Ok(image_index_entry)
}

fn build_external_function_and_type_entries(
    external_library_remap_indices_list: &[RemapIndices],
    type_entries_list: &[&[TypeEntry]],
    external_function_entries_list: &[&[ExternalFunctionEntry]],
) -> (
    /* type_entries */ Vec<TypeEntry>,
    /* external_function_entries */ Vec<ExternalFunctionEntry>,
    /* external_function_remap_indices_list */ Vec<RemapIndices>,
) {
    let mut type_entries_merged: Vec<TypeEntry> = vec![];
    let mut external_function_entries_merged: Vec<ExternalFunctionEntry> = vec![];
    let mut external_function_remap_indices_list: Vec<RemapIndices> = vec![];

    for (submodule_index, external_function_entries) in
        external_function_entries_list.iter().enumerate()
    {
        let mut indices = vec![];

        for external_function_entry_source in external_function_entries.iter() {
            let type_index_source = external_function_entry_source.type_index;
            let type_entry_source = &type_entries_list[submodule_index][type_index_source];
            let type_index_merged_opt = type_entries_merged
                .iter()
                .position(|item| item == type_entry_source);

            let type_index_merged = match type_index_merged_opt {
                Some(pos_merged) => {
                    // found exists
                    pos_merged
                }
                None => {
                    // add entry
                    let pos_new = type_entries_merged.len();
                    type_entries_merged.push(type_entry_source.to_owned());
                    pos_new
                }
            };

            let external_library_index_merged = external_library_remap_indices_list
                [submodule_index][external_function_entry_source.external_library_index];

            // how to determine if two external functions are the same?
            // Is it just checking the function name like in C/ELF programs,
            // includes the library name?
            let pos_merged_opt = external_function_entries_merged.iter().position(|item| {
                item.name == external_function_entry_source.name
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
                    let pos_new = external_function_entries_merged.len();

                    let external_function_entry_merged = ExternalFunctionEntry::new(
                        external_function_entry_source.name.clone(),
                        external_library_index_merged,
                        type_index_merged,
                    );
                    external_function_entries_merged.push(external_function_entry_merged);
                    indices.push(pos_new);
                }
            }
        }

        external_function_remap_indices_list.push(indices);
    }

    (
        type_entries_merged,
        external_function_entries_merged,
        external_function_remap_indices_list,
    )
}

#[cfg(test)]
mod tests {

    use pretty_assertions::assert_eq;

    use anc_assembler::assembler::assemble_module_node;
    use anc_image::{
        bytecode_reader::format_bytecode_as_text,
        entry::{
            DataIndexEntry, ExternalFunctionEntry, ExternalFunctionIndexEntry,
            ExternalLibraryEntry, FunctionIndexEntry, ImageCommonEntry, ImageIndexEntry,
            ImportModuleEntry, TypeEntry,
        },
        module_image::ImageType,
    };
    use anc_isa::{DataSectionType, ExternalLibraryDependency, ModuleDependency, OperandDataType};
    use anc_parser_asm::parser::parse_from_str;

    use crate::indexer::build_indices;

    use super::sort_modules;

    fn build_module(
        module_name: &str,
        source_code: &str,
        import_module_entries: Vec<ImportModuleEntry>,
        external_library_entries: Vec<ExternalLibraryEntry>,
    ) -> ImageCommonEntry {
        let module_node = match parse_from_str(source_code) {
            Ok(node) => node,
            Err(parser_error) => {
                panic!("{}", parser_error.with_source(source_code));
            }
        };

        assemble_module_node(
            &module_node,
            module_name,
            &import_module_entries,
            &external_library_entries,
        )
        .unwrap()
    }

    fn build_index(
        original_image_common_entries: Vec<ImageCommonEntry>,
    ) -> (Vec<ImageCommonEntry>, ImageIndexEntry) {
        let (image_common_entries, module_entries) = sort_modules(original_image_common_entries);
        let image_index_entry = build_indices(&image_common_entries, &module_entries).unwrap();
        (image_common_entries, image_index_entry)
    }

    #[test]
    fn test_sort_modules() {
        let make_dependency = |name: &str| {
            ImportModuleEntry::new(name.to_owned(), Box::new(ModuleDependency::Runtime))
        };

        let make_module_entry = |name: &str, dependency_names: &[&str]| {
            let import_module_entries = dependency_names
                .iter()
                .map(|item| make_dependency(item))
                .collect::<Vec<_>>();

            ImageCommonEntry {
                name: name.to_owned(),
                image_type: ImageType::SharedModule,
                import_module_entries,
                import_function_entries: vec![],
                import_data_entries: vec![],
                type_entries: vec![],
                local_variable_list_entries: vec![],
                function_entries: vec![],
                read_only_data_entries: vec![],
                read_write_data_entries: vec![],
                uninit_data_entries: vec![],
                export_function_entries: vec![],
                export_data_entries: vec![],
                relocate_list_entries: vec![],
                external_library_entries: vec![],
                external_function_entries: vec![],
            }
        };

        let modules = vec![
            make_module_entry("a", &["b", "c", "d", "e"]),
            make_module_entry("b", &["j"]),
            make_module_entry("c", &["g"]),
            make_module_entry("d", &["g", "h"]),
            make_module_entry("e", &["f"]),
            make_module_entry("f", &["d"]),
            make_module_entry("g", &["j"]),
            make_module_entry("h", &["i"]),
            make_module_entry("i", &["j"]),
            make_module_entry("j", &[]),
        ];

        let (sorted_modules, dependencies) = sort_modules(modules);

        assert_eq!(
            "a,b,c,e,f,d,g,h,i,j",
            sorted_modules
                .iter()
                .map(|item| item.name.to_owned())
                .collect::<Vec<String>>()
                .join(",")
        );

        assert_eq!(
            "module,b,c,e,f,d,g,h,i,j",
            dependencies
                .iter()
                .map(|item| item.name.to_owned())
                .collect::<Vec<String>>()
                .join(",")
        );
    }

    #[test]
    fn test_build_index_functions_and_data() {
        // app, module index = 0
        let entry_app = build_module(
            "app",
            r#"
import fn std::add(i32,i32) -> i32  // func pub idx: 0 (target mod idx: 2, target func inter idx: 0)
import fn math::inc(i32) -> i32     // func pub idx: 1 (target mod idx: 1, target func inter idx: 0)
import fn std::sub(i32,i32) -> i32  // func pub idx: 2 (target mod idx: 2, target func inter idx: 1)

import uninit data std::errno type i32      // data pub idx: 2 (target mod idx: 2, target data idx: 0, section: 2)
import readonly data math::MAGIC type i32   // data pub idx: 0 (target mod idx: 1, target data idx: 0, section: 0)
import readonly data math::POWER type i32   // data pub idx: 1 (target mod idx: 1, target data idx: 1, section: 0)

data foo:i32 = 11                   // data pub idx: 3 (target mod idx: 0, target data idx: 0, section: 1)
data bar:i32 = 13                   // data pub idx: 4 (target mod idx: 0, target data idx: 1, section: 1)

fn _start() {                       // func pub idx: 3 (target mod idx: 0, target func inter idx: 0)
    when ne_i32(
        data_load_i32_s(MAGIC)      // data pub idx: 0
        imm_i32(42))
        panic(1)

    when ne_i32(
        call(test)                  // func pub idx: 4
        imm_i32(66))
        panic(2)

    when ne_i32(
        data_load_i32_s(errno)      // data pub idx: 2
        imm_i32(19))
        panic(3)
}

fn test() -> i32 {                  // func pub idx: 4 (target mod idx: 0, target func inter idx: 1)
    // returns (11 + 13) + 42 = 66
    call(inc                        // func pub idx: 1
        call(add                    // func pub idx: 0
            data_load_i32_s(foo)    // data pub idx: 3
            data_load_i32_s(bar)))  // data pub idx: 4
}
"#,
            vec![
                ImportModuleEntry::new("math".to_owned(), Box::new(ModuleDependency::Runtime)),
                ImportModuleEntry::new("std".to_owned(), Box::new(ModuleDependency::Runtime)),
            ],
            vec![],
        );

        // math, module index = 1
        let entry_math = build_module(
            "math",
            r#"
import fn std::sub(i32,i32) -> i32      // func pub idx: 0 (target mod idx: 2, target func inter idx: 1)
import fn std::add(i32,i32) -> i32      // func pub idx: 1 (target mod idx: 2, target func inter idx: 0)
import uninit data std::errno type i32  // data pub idx: 0 (target mod idx: 2, target data idx: 0, section: 2)

pub readonly data MAGIC:i32 = 42        // data pub idx: 1 (target mod idx: 1, target data idx: 0, section: 0)
pub readonly data POWER:i32 = 24        // data pub idx: 2 (target mod idx: 1, target data idx: 1, section: 0)

pub fn inc(num:i32) -> i32 {            // func pub idx: 2 (target mod idx: 1, target func inter idx: 0)
    // returns num + MAGIC
    data_store_i32(
        errno                           // data pub idx: 0
        imm_i32(19))
    call(add                            // func pub idx: 1
        data_load_i32_s(MAGIC)          // data pub idx: 1
        local_load_i32_s(num))
}
"#,
            vec![ImportModuleEntry::new(
                "std".to_owned(),
                Box::new(ModuleDependency::Runtime),
            )],
            vec![],
        );

        // std, module index = 2
        let entry_std = build_module(
            "std",
            r#"
pub uninit data errno:i32                   // data pub idx: 0 (target mod idx: 2, target data idx: 0, section: 2)

pub fn add(left:i32, right:i32) -> i32 {    // func pub idx: 0 (target mod idx: 2, target func inter idx: 0)
    add_i32(
        local_load_i32_s(left)
        local_load_i32_s(right))
}

pub fn sub(left:i32, right:i32) -> i32 {    // func pub idx: 0 (target mod idx: 2, target func inter idx: 1)
    sub_i32(
        local_load_i32_s(left)
        local_load_i32_s(right))
}
"#,
            vec![],
            vec![],
        );

        let (image_common_entries, image_index_entry) =
            build_index(vec![entry_app, entry_math, entry_std]);

        // save image
        // let mut f = File::create_new("/tmp/test.anci").unwrap();
        // write_image_file(&image_common_entries[0], &image_index_entry, &mut f).unwrap();

        // check function index list
        assert_eq!(
            image_index_entry.function_index_list_entries[0].index_entries,
            vec![
                FunctionIndexEntry::new(2, 0),
                FunctionIndexEntry::new(1, 0),
                FunctionIndexEntry::new(2, 1),
                FunctionIndexEntry::new(0, 0),
                FunctionIndexEntry::new(0, 1),
            ]
        );

        assert_eq!(
            image_index_entry.function_index_list_entries[1].index_entries,
            vec![
                FunctionIndexEntry::new(2, 1),
                FunctionIndexEntry::new(2, 0),
                FunctionIndexEntry::new(1, 0),
            ]
        );

        assert_eq!(
            image_index_entry.function_index_list_entries[2].index_entries,
            vec![FunctionIndexEntry::new(2, 0), FunctionIndexEntry::new(2, 1),]
        );

        // check data index list
        assert_eq!(
            image_index_entry.data_index_list_entries[0].index_entries,
            vec![
                DataIndexEntry::new(1, 0, DataSectionType::ReadOnly),
                DataIndexEntry::new(1, 1, DataSectionType::ReadOnly),
                DataIndexEntry::new(2, 0, DataSectionType::Uninit),
                DataIndexEntry::new(0, 0, DataSectionType::ReadWrite),
                DataIndexEntry::new(0, 1, DataSectionType::ReadWrite),
            ]
        );

        assert_eq!(
            image_index_entry.data_index_list_entries[1].index_entries,
            vec![
                DataIndexEntry::new(2, 0, DataSectionType::Uninit),
                DataIndexEntry::new(1, 0, DataSectionType::ReadOnly),
                DataIndexEntry::new(1, 1, DataSectionType::ReadOnly),
            ]
        );

        assert_eq!(
            image_index_entry.data_index_list_entries[2].index_entries,
            vec![DataIndexEntry::new(2, 0, DataSectionType::Uninit),]
        );

        // check module list
        assert_eq!(
            image_index_entry.module_entries,
            vec![
                ImportModuleEntry::new("module".to_owned(), Box::new(ModuleDependency::Current)),
                ImportModuleEntry::new("math".to_owned(), Box::new(ModuleDependency::Runtime)),
                ImportModuleEntry::new("std".to_owned(), Box::new(ModuleDependency::Runtime)),
            ]
        );

        assert_eq!(
            format_bytecode_as_text(&image_common_entries[0].function_entries[0].code),
            "\
0x0000  c1 01 00 00  00 00 00 00    data_load_i32_s   off:0x00  idx:0
0x0008  40 01 00 00  2a 00 00 00    imm_i32           0x0000002a
0x0010  c3 02                       ne_i32
0x0012  00 01                       nop
0x0014  c6 03 00 00  00 00 00 00    block_nez         local:0   off:0x16
        16 00 00 00
0x0020  40 04 00 00  01 00 00 00    panic             code:1
0x0028  c0 03                       end
0x002a  00 01                       nop
0x002c  00 04 00 00  04 00 00 00    call              idx:4
0x0034  40 01 00 00  42 00 00 00    imm_i32           0x00000042
0x003c  c3 02                       ne_i32
0x003e  00 01                       nop
0x0040  c6 03 00 00  00 00 00 00    block_nez         local:0   off:0x16
        16 00 00 00
0x004c  40 04 00 00  02 00 00 00    panic             code:2
0x0054  c0 03                       end
0x0056  00 01                       nop
0x0058  c1 01 00 00  02 00 00 00    data_load_i32_s   off:0x00  idx:2
0x0060  40 01 00 00  13 00 00 00    imm_i32           0x00000013
0x0068  c3 02                       ne_i32
0x006a  00 01                       nop
0x006c  c6 03 00 00  00 00 00 00    block_nez         local:0   off:0x16
        16 00 00 00
0x0078  40 04 00 00  03 00 00 00    panic             code:3
0x0080  c0 03                       end
0x0082  c0 03                       end"
        );

        assert_eq!(
            format_bytecode_as_text(&image_common_entries[0].function_entries[1].code),
            "\
0x0000  c1 01 00 00  03 00 00 00    data_load_i32_s   off:0x00  idx:3
0x0008  c1 01 00 00  04 00 00 00    data_load_i32_s   off:0x00  idx:4
0x0010  00 04 00 00  00 00 00 00    call              idx:0
0x0018  00 04 00 00  01 00 00 00    call              idx:1
0x0020  c0 03                       end"
        );

        assert_eq!(
            format_bytecode_as_text(&image_common_entries[1].function_entries[0].code),
            "\
0x0000  40 01 00 00  13 00 00 00    imm_i32           0x00000013
0x0008  ca 01 00 00  00 00 00 00    data_store_i32    off:0x00  idx:0
0x0010  c1 01 00 00  01 00 00 00    data_load_i32_s   off:0x00  idx:1
0x0018  81 01 00 00  00 00 00 00    local_load_i32_s  rev:0   off:0x00  idx:0
0x0020  00 04 00 00  01 00 00 00    call              idx:1
0x0028  c0 03                       end"
        );
    }

    #[test]
    fn test_build_index_external_functions() {
        // app, module index = 0
        let entry_app = build_module(
            "app",
            r#"
external fn hello::x(i32,i32)-> i32 // internal: lib(2),idx(0)  unified: lib(2),idx(0),type(0)
external fn foo::b(i32) -> i32      // internal: lib(0),idx(1)  unified: lib(0),idx(1),type(1)
external fn bar::m(i32, i32)        // internal: lib(1),idx(2)  unified: lib(1),idx(2),type(2)
external fn hello::y(i32)           // internal: lib(2),idx(3)  unified: lib(2),idx(3),type(3)

fn _start()->i32 {
    extcall(x, imm_i32(0x53), imm_i32(0x59))    // idx 0
    extcall(b, imm_i32(0x61))                   // idx 1
    extcall(m, imm_i32(0x73), imm_i32(0x79))    // idx 2
    extcall(y, imm_i32(0x87))                   // idx 3
}

"#,
            vec![
                ImportModuleEntry::new("math".to_owned(), Box::new(ModuleDependency::Runtime)),
                ImportModuleEntry::new("std".to_owned(), Box::new(ModuleDependency::Runtime)),
            ],
            vec![
                ExternalLibraryEntry::new(
                    "foo".to_owned(),
                    Box::new(ExternalLibraryDependency::Runtime),
                ),
                ExternalLibraryEntry::new(
                    "bar".to_owned(),
                    Box::new(ExternalLibraryDependency::Runtime),
                ),
                ExternalLibraryEntry::new(
                    "hello".to_owned(),
                    Box::new(ExternalLibraryDependency::Runtime),
                ),
            ],
        );

        // math, module index = 1
        let entry_math = build_module(
            "math",
            r#"
external fn bar::m(i32, i32)        // internal: lib(0),idx(0)  unified: lib(1),idx(2),type(2)
external fn bar::n(i32)             // internal: lib(0),idx(1)  unified: lib(1),idx(4),type(3)
external fn foo::a(i32,i32)-> i32   // internal: lib(1),idx(2)  unified: lib(0),idx(5),type(0)
external fn world::p()->i32         // internal: lib(2),idx(3)  unified: lib(3),idx(6),type(4)
external fn world::q()              // internal: lib(2),idx(4)  unified: lib(3),idx(7),type(5)

fn do_that() {
    extcall(m, imm_i32(0x23), imm_i32(0x29))    // idx 0
    extcall(n, imm_i32(0x31))                   // idx 1
    extcall(a, imm_i32(0x37), imm_i32(0x41))    // idx 2
    extcall(p)                                  // idx 3
    extcall(q)                                  // idx 4
}
"#,
            vec![],
            vec![
                ExternalLibraryEntry::new(
                    "bar".to_owned(),
                    Box::new(ExternalLibraryDependency::Runtime),
                ),
                ExternalLibraryEntry::new(
                    "foo".to_owned(),
                    Box::new(ExternalLibraryDependency::Runtime),
                ),
                ExternalLibraryEntry::new(
                    "world".to_owned(),
                    Box::new(ExternalLibraryDependency::Runtime),
                ),
            ],
        );

        // std, module index = 2
        let entry_std = build_module(
            "std",
            r#"
external fn foo::a(i32,i32)-> i32   // internal: lib(0),idx(0)  unified: lib(0),idx(5),type(0)
external fn foo::b(i32) -> i32      // internal: lib(0),idx(1)  unified: lib(0),idx(1),type(1)

fn do_this() -> i32 {
    extcall(a, imm_i32(0x11), imm_i32(0x13))    // idx 0
    extcall(b, imm_i32(0x17))                   // idx 1
}
"#,
            vec![],
            vec![ExternalLibraryEntry::new(
                "foo".to_owned(),
                Box::new(ExternalLibraryDependency::Runtime),
            )],
        );

        let (image_common_entries, image_index_entry) =
            build_index(vec![entry_app, entry_math, entry_std]);

        // check module list
        assert_eq!(
            image_index_entry.module_entries,
            vec![
                ImportModuleEntry::new("module".to_owned(), Box::new(ModuleDependency::Current)),
                ImportModuleEntry::new("math".to_owned(), Box::new(ModuleDependency::Runtime)),
                ImportModuleEntry::new("std".to_owned(), Box::new(ModuleDependency::Runtime)),
            ]
        );

        // check unified external library list
        assert_eq!(
            image_index_entry.unified_external_library_entries,
            vec![
                ExternalLibraryEntry::new(
                    "foo".to_owned(),
                    Box::new(ExternalLibraryDependency::Runtime)
                ),
                ExternalLibraryEntry::new(
                    "bar".to_owned(),
                    Box::new(ExternalLibraryDependency::Runtime)
                ),
                ExternalLibraryEntry::new(
                    "hello".to_owned(),
                    Box::new(ExternalLibraryDependency::Runtime)
                ),
                ExternalLibraryEntry::new(
                    "world".to_owned(),
                    Box::new(ExternalLibraryDependency::Runtime)
                ),
            ]
        );

        // check unified external type
        assert_eq!(
            image_index_entry.unified_external_type_entries,
            vec![
                TypeEntry::new(
                    vec![OperandDataType::I32, OperandDataType::I32],
                    vec![OperandDataType::I32]
                ),
                TypeEntry::new(vec![OperandDataType::I32], vec![OperandDataType::I32]),
                TypeEntry::new(vec![OperandDataType::I32, OperandDataType::I32], vec![]),
                TypeEntry::new(vec![OperandDataType::I32], vec![]),
                TypeEntry::new(vec![], vec![OperandDataType::I32]),
                TypeEntry::new(vec![], vec![]),
            ]
        );

        // check unified external function list
        assert_eq!(
            image_index_entry.unified_external_function_entries,
            vec![
                ExternalFunctionEntry::new("x".to_owned(), 2, 0),
                ExternalFunctionEntry::new("b".to_owned(), 0, 1),
                ExternalFunctionEntry::new("m".to_owned(), 1, 2),
                ExternalFunctionEntry::new("y".to_owned(), 2, 3),
                //
                ExternalFunctionEntry::new("n".to_owned(), 1, 3),
                ExternalFunctionEntry::new("a".to_owned(), 0, 0),
                ExternalFunctionEntry::new("p".to_owned(), 3, 4),
                ExternalFunctionEntry::new("q".to_owned(), 3, 5),
            ]
        );

        // check external function index list
        assert_eq!(
            image_index_entry.external_function_index_entries[0].index_entries,
            vec![
                ExternalFunctionIndexEntry::new(0),
                ExternalFunctionIndexEntry::new(1),
                ExternalFunctionIndexEntry::new(2),
                ExternalFunctionIndexEntry::new(3),
            ]
        );

        assert_eq!(
            image_index_entry.external_function_index_entries[1].index_entries,
            vec![
                ExternalFunctionIndexEntry::new(2),
                ExternalFunctionIndexEntry::new(4),
                ExternalFunctionIndexEntry::new(5),
                ExternalFunctionIndexEntry::new(6),
                ExternalFunctionIndexEntry::new(7),
            ]
        );

        assert_eq!(
            image_index_entry.external_function_index_entries[2].index_entries,
            vec![
                ExternalFunctionIndexEntry::new(5),
                ExternalFunctionIndexEntry::new(1),
            ]
        );

        // check bytecodes
        assert_eq!(
            format_bytecode_as_text(&image_common_entries[0].function_entries[0].code),
            "\
0x0000  40 01 00 00  53 00 00 00    imm_i32           0x00000053
0x0008  40 01 00 00  59 00 00 00    imm_i32           0x00000059
0x0010  04 04 00 00  00 00 00 00    extcall           idx:0
0x0018  40 01 00 00  61 00 00 00    imm_i32           0x00000061
0x0020  04 04 00 00  01 00 00 00    extcall           idx:1
0x0028  40 01 00 00  73 00 00 00    imm_i32           0x00000073
0x0030  40 01 00 00  79 00 00 00    imm_i32           0x00000079
0x0038  04 04 00 00  02 00 00 00    extcall           idx:2
0x0040  40 01 00 00  87 00 00 00    imm_i32           0x00000087
0x0048  04 04 00 00  03 00 00 00    extcall           idx:3
0x0050  c0 03                       end"
        );

        assert_eq!(
            format_bytecode_as_text(&image_common_entries[1].function_entries[0].code),
            "\
0x0000  40 01 00 00  23 00 00 00    imm_i32           0x00000023
0x0008  40 01 00 00  29 00 00 00    imm_i32           0x00000029
0x0010  04 04 00 00  00 00 00 00    extcall           idx:0
0x0018  40 01 00 00  31 00 00 00    imm_i32           0x00000031
0x0020  04 04 00 00  01 00 00 00    extcall           idx:1
0x0028  40 01 00 00  37 00 00 00    imm_i32           0x00000037
0x0030  40 01 00 00  41 00 00 00    imm_i32           0x00000041
0x0038  04 04 00 00  02 00 00 00    extcall           idx:2
0x0040  04 04 00 00  03 00 00 00    extcall           idx:3
0x0048  04 04 00 00  04 00 00 00    extcall           idx:4
0x0050  c0 03                       end"
        );

        assert_eq!(
            format_bytecode_as_text(&image_common_entries[2].function_entries[0].code),
            "\
0x0000  40 01 00 00  11 00 00 00    imm_i32           0x00000011
0x0008  40 01 00 00  13 00 00 00    imm_i32           0x00000013
0x0010  04 04 00 00  00 00 00 00    extcall           idx:0
0x0018  40 01 00 00  17 00 00 00    imm_i32           0x00000017
0x0020  04 04 00 00  01 00 00 00    extcall           idx:1
0x0028  c0 03                       end"
        );
    }
}
