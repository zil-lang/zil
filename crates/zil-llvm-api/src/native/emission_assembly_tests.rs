use super::*;

fn aliases() -> BTreeMap<String, String> {
    BTreeMap::from([("zil.fn.2".into(), "zil.fn.1".into())])
}
fn order(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).into()).collect()
}
fn leaf(name: &str, number: usize) -> String {
    format!(
        "\t.globl\t{name}\t# -- Begin function {name}\n\t.p2align\t4, 0x90\n\t.type\t{name},@function\n{name}:\n\ttestl\t%edi, %edi\n\tje\t.LBB{number}_1\n\tleal\t7(%rdi), %eax\n\tretq\n.LBB{number}_1:\n\txorl\t%eax, %eax\n\tretq\n.Lfunc_end{number}:\n\t.size\t{name}, .Lfunc_end{number}-{name}\n"
    )
}
fn input() -> String {
    format!(
        "\t.text\n\t.file\t\"current\"\n{}\t.section\t\".note.GNU-stack\",\"\",@progbits\n\t.addrsig\n",
        leaf("zil.fn.1", 0)
    )
}

#[test]
fn distinct_function_and_local_labels_are_reconstructed_in_definition_order() {
    let source = input();
    let a = BTreeMap::from([
        ("zil.fn.2".into(), "zil.fn.1".into()),
        ("zil.fn.10".into(), "zil.fn.1".into()),
    ]);
    let names = order(&["zil.fn.1", "zil.fn.2", "zil.fn.10"]);
    let out = String::from_utf8(reconstruct(source.as_bytes(), &a, &names).unwrap()).unwrap();
    assert!(out.contains("zil.fn.2:\n"));
    assert!(out.contains("zil.fn.10:\n"));
    assert!(out.find("zil.fn.1:\n") < out.find("zil.fn.2:\n"));
    assert!(out.find("zil.fn.2:\n") < out.find("zil.fn.10:\n"));
    assert!(out.contains("\tje\t.Lzil_assembly_clone_1_0\n"));
    assert!(out.contains(".Lzil_assembly_clone_1_0:\n"));
    assert!(out.contains("\t.size\tzil.fn.2, .Lzil_assembly_clone_1_1-zil.fn.2\n"));
    assert!(out.contains(".Lzil_assembly_clone_2_0:\n"));
    assert_eq!(out.matches("\t.file\t").count(), 0);
    assert_eq!(out.matches("\tleal\t7(%rdi), %eax\n").count(), 3);
    assert!(!out.contains(".set"));
    assert_eq!(out.matches(".LBB0_1:\n").count(), 1);
    assert!(out.ends_with("\t.addrsig\n"));
}

#[test]
fn normal_cfi_functions_globals_and_directives_remain_byte_exact_and_ordered() {
    let ordinary = "\t.globl\tordinary\n\t.type\tordinary,@function\nordinary:\n\t.cfi_startproc\n\tcallq\texternal@PLT\n\tretq\n.Lfunc_end9:\n\t.size\tordinary, .Lfunc_end9-ordinary\n\t.cfi_endproc\n";
    let global = "\t.data\n\t.type\tglobal_data,@object\nglobal_data:\n\t.quad\tordinary\n\t.size\tglobal_data, 8\n\t.text\n";
    let source = format!(
        "\t.text\n{}{}{}\t.ident\t\"LLVM\"\n",
        leaf("zil.fn.1", 0),
        global,
        ordinary
    );
    let out = String::from_utf8(
        reconstruct(
            source.as_bytes(),
            &aliases(),
            &order(&["zil.fn.1", "ordinary", "zil.fn.2"]),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(out.contains(global));
    assert!(out.contains(ordinary));
    assert!(out.find(global) < out.find(ordinary));
    assert!(out.find("\t.cfi_endproc\n") < out.find("zil.fn.2:\n"));
    assert!(out.ends_with("\t.ident\t\"LLVM\"\n"));
}

#[test]
fn clones_are_inserted_before_later_globals_change_the_active_section() {
    let suffix = "\t.section\t.rodata\n\t.type\tconstant,@object\nconstant:\n\t.long\t17\n\t.size\tconstant,4\n";
    let source = format!("\t.text\n{}{suffix}", leaf("zil.fn.1", 0));
    let out = String::from_utf8(
        reconstruct(
            source.as_bytes(),
            &aliases(),
            &order(&["zil.fn.1", "zil.fn.2"]),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(out.find("zil.fn.2:\n") < out.find("\t.section\t.rodata\n"));
    assert!(out.ends_with(suffix));
}

#[test]
fn cfi_eh_sections_global_relocations_calls_and_unknown_shapes_decline() {
    let source = input();
    for bad in [
        "\t.cfi_startproc\n\t.cfi_endproc\n",
        "\t.cfi_def_cfa_offset 16\n",
        "\t.seh_endprologue\n",
        "\t.section .rodata\n",
        "\t.pushsection .text\n\t.popsection\n",
        "\t.quad external\n",
        "\tmovq external(%rip), %rax\n",
        "\tmovq .LCPI0_0(%rip), %rax\n",
        "\tleaq .LBB0_1(%rip), %rax\n",
        "\tmovq $external, %rax\n",
        "\tmovq $zil.fn.10, %rax\n",
        "\tcallq external\n",
        "\tcallq llvm.trap\n",
        "\tcallq *%rax\n",
        "\tjmp *%rax\n",
        "\tjmp .LBB99_1\n",
        "\tmovl (%rdi), %eax\n",
        "\tmovq %fs:0, %rax\n",
        "\t.byte 0xc3\n",
        "\t.file 2 \"extra\"\n",
        "\t.loc 1 2 3\n",
        "\tunknown_opcode %rax\n",
        "\tnop; callq external\n",
    ] {
        let changed = source.replacen("\ttestl\t%edi, %edi\n", bad, 1);
        assert!(
            reconstruct(
                changed.as_bytes(),
                &aliases(),
                &order(&["zil.fn.1", "zil.fn.2"])
            )
            .is_none(),
            "accepted {bad:?}"
        );
    }
    let changed = source.replacen(
        "\t.text\n",
        "\t.section .text.special,\"ax\",@progbits\n",
        1,
    );
    assert!(
        reconstruct(
            changed.as_bytes(),
            &aliases(),
            &order(&["zil.fn.1", "zil.fn.2"])
        )
        .is_none()
    );
}

#[test]
fn exact_token_substitution_does_not_rename_comments_registers_or_prefixes() {
    let source = input().replace(
        "\ttestl\t%edi, %edi",
        "# zil.fn.1 zil.fn.10 .LBB0_10\n\ttestl\t%edi, %edi",
    );
    let out = String::from_utf8(
        reconstruct(
            source.as_bytes(),
            &aliases(),
            &order(&["zil.fn.1", "zil.fn.2"]),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(out.matches("# zil.fn.1 zil.fn.10 .LBB0_10\n").count(), 2);
    let source = format!("\t.text\n{}", leaf("eax", 0));
    let a = BTreeMap::from([("copy".into(), "eax".into())]);
    let out =
        String::from_utf8(reconstruct(source.as_bytes(), &a, &order(&["eax", "copy"])).unwrap())
            .unwrap();
    assert_eq!(out.matches("%eax").count(), 4);
    assert!(out.contains("copy:\n"));
    assert!(!out.contains("%copy"));
}

#[test]
fn missing_duplicate_reordered_or_cyclic_bindings_decline_atomically() {
    let source = input();
    for names in [
        vec!["zil.fn.1"],
        vec!["zil.fn.1", "zil.fn.2", "zil.fn.2"],
        vec!["zil.fn.1", "zil.fn.2", "missing"],
    ] {
        assert!(reconstruct(source.as_bytes(), &aliases(), &order(&names)).is_none());
    }
    for a in [
        BTreeMap::from([("zil.fn.2".into(), "missing".into())]),
        BTreeMap::from([("zil.fn.2".into(), "zil.fn.2".into())]),
        BTreeMap::from([
            ("zil.fn.1".into(), "zil.fn.2".into()),
            ("zil.fn.2".into(), "zil.fn.1".into()),
        ]),
    ] {
        assert!(reconstruct(source.as_bytes(), &a, &order(&["zil.fn.1", "zil.fn.2"])).is_none());
    }
    let emitted_clone = format!("\t.text\n{}{}", leaf("zil.fn.1", 0), leaf("zil.fn.2", 1));
    assert!(
        reconstruct(
            emitted_clone.as_bytes(),
            &aliases(),
            &order(&["zil.fn.1", "zil.fn.2"])
        )
        .is_none()
    );
    let reordered = format!("\t.text\n{}{}", leaf("other", 1), leaf("zil.fn.1", 0));
    assert!(
        reconstruct(
            reordered.as_bytes(),
            &aliases(),
            &order(&["zil.fn.1", "other", "zil.fn.2"])
        )
        .is_none()
    );
}

#[test]
fn local_collisions_ambiguous_assembly_and_truncation_decline() {
    for source in [
        input().replace(".LBB0_1:\n", ".LBB0_1:\n.LBB0_1:\n"),
        input().replace(".Lfunc_end0-zil.fn.1", ".Lfunc_end0-zil.fn.10"),
        input().replace(".LBB0_1:", ".LJTI0_0:"),
        input().replace(".LBB0_1:", ".Lzil_assembly_clone_1_0:"),
        input().replace("\t.text\n", "\t.text\n\t.macro retq\n\t.endm\n"),
        input().replace("\t.text\n", "\t.text\n\t.set zil.fn.2,zil.fn.1\n"),
        input().replace("\t.text\n", "\t.text\n\t.intel_syntax noprefix\n"),
        input().replace("\t.size\tzil.fn.1, .Lfunc_end0-zil.fn.1\n", ""),
        input().trim_end().to_string(),
    ] {
        assert!(
            reconstruct(
                source.as_bytes(),
                &aliases(),
                &order(&["zil.fn.1", "zil.fn.2"])
            )
            .is_none(),
            "accepted {source}"
        );
    }
}

#[test]
fn no_aliases_preserve_opaque_assembly_and_stack_leaf_is_supported() {
    let opaque = b"\t.text\n# ordinary CFI is not a clone permission\n";
    assert_eq!(reconstruct(opaque, &BTreeMap::new(), &[]).unwrap(), opaque);
    let source = input().replace("\tleal\t7(%rdi), %eax", "\tpushq %rbp\n\tmovq %rsp, %rbp\n\tmovl %edi, -4(%rbp)\n\tmovl -4(%rbp), %eax\n\tpopq %rbp");
    assert!(
        reconstruct(
            source.as_bytes(),
            &aliases(),
            &order(&["zil.fn.1", "zil.fn.2"])
        )
        .is_some()
    );
}

#[test]
fn llvm23_actual_prefalign_shape_reconstructs_three_bodies_in_twelve_definitions() {
    // Shape transcribed from emission-asm-example.s, emitted by LLVM23 C API.
    let mut source = String::from("\t.att_syntax\n\t.file\t\"zil-unit\"\n\t.text\n");
    for n in 0..3 {
        let body = if n == 0 {
            "movl\t%edi, %eax".to_string()
        } else {
            format!("leal\t{n}(%rdi), %eax")
        };
        source.push_str(&format!("\t.globl\tzil.fn.{n}\n\t.prefalign\t4, .Lfunc_end{n}, nop\n\t.type\tzil.fn.{n},@function\nzil.fn.{n}:\n\t{body}\n\tretq\n.Lfunc_end{n}:\n\t.size\tzil.fn.{n}, .Lfunc_end{n}-zil.fn.{n}\n\n"));
    }
    let suffix = "\t.section\t\".note.GNU-stack\",\"\",@progbits\n";
    source.push_str(suffix);
    let aliases = (3..12)
        .map(|n| (format!("zil.fn.{n}"), format!("zil.fn.{}", n % 3)))
        .collect();
    let names = (0..12).map(|n| format!("zil.fn.{n}")).collect::<Vec<_>>();
    let out = String::from_utf8(reconstruct(source.as_bytes(), &aliases, &names).unwrap()).unwrap();
    assert!(out.starts_with("\t.att_syntax\n\t.text\n"));
    assert!(!out.contains(".file"));
    assert_eq!(out.matches("\t.prefalign\t").count(), 12);
    assert_eq!(out.matches("\tretq\n").count(), 12);
    let mut last = 0;
    for (n, name) in names.iter().enumerate() {
        let position = out.find(&format!("\n{name}:\n")).unwrap();
        assert!(position > last);
        last = position;
        if n >= 3 {
            assert!(out.contains(&format!(
                "\t.prefalign\t4, .Lzil_assembly_clone_{n}_0, nop\n"
            )));
            assert!(out.contains(&format!(
                "\t.size\t{name}, .Lzil_assembly_clone_{n}_0-{name}\n"
            )));
        }
    }
    assert!(out.ends_with(suffix));
    for bad in [
        "4, external, nop",
        "4, .Lfunc_end0, trap",
        "4, .Lfunc_end0, nop, 8",
    ] {
        let bad_source = source.replacen("4, .Lfunc_end0, nop", bad, 1);
        assert!(reconstruct(bad_source.as_bytes(), &aliases, &names).is_none());
    }
}

#[test]
fn file_record_is_removed_once_debug_files_and_type_marker_are_preserved() {
    let source = format!(
        "\t.file\t\"source#name\"\n\t.file 1 \"debug\"\n\t.text\n{}",
        leaf("function", 0)
    );
    let a = BTreeMap::from([("copy".into(), "function".into())]);
    let names = order(&["function", "copy"]);
    let out = String::from_utf8(reconstruct(source.as_bytes(), &a, &names).unwrap()).unwrap();
    assert!(!out.contains("source#name"));
    assert!(out.contains("\t.file 1 \"debug\"\n"));
    assert!(out.contains("\t.type\tcopy,@function\n"));
    assert!(!out.contains("@copy"));
    for bad in [
        format!("\t.file \"second\"\n{source}"),
        format!("{source}\t.file \"late\"\n"),
    ] {
        assert!(reconstruct(bad.as_bytes(), &a, &names).is_none());
    }
}
