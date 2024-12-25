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
    merger::{
        merge_data_entries, merge_external_function_entries, merge_external_library_entries,
        merge_function_entries, merge_import_data_entries, merge_import_function_entries,
        merge_import_module_entries, merge_local_variable_list_entries, merge_type_entries,
    },
    LinkErrorType, LinkerError,
};

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

/// note that not only submodules under the same module can be merged.
pub fn link_modules(
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
    use pretty_assertions::assert_eq;

    use anc_assembler::{
        assembler::{assemble_module_node, create_self_dependent_import_module_entry},
        entry::ImageCommonEntry,
    };
    use anc_image::{
        bytecode_reader::format_bytecode_as_text,
        entry::{
            DataNameEntry, ExternalFunctionEntry, ExternalLibraryEntry, FunctionNameEntry,
            ImportModuleEntry, InitedDataEntry, LocalVariableEntry, LocalVariableListEntry,
            RelocateEntry, RelocateListEntry, TypeEntry, UninitDataEntry,
        },
        module_image::RelocateType,
    };
    use anc_isa::{DependencyShare, ExternalLibraryDependency, ModuleDependency, OperandDataType};
    use anc_parser_asm::parser::parse_from_str;

    use crate::{
        linker::link_modules, merger::merge_import_module_entries, LinkErrorType, LinkerError,
    };

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
    fn test_merge_type_and_local_variable_list_entries() {
        let module0 = SubModule {
            fullname: "hello",
            source: r#"
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
        };

        let module1 = SubModule {
            fullname: "hello::world",
            source: r#"
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
        };

        let submodules = vec![module0, module1];
        let submodule_entries = assemble_submodules(&submodules, &[], &[]);
        let merged_entry = link_modules("merged", false, &submodule_entries).unwrap();

        // type
        assert_eq!(
            merged_entry.type_entries,
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
            merged_entry.local_variable_list_entries,
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
        let func0 = &merged_entry.function_entries[0];
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

        let func1 = &merged_entry.function_entries[1];
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
            merged_entry.relocate_list_entries,
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
        let module0 = SubModule {
            fullname: "hello",
            source: r#"
import data module::middle::d0 type i32
import data module::middle::d2 type i32
import data module::middle::d4 type i32
import data module::base::d1 type i32
import data module::base::d3 type i32
import data module::base::d5 type i32

fn main() {
    data_load_i32_s(d0)
    data_load_i32_s(d1)
    data_load_i32_s(d2)
    data_load_i32_s(d3)
    data_load_i32_s(d4)
    data_load_i32_s(d5)
}"#,
        };

        let module1 = SubModule {
            fullname: "hello::middle",
            source: r#"
import data module::base::d5 type i32
import data module::base::d1 type i32
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
        };

        let module2 = SubModule {
            fullname: "hello::base",
            source: r#"
data d3:i32 = 0x19
uninit data d5:i32
readonly data d1:i32 = 0x17

fn bar() {
    data_load_i32_s(d1)
    data_load_i32_s(d3)
    data_load_i32_s(d5)
}"#,
        };

        let submodules = vec![module0, module1, module2];
        let submodule_entries = assemble_submodules(&submodules, &[], &[]);
        let merged_entry = link_modules("merged", false, &submodule_entries).unwrap();

        // import modules
        assert_eq!(
            merged_entry.import_module_entries,
            vec![create_self_dependent_import_module_entry()]
        );

        // import data
        assert!(merged_entry.import_data_entries.is_empty());

        // functions
        assert_eq!(
            format_bytecode_as_text(&merged_entry.function_entries[0].code),
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
            format_bytecode_as_text(&merged_entry.function_entries[1].code),
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
            format_bytecode_as_text(&merged_entry.function_entries[2].code),
            "\
0x0000  c1 01 00 00  01 00 00 00    data_load_i32_s   off:0x00  idx:1
0x0008  c1 01 00 00  03 00 00 00    data_load_i32_s   off:0x00  idx:3
0x0010  c1 01 00 00  05 00 00 00    data_load_i32_s   off:0x00  idx:5
0x0018  c0 03                       end"
        );

        // .rodata
        assert_eq!(
            merged_entry.read_only_data_entries,
            vec![
                InitedDataEntry::from_i32(0x11),
                InitedDataEntry::from_i32(0x17)
            ]
        );

        // .data
        assert_eq!(
            merged_entry.read_write_data_entries,
            vec![
                InitedDataEntry::from_i32(0x13),
                InitedDataEntry::from_i32(0x19)
            ]
        );

        // .bss
        assert_eq!(
            merged_entry.uninit_data_entries,
            vec![UninitDataEntry::from_i32(), UninitDataEntry::from_i32(),]
        );

        // data name
        assert_eq!(
            merged_entry.data_name_entries,
            vec![
                DataNameEntry::new("hello::middle::d0".to_owned(), false),
                DataNameEntry::new("hello::base::d1".to_owned(), false),
                DataNameEntry::new("hello::middle::d2".to_owned(), false),
                DataNameEntry::new("hello::base::d3".to_owned(), false),
                DataNameEntry::new("hello::middle::d4".to_owned(), false),
                DataNameEntry::new("hello::base::d5".to_owned(), false),
            ]
        );

        // relocate
        assert_eq!(
            merged_entry.relocate_list_entries,
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
        let module0 = SubModule {
            fullname: "hello",
            source: r#"
external fn abc::do_something()
external fn def::do_this(i32) -> i32

fn main() {
    extcall(do_something)
    extcall(do_this, imm_i32(0x11))
}
"#,
        };

        let module1 = SubModule {
            fullname: "hello::world",
            source: r#"
external fn def::do_that(i32,i32) -> i32
external fn abc::do_something()

fn foo(n:i32)->i32 {
    extcall(do_something)
}

fn bar() -> i32 {
    extcall(do_that, imm_i32(0x13), imm_i32(0x17))
}
"#,
        };

        let libabc = ExternalLibraryEntry::new(
            "abc".to_owned(),
            Box::new(ExternalLibraryDependency::Runtime),
        );
        let libdef = ExternalLibraryEntry::new(
            "def".to_owned(),
            Box::new(ExternalLibraryDependency::Runtime),
        );

        let submodules = vec![module0, module1];
        let submodule_entries =
            assemble_submodules(&submodules, &[], &[libabc.clone(), libdef.clone()]);
        let merged_entry = link_modules("merged", false, &submodule_entries).unwrap();

        // types
        assert_eq!(
            merged_entry.type_entries,
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
        let func0 = &merged_entry.function_entries[0];
        assert_eq!(func0.type_index, 0);
        assert_eq!(
            format_bytecode_as_text(&func0.code),
            "\
0x0000  04 04 00 00  00 00 00 00    extcall           idx:0
0x0008  40 01 00 00  11 00 00 00    imm_i32           0x00000011
0x0010  04 04 00 00  01 00 00 00    extcall           idx:1
0x0018  c0 03                       end"
        );

        let func1 = &merged_entry.function_entries[1];
        assert_eq!(func1.type_index, 1);
        assert_eq!(
            format_bytecode_as_text(&func1.code),
            "\
0x0000  04 04 00 00  00 00 00 00    extcall           idx:0
0x0008  c0 03                       end"
        );

        let func2 = &merged_entry.function_entries[2];
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
            merged_entry.relocate_list_entries,
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
        assert_eq!(merged_entry.external_library_entries, vec![libabc, libdef]);

        // external functions
        assert_eq!(
            merged_entry.external_function_entries,
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
        let module0 = SubModule {
            fullname: "hello",
            source: r#"
import fn module::base::add(i32,i32)->i32
import fn module::middle::muladd(i32,i32,i32)->i32

fn main()->i32 {
    call(muladd, imm_i32(0x11), imm_i32(0x13), imm_i32(0x17))
    call(add, imm_i32(0x23), imm_i32(0x29))
}"#,
        };

        let module1 = SubModule {
            fullname: "hello::middle",
            source: r#"
import fn module::base::add(i32,i32)->i32

fn muladd(left:i32, right:i32, factor:i32)->i32 {
    mul_i32(
        call(add,
            local_load_i32_s(left),
            local_load_i32_s(right)),
        local_load_i32_s(factor))
}"#,
        };

        let module2 = SubModule {
            fullname: "hello::base",
            source: r#"
fn add(left:i32, right:i32)->i32 {
    add_i32(
        local_load_i32_s(left),
        local_load_i32_s(right))
}"#,
        };

        let submodules = vec![module0, module1, module2];
        let submodule_entries = assemble_submodules(&submodules, &[], &[]);
        let merged_entry = link_modules("merged", false, &submodule_entries).unwrap();

        // import modules
        assert_eq!(
            merged_entry.import_module_entries,
            vec![create_self_dependent_import_module_entry()]
        );

        // import functions
        assert!(merged_entry.import_function_entries.is_empty());

        // types
        assert_eq!(
            merged_entry.type_entries,
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
            merged_entry.local_variable_list_entries,
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
        let func0 = &merged_entry.function_entries[0];
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
        let func1 = &merged_entry.function_entries[1];
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
        let func2 = &merged_entry.function_entries[2];
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
        assert_eq!(merged_entry.read_only_data_entries, vec![]);
        assert_eq!(merged_entry.read_write_data_entries, vec![]);
        assert_eq!(merged_entry.uninit_data_entries, vec![]);

        // function names
        assert_eq!(
            merged_entry.function_name_entries,
            vec![
                FunctionNameEntry::new("hello::main".to_owned(), false),
                FunctionNameEntry::new("hello::middle::muladd".to_owned(), false),
                FunctionNameEntry::new("hello::base::add".to_owned(), false),
            ]
        );

        // relocate list
        assert_eq!(
            merged_entry.relocate_list_entries,
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
    fn test_link_with_unresolved_function() {
        let module0 = SubModule {
            fullname: "hello",
            source: r#"
import fn module::world::do_this()
import fn module::world::do_that()

fn main()->i32 {
    nop()
}"#,
        };

        let module1 = SubModule {
            fullname: "hello::world",
            source: r#"
fn do_this() {
    nop()
}"#,
        };

        let submodules = vec![module0, module1];
        let submodule_entries = assemble_submodules(&submodules, &[], &[]);
        let merged_result = link_modules("merged", true, &submodule_entries);

        assert!(matches!(
            merged_result,
            Err(LinkerError {
                error_type: LinkErrorType::UnresolvedFunctionName(text)
            }) if text == "hello::world::do_that"
        ));
    }

    #[test]
    fn test_link_with_unresolved_data() {
        let module0 = SubModule {
            fullname: "hello",
            source: r#"
import data module::world::d0 type i32
import data module::world::d1 type i32

fn main()->i32 {
    nop()
}"#,
        };

        let module1 = SubModule {
            fullname: "hello::world",
            source: r#"
data d0:i32 = 0x11
"#,
        };

        let submodules = vec![module0, module1];
        let submodule_entries = assemble_submodules(&submodules, &[], &[]);
        let merged_result = link_modules("merged", true, &submodule_entries);

        assert!(matches!(
            merged_result,
            Err(LinkerError {
                error_type: LinkErrorType::UnresolvedDataName(text)
            }) if text == "hello::world::d1"
        ));
    }
}
