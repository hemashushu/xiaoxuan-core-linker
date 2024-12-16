// Copyright (c) 2024 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions,
// more details in file LICENSE, LICENSE.additional and CONTRIBUTING.

use anc_assembler::entry::ImageCommonEntry;
use anc_image::module_image::ModuleImage;

struct ModuleMaterial {
    import_module_names: Vec<String>,
    import_function_fullnames: Vec<String>,
    internal_function_fullnames: Vec<String>, // starts with "module::"
}

/// note that only submodules under the same module can be merged.
pub fn merge_modules(submodule_entries: &[ImageCommonEntry]) {
    let a = submodule_entries
        .iter()
        .map(|item| &item.import_module_entries)
        .collect::<Vec<_>>();

    todo!()
}

#[cfg(test)]
mod tests {
    use anc_assembler::{assembler::assemble_module_node, entry::ImageCommonEntry};
    use anc_image::{
        entry::{ExternalLibraryEntry, ImportModuleEntry},
        module_image::ModuleImage,
    };
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
