/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::FileKeyInner;
use flow_parser::parse_program_without_file;
use flow_parser_utils::file_sig::FileSig;
use flow_parser_utils::file_sig::FileSigOptions;
use pretty_assertions::assert_eq;

use crate::type_sig_options::TypeSigOptions;
use crate::type_sig_utils;

fn dedent_trim(s: &str) -> String {
    let lines: Vec<&str> = s.lines().filter(|line| !line.trim().is_empty()).collect();

    if lines.is_empty() {
        return String::new();
    }

    let min_indent = lines
        .iter()
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|line| {
            if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn default_sig_options() -> TypeSigOptions {
    TypeSigOptions {
        munge: false,
        facebook_key_mirror: false,
        facebook_fbt: None,
        enable_custom_error: false,
        enable_enums: true,
        enable_component_syntax: true,
        component_syntax_enabled_in_config: true,
        enable_ts_syntax: true,
        hook_compatibility: true,
        enable_records: true,
        enable_relay_integration: false,
        relay_integration_module_prefix: None,
        for_builtins: false,
        locs_to_dirtify: vec![],
        is_ts_file: false,
        is_dts_file: false,
        tslib_syntax: false,
        is_lib_file: false,
    }
}

fn print_sig(contents: &str) -> String {
    print_sig_with_options(contents, None::<fn(&mut TypeSigOptions)>)
}

fn print_sig_with_parse_and_sig_options<F>(
    contents: &str,
    parse_options: Option<flow_parser::ParseOptions>,
    sig_configurator: Option<F>,
) -> String
where
    F: FnOnce(&mut TypeSigOptions),
{
    let contents = dedent_trim(contents);
    let (ast, _errors) = parse_program_without_file(false, None, parse_options, Ok(&contents));
    let mut opts = default_sig_options();

    if let Some(config) = sig_configurator {
        config(&mut opts);
    }

    let arena = bumpalo::Bump::new();
    let (errors, locs, module) =
        type_sig_utils::parse_and_pack_module(&opts, &arena, true, None, None, &ast);
    let mut loc_string = String::new();
    for (i, loc) in locs.iter().enumerate() {
        loc_string.push_str(&i.to_string());
        loc_string.push_str(". ");
        if loc.start == loc.end {
            loc_string.push_str(&format!("[{}:{}]", loc.start.line, loc.start.column));
        } else if loc.start.line == loc.end.line {
            loc_string.push_str(&format!(
                "[{}:{}-{}]",
                loc.start.line, loc.start.column, loc.end.column
            ));
        } else {
            loc_string.push_str(&format!(
                "[{}:{}-{}:{}]",
                loc.start.line, loc.start.column, loc.end.line, loc.end.column
            ));
        }
        loc_string.push('\n');
    }

    let mut output = format!("Locs:\n{}Type Sig:\n{:#?}", loc_string, module);

    if !errors.is_empty() {
        output.push_str("\n\nErrors:\n");
        for err in errors.iter() {
            output.push_str(&format!("{:?}\n", err));
        }
    }

    output
}

fn print_sig_with_options<F>(contents: &str, configurator: Option<F>) -> String
where
    F: FnOnce(&mut TypeSigOptions),
{
    print_sig_with_parse_and_sig_options(contents, None, configurator)
}

fn print_builtins(contents_list: Vec<&str>) -> String {
    print_builtins_with_global_augmentations(contents_list, vec![])
}

fn print_builtins_with_global_augmentations(
    contents_list: Vec<&str>,
    global_augmentation_contents_list: Vec<&str>,
) -> String {
    let mut asts = vec![];
    for contents in &contents_list {
        let contents_str = dedent_trim(contents);
        let (ast, _errors) = parse_program_without_file(false, None, None, Ok(&contents_str));
        asts.push(ast);
    }
    let mut global_augmentation_asts = vec![];
    for contents in &global_augmentation_contents_list {
        let contents_str = dedent_trim(contents);
        let (ast, _errors) = parse_program_without_file(false, None, None, Ok(&contents_str));
        global_augmentation_asts.push(ast);
    }
    let opts = TypeSigOptions {
        for_builtins: true,
        ..default_sig_options()
    };
    let arena = bumpalo::Bump::new();
    let ast_refs: Vec<_> = asts
        .iter()
        .map(|ast| (ast, flow_common::type_strictness::TypeStrictnessKind::Flow))
        .collect();
    let file_key = FileKey::new(FileKeyInner::SourceFile("test.ts".to_owned()));
    let file_sigs: Vec<_> = global_augmentation_asts
        .iter()
        .map(|ast| FileSig::from_program(&file_key, ast, &FileSigOptions::default()))
        .collect();
    let global_augmentation_ast_refs: Vec<_> = global_augmentation_asts
        .iter()
        .zip(&file_sigs)
        .map(|(ast, file_sig)| {
            (
                ast,
                flow_common::type_strictness::TypeStrictnessKind::from_is_typescript(true),
                file_sig,
            )
        })
        .collect();
    let (errors, locs, builtins) = type_sig_utils::parse_and_pack_builtins(
        &opts,
        &arena,
        &ast_refs,
        &global_augmentation_ast_refs,
    );

    let mut loc_string = String::new();
    for (i, loc) in locs.iter().enumerate() {
        loc_string.push_str(&i.to_string());
        loc_string.push_str(". ");
        if loc.start == loc.end {
            loc_string.push_str(&format!("[{}:{}]", loc.start.line, loc.start.column));
        } else if loc.start.line == loc.end.line {
            loc_string.push_str(&format!(
                "[{}:{}-{}]",
                loc.start.line, loc.start.column, loc.end.column
            ));
        } else {
            loc_string.push_str(&format!(
                "[{}:{}-{}:{}]",
                loc.start.line, loc.start.column, loc.end.line, loc.end.column
            ));
        }
        loc_string.push('\n');
    }

    let mut output = format!("Locs:\n{}", loc_string);

    if !builtins.module_refs.is_empty() {
        output.push_str("\nModule refs:\n");
        for (i, module_ref) in builtins.module_refs.iter().enumerate() {
            output.push_str(&format!("{}. {:?}\n", i, module_ref));
        }
    }
    if !builtins.local_defs.is_empty() {
        output.push_str("\nLocal defs:\n");
        for (i, def) in builtins.local_defs.iter().enumerate() {
            output.push_str(&format!("{}. {:#?}\n", i, def));
        }
    }
    if !builtins.remote_refs.is_empty() {
        output.push_str("\nRemote refs:\n");
        for (i, remote_ref) in builtins.remote_refs.iter().enumerate() {
            output.push_str(&format!("{}. {:?}\n", i, remote_ref));
        }
    }
    if !builtins.pattern_defs.is_empty() {
        output.push_str("\nPattern defs:\n");
        for (i, pattern_def) in builtins.pattern_defs.iter().enumerate() {
            output.push_str(&format!("{}. {:?}\n", i, pattern_def));
        }
    }
    if !builtins.patterns.is_empty() {
        output.push_str("\nPatterns:\n");
        for (i, pattern) in builtins.patterns.iter().enumerate() {
            output.push_str(&format!("{}. {:?}\n", i, pattern));
        }
    }

    // Global values
    for name in builtins.global_values.keys() {
        output.push_str(&format!("\nBuiltin global value {}\n", name));
    }

    // Global types
    for name in builtins.global_types.keys() {
        output.push_str(&format!("\nBuiltin global type {}\n", name));
    }

    // Global modules
    for (name, module_def) in &builtins.global_modules {
        output.push_str(&format!("\nBuiltin module {}:\n", name));
        output.push_str(&format!("Loc: {:?}\n", module_def.loc));
        output.push_str(&format!("{:#?}\n", module_def.module_kind));
    }

    // Global UMD modules
    for (name, module_def) in &builtins.global_umd_modules {
        output.push_str(&format!("\nBuiltin UMD global {}:\n", name));
        output.push_str(&format!("Loc: {:?}\n", module_def.loc));
        output.push_str(&format!("{:#?}\n", module_def.module_kind));
    }

    if !errors.is_empty() {
        output.push_str("\nErrors:\n");
        for err in errors.iter() {
            output.push_str(&format!("{:?}\n", err));
        }
    }

    output
}

#[test]
fn export_number_literal() {
    let input = r#"
export default 0;
"#;
    let expected_output = r#"
Locs:
0. [1:7-14]
1. [1:15-16]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 0,
                    def: Value(
                        NumberLit(
                            (
                                1,
                                0.0,
                                "0",
                            ),
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_options(
            input,
            Some(|opts: &mut TypeSigOptions| {
                opts.facebook_fbt = Some(FlowSmolStr::new("FbtElement"));
            })
        ))
    )
}

#[test]
fn export_bigint_literal() {
    let input = r#"
export default 0n;
"#;
    let expected_output = r#"
Locs:
0. [1:7-14]
1. [1:15-17]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 0,
                    def: Value(
                        BigIntLit(
                            (
                                1,
                                Some(
                                    0,
                                ),
                                "0n",
                            ),
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_options(
            input,
            Some(|opts: &mut TypeSigOptions| {
                opts.munge = false; // Explicit: underscore-prefixed members are NOT filtered
            })
        ))
    )
}

#[test]
fn export_bigint_literal_neg() {
    let input = r#"
            export default -0n;
        "#;
    let expected_output = r#"
Locs:
0. [1:7-14]
1. [1:15-18]
2. [1:16-18]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 0,
                    def: Eval(
                        PackedEval {
                            loc: 1,
                            packed: Value(
                                BigIntLit(
                                    (
                                        2,
                                        Some(
                                            0,
                                        ),
                                        "0n",
                                    ),
                                ),
                            ),
                            op: Unary(
                                Minus,
                            ),
                            strictness_kind: Flow,
                        },
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_function_literal() {
    let input = r#"
            export default function(x: number): number { return x };
        "#;
    let expected_output = r#"
Locs:
0. [1:7-14]
1. [1:15-55]
2. [1:27-33]
3. [1:36-42]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 0,
                    def: Value(
                        FunExpr(
                            ValueFunExpr {
                                loc: 1,
                                async_: false,
                                generator: false,
                                def: FunSig {
                                    tparams: Mono,
                                    params: [
                                        FunParam {
                                            name: Some(
                                                "x",
                                            ),
                                            t: Annot(
                                                Number(
                                                    2,
                                                ),
                                            ),
                                        },
                                    ],
                                    rest_param: None,
                                    this_param: None,
                                    return_: Annot(
                                        Number(
                                            3,
                                        ),
                                    ),
                                    type_guard: None,
                                    effect_: ArbitraryEffect,
                                    strictness_kind: Flow,
                                },
                                statics: {},
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_function_literal_check1() {
    let input = r#"
export default function(x): number { return x };
"#;
    let expected_output = r#"
Locs:
0. [1:7-14]
1. [1:15-47]
2. [1:24-25]
3. [1:28-34]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 0,
                    def: Value(
                        FunExpr(
                            ValueFunExpr {
                                loc: 1,
                                async_: false,
                                generator: false,
                                def: FunSig {
                                    tparams: Mono,
                                    params: [
                                        FunParam {
                                            name: Some(
                                                "x",
                                            ),
                                            t: Err(
                                                2,
                                            ),
                                        },
                                    ],
                                    rest_param: None,
                                    this_param: None,
                                    return_: Annot(
                                        Number(
                                            3,
                                        ),
                                    ),
                                    type_guard: None,
                                    effect_: ArbitraryEffect,
                                    strictness_kind: Flow,
                                },
                                statics: {},
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(2, Identifier))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_function_literal_check2() {
    let input = r#"
            export default function(x: number) { return x };
        "#;
    let expected_output = r#"
Locs:
0. [1:7-14]
1. [1:15-47]
2. [1:27-33]
3. [1:34]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 0,
                    def: Value(
                        FunExpr(
                            ValueFunExpr {
                                loc: 1,
                                async_: false,
                                generator: false,
                                def: FunSig {
                                    tparams: Mono,
                                    params: [
                                        FunParam {
                                            name: Some(
                                                "x",
                                            ),
                                            t: Annot(
                                                Number(
                                                    2,
                                                ),
                                            ),
                                        },
                                    ],
                                    rest_param: None,
                                    this_param: None,
                                    return_: Err(
                                        3,
                                    ),
                                    type_guard: None,
                                    effect_: ArbitraryEffect,
                                    strictness_kind: Flow,
                                },
                                statics: {},
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(3, FunctionReturn))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_function_reference() {
    let input = r#"
            function foo(x: number): number { return x };
            export default foo;
        "#;
    let expected_output = r#"
Locs:
0. [1:0-31]
1. [1:9-12]
2. [1:16-22]
3. [1:25-31]
4. [2:7-14]
5. [2:15-18]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 4,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 5,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "foo",
                async_: false,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Mono,
                    params: [
                        FunParam {
                            name: Some(
                                "x",
                            ),
                            t: Annot(
                                Number(
                                    2,
                                ),
                            ),
                        },
                    ],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Number(
                            3,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_function_reference_check1() {
    let input = r#"
function foo(x): number { return x }
export default foo;
        "#;
    let expected_output = r#"
Locs:
0. [1:0-23]
1. [1:9-12]
2. [1:13-14]
3. [1:17-23]
4. [2:7-14]
5. [2:15-18]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 4,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 5,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "foo",
                async_: false,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Mono,
                    params: [
                        FunParam {
                            name: Some(
                                "x",
                            ),
                            t: Err(
                                2,
                            ),
                        },
                    ],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Number(
                            3,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(2, Identifier))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_function_reference_check2() {
    let input = r#"
function foo(x: number) { return x }
export default foo;
"#;
    let expected_output = r#"
Locs:
0. [1:0-23]
1. [1:9-12]
2. [1:16-22]
3. [1:23]
4. [2:7-14]
5. [2:15-18]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 4,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 5,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "foo",
                async_: false,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Mono,
                    params: [
                        FunParam {
                            name: Some(
                                "x",
                            ),
                            t: Annot(
                                Number(
                                    2,
                                ),
                            ),
                        },
                    ],
                    rest_param: None,
                    this_param: None,
                    return_: Err(
                        3,
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(3, FunctionReturn))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_function_generic_typeof() {
    let input = r#"
            export function b<X>(x: X, y: typeof x): void { }
        "#;
    let expected_output = r#"
Locs:
0. [1:7-45]
1. [1:16-17]
2. [1:17-20]
3. [1:18-19]
4. [1:21-22]
5. [1:24-25]
6. [1:30-38]
7. [1:37-38]
8. [1:41-45]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "b",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "b",
                async_: false,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Poly(
                        (
                            2,
                            [
                                TParam {
                                    name_loc: 3,
                                    name: "X",
                                    polarity: None,
                                    bound: None,
                                    default: None,
                                    is_const: false,
                                },
                            ],
                        ),
                    ),
                    params: [
                        FunParam {
                            name: Some(
                                "x",
                            ),
                            t: Annot(
                                Bound(
                                    AnnotBound {
                                        ref_loc: 5,
                                        name: "X",
                                    },
                                ),
                            ),
                        },
                        FunParam {
                            name: Some(
                                "y",
                            ),
                            t: Annot(
                                Typeof(
                                    AnnotTypeof {
                                        loc: 6,
                                        qname: [
                                            "x",
                                        ],
                                        t: Ref(
                                            LocalRef(
                                                PackedRefLocal {
                                                    ref_loc: 7,
                                                    index: 1,
                                                },
                                            ),
                                        ),
                                        targs: None,
                                    },
                                ),
                            ),
                        },
                    ],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Void(
                            8,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
        Parameter(
            DefParameter {
                id_loc: 4,
                name: "x",
                def: Annot(
                    Bound(
                        AnnotBound {
                            ref_loc: 5,
                            name: "X",
                        },
                    ),
                ),
                tparams: Poly(
                    (
                        2,
                        [
                            TParam {
                                name_loc: 3,
                                name: "X",
                                polarity: None,
                                bound: None,
                                default: None,
                                is_const: false,
                            },
                        ],
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_function_typeof_return() {
    let input = r#"
            export function a(b: string, c: string): typeof b { return b; }
        "#;
    let expected_output = r#"
Locs:
0. [1:7-49]
1. [1:16-17]
2. [1:18-19]
3. [1:21-27]
4. [1:32-38]
5. [1:41-49]
6. [1:48-49]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "a",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "a",
                async_: false,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Mono,
                    params: [
                        FunParam {
                            name: Some(
                                "b",
                            ),
                            t: Annot(
                                String(
                                    3,
                                ),
                            ),
                        },
                        FunParam {
                            name: Some(
                                "c",
                            ),
                            t: Annot(
                                String(
                                    4,
                                ),
                            ),
                        },
                    ],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Typeof(
                            AnnotTypeof {
                                loc: 5,
                                qname: [
                                    "b",
                                ],
                                t: Ref(
                                    LocalRef(
                                        PackedRefLocal {
                                            ref_loc: 6,
                                            index: 1,
                                        },
                                    ),
                                ),
                                targs: None,
                            },
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
        Parameter(
            DefParameter {
                id_loc: 2,
                name: "b",
                def: Annot(
                    String(
                        3,
                    ),
                ),
                tparams: Mono,
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_function_generic_typeof_return() {
    let input = r#"
            export function b<X>(x: X): typeof x { return x; }
        "#;
    let expected_output = r#"
Locs:
0. [1:7-36]
1. [1:16-17]
2. [1:17-20]
3. [1:18-19]
4. [1:21-22]
5. [1:24-25]
6. [1:28-36]
7. [1:35-36]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "b",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "b",
                async_: false,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Poly(
                        (
                            2,
                            [
                                TParam {
                                    name_loc: 3,
                                    name: "X",
                                    polarity: None,
                                    bound: None,
                                    default: None,
                                    is_const: false,
                                },
                            ],
                        ),
                    ),
                    params: [
                        FunParam {
                            name: Some(
                                "x",
                            ),
                            t: Annot(
                                Bound(
                                    AnnotBound {
                                        ref_loc: 5,
                                        name: "X",
                                    },
                                ),
                            ),
                        },
                    ],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Typeof(
                            AnnotTypeof {
                                loc: 6,
                                qname: [
                                    "x",
                                ],
                                t: Ref(
                                    LocalRef(
                                        PackedRefLocal {
                                            ref_loc: 7,
                                            index: 1,
                                        },
                                    ),
                                ),
                                targs: None,
                            },
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
        Parameter(
            DefParameter {
                id_loc: 4,
                name: "x",
                def: Annot(
                    Bound(
                        AnnotBound {
                            ref_loc: 5,
                            name: "X",
                        },
                    ),
                ),
                tparams: Poly(
                    (
                        2,
                        [
                            TParam {
                                name_loc: 3,
                                name: "X",
                                polarity: None,
                                bound: None,
                                default: None,
                                is_const: false,
                            },
                        ],
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn function_param_optional() {
    let input = r#"
            export default function(p?: string): void {};
        "#;
    let expected_output = r#"
Locs:
0. [1:7-14]
1. [1:15-44]
2. [1:28-34]
3. [1:37-41]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 0,
                    def: Value(
                        FunExpr(
                            ValueFunExpr {
                                loc: 1,
                                async_: false,
                                generator: false,
                                def: FunSig {
                                    tparams: Mono,
                                    params: [
                                        FunParam {
                                            name: Some(
                                                "p",
                                            ),
                                            t: Annot(
                                                Optional(
                                                    Annot(
                                                        String(
                                                            2,
                                                        ),
                                                    ),
                                                ),
                                            ),
                                        },
                                    ],
                                    rest_param: None,
                                    this_param: None,
                                    return_: Annot(
                                        Void(
                                            3,
                                        ),
                                    ),
                                    type_guard: None,
                                    effect_: ArbitraryEffect,
                                    strictness_kind: Flow,
                                },
                                statics: {},
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn function_param_default() {
    let input = r#"
            export default function(p: string = "foo"): void {};
        "#;
    let expected_output = r#"
Locs:
0. [1:7-14]
1. [1:15-51]
2. [1:27-33]
3. [1:44-48]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 0,
                    def: Value(
                        FunExpr(
                            ValueFunExpr {
                                loc: 1,
                                async_: false,
                                generator: false,
                                def: FunSig {
                                    tparams: Mono,
                                    params: [
                                        FunParam {
                                            name: Some(
                                                "p",
                                            ),
                                            t: Annot(
                                                Optional(
                                                    Annot(
                                                        String(
                                                            2,
                                                        ),
                                                    ),
                                                ),
                                            ),
                                        },
                                    ],
                                    rest_param: None,
                                    this_param: None,
                                    return_: Annot(
                                        Void(
                                            3,
                                        ),
                                    ),
                                    type_guard: None,
                                    effect_: ArbitraryEffect,
                                    strictness_kind: Flow,
                                },
                                statics: {},
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn function_param_default_check() {
    let input = r#"
            export default function(p = "foo") {}
        "#;
    let expected_output = r#"
Locs:
0. [1:7-14]
1. [1:15-37]
2. [1:24-25]
3. [1:34]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 0,
                    def: Value(
                        FunExpr(
                            ValueFunExpr {
                                loc: 1,
                                async_: false,
                                generator: false,
                                def: FunSig {
                                    tparams: Mono,
                                    params: [
                                        FunParam {
                                            name: Some(
                                                "p",
                                            ),
                                            t: Annot(
                                                Optional(
                                                    Err(
                                                        2,
                                                    ),
                                                ),
                                            ),
                                        },
                                    ],
                                    rest_param: None,
                                    this_param: None,
                                    return_: Annot(
                                        Void(
                                            3,
                                        ),
                                    ),
                                    type_guard: None,
                                    effect_: ArbitraryEffect,
                                    strictness_kind: Flow,
                                },
                                statics: {},
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(2, Identifier))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn function_param_typeof_reference() {
    let input = r#"
            declare const bar: string;
    export default function(bar: typeof bar, baz: typeof bar, {boz}: {boz: typeof baz}) {}
        "#;
    let expected_output = r#"
Locs:
0. [1:22-25]
1. [1:27-33]
2. [2:7-14]
3. [2:15-86]
4. [2:24-27]
5. [2:29-39]
6. [2:36-39]
7. [2:41-44]
8. [2:46-56]
9. [2:53-56]
10. [2:65-82]
11. [2:66-69]
12. [2:71-81]
13. [2:78-81]
14. [2:83]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 2,
                    def: Value(
                        FunExpr(
                            ValueFunExpr {
                                loc: 3,
                                async_: false,
                                generator: false,
                                def: FunSig {
                                    tparams: Mono,
                                    params: [
                                        FunParam {
                                            name: Some(
                                                "bar",
                                            ),
                                            t: Annot(
                                                Typeof(
                                                    AnnotTypeof {
                                                        loc: 5,
                                                        qname: [
                                                            "bar",
                                                        ],
                                                        t: Ref(
                                                            LocalRef(
                                                                PackedRefLocal {
                                                                    ref_loc: 6,
                                                                    index: 0,
                                                                },
                                                            ),
                                                        ),
                                                        targs: None,
                                                    },
                                                ),
                                            ),
                                        },
                                        FunParam {
                                            name: Some(
                                                "baz",
                                            ),
                                            t: Annot(
                                                Typeof(
                                                    AnnotTypeof {
                                                        loc: 8,
                                                        qname: [
                                                            "bar",
                                                        ],
                                                        t: Ref(
                                                            LocalRef(
                                                                PackedRefLocal {
                                                                    ref_loc: 9,
                                                                    index: 1,
                                                                },
                                                            ),
                                                        ),
                                                        targs: None,
                                                    },
                                                ),
                                            ),
                                        },
                                        FunParam {
                                            name: None,
                                            t: Annot(
                                                ObjAnnot(
                                                    AnnotObjAnnot {
                                                        loc: 10,
                                                        obj_kind: ExactObj,
                                                        props: {
                                                            "boz": ObjAnnotField(
                                                                (
                                                                    11,
                                                                    Annot(
                                                                        Typeof(
                                                                            AnnotTypeof {
                                                                                loc: 12,
                                                                                qname: [
                                                                                    "baz",
                                                                                ],
                                                                                t: Ref(
                                                                                    LocalRef(
                                                                                        PackedRefLocal {
                                                                                            ref_loc: 13,
                                                                                            index: 2,
                                                                                        },
                                                                                    ),
                                                                                ),
                                                                                targs: None,
                                                                            },
                                                                        ),
                                                                    ),
                                                                    Neutral,
                                                                ),
                                                            ),
                                                        },
                                                        computed_props: [],
                                                        proto: ObjAnnotImplicitProto,
                                                        strictness_kind: Flow,
                                                    },
                                                ),
                                            ),
                                        },
                                    ],
                                    rest_param: None,
                                    this_param: None,
                                    return_: Annot(
                                        Void(
                                            14,
                                        ),
                                    ),
                                    type_guard: None,
                                    effect_: ArbitraryEffect,
                                    strictness_kind: Flow,
                                },
                                statics: {},
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "bar",
                def: Annot(
                    String(
                        1,
                    ),
                ),
            },
        ),
        Parameter(
            DefParameter {
                id_loc: 4,
                name: "bar",
                def: Annot(
                    Typeof(
                        AnnotTypeof {
                            loc: 5,
                            qname: [
                                "bar",
                            ],
                            t: Ref(
                                LocalRef(
                                    PackedRefLocal {
                                        ref_loc: 6,
                                        index: 0,
                                    },
                                ),
                            ),
                            targs: None,
                        },
                    ),
                ),
                tparams: Mono,
                strictness_kind: Flow,
            },
        ),
        Parameter(
            DefParameter {
                id_loc: 7,
                name: "baz",
                def: Annot(
                    Typeof(
                        AnnotTypeof {
                            loc: 8,
                            qname: [
                                "bar",
                            ],
                            t: Ref(
                                LocalRef(
                                    PackedRefLocal {
                                        ref_loc: 9,
                                        index: 1,
                                    },
                                ),
                            ),
                            targs: None,
                        },
                    ),
                ),
                tparams: Mono,
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn component_param_typeof_reference() {
    let input = r#"
            declare const bar: string;
    export component C(bar: typeof bar, baz: typeof bar, booz as {boz}: {boz: typeof baz}) {}
        "#;
    let expected_output = r#"
Locs:
0. [1:22-25]
1. [1:27-33]
2. [2:7-86]
3. [2:17-18]
4. [2:18-86]
5. [2:19-22]
6. [2:24-34]
7. [2:31-34]
8. [2:36-39]
9. [2:41-51]
10. [2:48-51]
11. [2:53-57]
12. [2:68-85]
13. [2:69-72]
14. [2:74-84]
15. [2:81-84]
16. [2:86]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                1,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "C",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "bar",
                def: Annot(
                    String(
                        1,
                    ),
                ),
            },
        ),
        ComponentBinding(
            DefComponentBinding {
                id_loc: 3,
                name: "C",
                fn_loc: 2,
                def: ComponentSig {
                    params_loc: 4,
                    tparams: Mono,
                    params: [
                        ComponentParam {
                            name: "bar",
                            name_loc: 5,
                            t: Annot(
                                Typeof(
                                    AnnotTypeof {
                                        loc: 6,
                                        qname: [
                                            "bar",
                                        ],
                                        t: Ref(
                                            LocalRef(
                                                PackedRefLocal {
                                                    ref_loc: 7,
                                                    index: 0,
                                                },
                                            ),
                                        ),
                                        targs: None,
                                    },
                                ),
                            ),
                        },
                        ComponentParam {
                            name: "baz",
                            name_loc: 8,
                            t: Annot(
                                Typeof(
                                    AnnotTypeof {
                                        loc: 9,
                                        qname: [
                                            "bar",
                                        ],
                                        t: Ref(
                                            LocalRef(
                                                PackedRefLocal {
                                                    ref_loc: 10,
                                                    index: 0,
                                                },
                                            ),
                                        ),
                                        targs: None,
                                    },
                                ),
                            ),
                        },
                        ComponentParam {
                            name: "booz",
                            name_loc: 11,
                            t: Annot(
                                ObjAnnot(
                                    AnnotObjAnnot {
                                        loc: 12,
                                        obj_kind: ExactObj,
                                        props: {
                                            "boz": ObjAnnotField(
                                                (
                                                    13,
                                                    Annot(
                                                        Typeof(
                                                            AnnotTypeof {
                                                                loc: 14,
                                                                qname: [
                                                                    "baz",
                                                                ],
                                                                t: Ref(
                                                                    BuiltinRef(
                                                                        PackedRefBuiltin {
                                                                            ref_loc: 15,
                                                                            type_ref: false,
                                                                            name: "baz",
                                                                        },
                                                                    ),
                                                                ),
                                                                targs: None,
                                                            },
                                                        ),
                                                    ),
                                                    Neutral,
                                                ),
                                            ),
                                        },
                                        computed_props: [],
                                        proto: ObjAnnotImplicitProto,
                                        strictness_kind: Flow,
                                    },
                                ),
                            ),
                        },
                    ],
                    rest_param: None,
                    renders: Annot(
                        ComponentMissingRenders(
                            16,
                        ),
                    ),
                    strictness_kind: Flow,
                },
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_object_literal_property_literal() {
    let input = r#"
            export default { p: 0 };
        "#;
    let expected_output = r#"
Locs:
0. [1:7-14]
1. [1:15-23]
2. [1:17-18]
3. [1:20-21]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 0,
                    def: Value(
                        ObjLit(
                            ValueObjLit {
                                loc: 1,
                                frozen: false,
                                proto: None,
                                props: {
                                    "p": ObjValueField(
                                        (
                                            2,
                                            Value(
                                                NumberLit(
                                                    (
                                                        3,
                                                        0.0,
                                                        "0",
                                                    ),
                                                ),
                                            ),
                                            Neutral,
                                        ),
                                    ),
                                },
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_object_literal_property_reference() {
    let input = r#"
            var x: number = 0;
    export default { p: x };
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:15-21]
2. [2:7-14]
3. [2:15-23]
4. [2:17-18]
5. [2:20-21]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 2,
                    def: Value(
                        ObjLit(
                            ValueObjLit {
                                loc: 3,
                                frozen: false,
                                proto: None,
                                props: {
                                    "p": ObjValueField(
                                        (
                                            4,
                                            Ref(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 5,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                            Neutral,
                                        ),
                                    ),
                                },
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "x",
                def: Annot(
                    Number(
                        1,
                    ),
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_object_literal_property_reference_check() {
    let input = r#"
            var x = 0;
    export default { p: x };
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [2:7-14]
2. [2:15-23]
3. [2:17-18]
4. [2:20-21]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 1,
                    def: Value(
                        ObjLit(
                            ValueObjLit {
                                loc: 2,
                                frozen: false,
                                proto: None,
                                props: {
                                    "p": ObjValueField(
                                        (
                                            3,
                                            Ref(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 4,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                            Neutral,
                                        ),
                                    ),
                                },
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "x",
                def: Err(
                    0,
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(0, VariableDefinition { name: "x" }))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn empty_object_literal() {
    let input = r#"
            export default { };
        "#;
    let expected_output = r#"
Locs:
0. [1:7-14]
1. [1:15-18]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 0,
                    def: Value(
                        ObjLit(
                            ValueObjLit {
                                loc: 1,
                                frozen: false,
                                proto: None,
                                props: {},
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_object_literal_number_key() {
    let input = r#"
            export default { 1: true, 2: "hello" };
        "#;
    let expected_output = r#"
Locs:
0. [1:7-14]
1. [1:15-38]
2. [1:17-18]
3. [1:20-24]
4. [1:26-27]
5. [1:29-36]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 0,
                    def: Value(
                        ObjLit(
                            ValueObjLit {
                                loc: 1,
                                frozen: false,
                                proto: None,
                                props: {
                                    "1": ObjValueField(
                                        (
                                            2,
                                            Value(
                                                BooleanLit(
                                                    (
                                                        3,
                                                        true,
                                                    ),
                                                ),
                                            ),
                                            Neutral,
                                        ),
                                    ),
                                    "2": ObjValueField(
                                        (
                                            4,
                                            Value(
                                                StringLit(
                                                    (
                                                        5,
                                                        "hello",
                                                    ),
                                                ),
                                            ),
                                            Neutral,
                                        ),
                                    ),
                                },
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_class_reference() {
    let input = r#"
    class C {
      f: number = 0;
      m(x: number): number { return x; }
    }
    export default C;
        "#;
    let expected_output = r#"
Locs:
0. [1:6-7]
1. [2:2-3]
2. [2:5-11]
3. [3:2-36]
4. [3:2-3]
5. [3:7-13]
6. [3:16-22]
7. [5:7-14]
8. [5:15-16]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 7,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 8,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {
                        "m": ObjValueMethod(
                            [
                                ObjValueMethodData {
                                    id_loc: 4,
                                    fn_loc: 3,
                                    async_: false,
                                    generator: false,
                                    uses_this: false,
                                    def: FunSig {
                                        tparams: Mono,
                                        params: [
                                            FunParam {
                                                name: Some(
                                                    "x",
                                                ),
                                                t: Annot(
                                                    Number(
                                                        5,
                                                    ),
                                                ),
                                            },
                                        ],
                                        rest_param: None,
                                        this_param: None,
                                        return_: Annot(
                                            Number(
                                                6,
                                            ),
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                },
                            ],
                        ),
                    },
                    own_props: {
                        "f": ObjValueField(
                            (
                                1,
                                Annot(
                                    Number(
                                        2,
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_class_reference_check1() {
    let input = r#"
            class C {
      f = 0;
      m(x: number): number { return x; }
    }
    export default C;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [2:2-3]
2. [2:6-7]
3. [3:2-36]
4. [3:2-3]
5. [3:7-13]
6. [3:16-22]
7. [5:7-14]
8. [5:15-16]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 7,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 8,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {
                        "m": ObjValueMethod(
                            [
                                ObjValueMethodData {
                                    id_loc: 4,
                                    fn_loc: 3,
                                    async_: false,
                                    generator: false,
                                    uses_this: false,
                                    def: FunSig {
                                        tparams: Mono,
                                        params: [
                                            FunParam {
                                                name: Some(
                                                    "x",
                                                ),
                                                t: Annot(
                                                    Number(
                                                        5,
                                                    ),
                                                ),
                                            },
                                        ],
                                        rest_param: None,
                                        this_param: None,
                                        return_: Annot(
                                            Number(
                                                6,
                                            ),
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                },
                            ],
                        ),
                    },
                    own_props: {
                        "f": ObjValueField(
                            (
                                1,
                                Value(
                                    NumberLit(
                                        (
                                            2,
                                            0.0,
                                            "0",
                                        ),
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_class_reference_check2() {
    let input = r#"
            class C {
      f: number = 0;
      m(x): number { return x; }
    }
    export default C;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [2:2-3]
2. [2:5-11]
3. [3:2-28]
4. [3:2-3]
5. [3:4-5]
6. [3:8-14]
7. [5:7-14]
8. [5:15-16]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 7,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 8,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {
                        "m": ObjValueMethod(
                            [
                                ObjValueMethodData {
                                    id_loc: 4,
                                    fn_loc: 3,
                                    async_: false,
                                    generator: false,
                                    uses_this: false,
                                    def: FunSig {
                                        tparams: Mono,
                                        params: [
                                            FunParam {
                                                name: Some(
                                                    "x",
                                                ),
                                                t: Err(
                                                    5,
                                                ),
                                            },
                                        ],
                                        rest_param: None,
                                        this_param: None,
                                        return_: Annot(
                                            Number(
                                                6,
                                            ),
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                },
                            ],
                        ),
                    },
                    own_props: {
                        "f": ObjValueField(
                            (
                                1,
                                Annot(
                                    Number(
                                        2,
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(5, Identifier))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_class_reference_check3() {
    let input = r#"
            class C {
      f: number = 0;
      m(x: number) { return x; }
    }
    export default C;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [2:2-3]
2. [2:5-11]
3. [3:2-28]
4. [3:2-3]
5. [3:7-13]
6. [3:14]
7. [5:7-14]
8. [5:15-16]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 7,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 8,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {
                        "m": ObjValueMethod(
                            [
                                ObjValueMethodData {
                                    id_loc: 4,
                                    fn_loc: 3,
                                    async_: false,
                                    generator: false,
                                    uses_this: false,
                                    def: FunSig {
                                        tparams: Mono,
                                        params: [
                                            FunParam {
                                                name: Some(
                                                    "x",
                                                ),
                                                t: Annot(
                                                    Number(
                                                        5,
                                                    ),
                                                ),
                                            },
                                        ],
                                        rest_param: None,
                                        this_param: None,
                                        return_: Err(
                                            6,
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                },
                            ],
                        ),
                    },
                    own_props: {
                        "f": ObjValueField(
                            (
                                1,
                                Annot(
                                    Number(
                                        2,
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(6, FunctionReturn))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn type_alias_dependencies() {
    let input = r#"
            type T1 = number;
    type T2 = number;
    type T3 = number;
    class C {
      f: T1 = 0;
      m(x: T2): T3 { return x; }
    }
    export default C;
        "#;
    let expected_output = r#"
Locs:
0. [1:13-15]
1. [1:18-24]
2. [2:5-7]
3. [2:10-16]
4. [3:5-7]
5. [3:10-16]
6. [4:6-7]
7. [5:2-3]
8. [5:5-7]
9. [6:2-28]
10. [6:2-3]
11. [6:7-9]
12. [6:12-14]
13. [8:7-14]
14. [8:15-16]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 13,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 14,
                                index: 3,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "T1",
                tparams: Mono,
                body: Annot(
                    Number(
                        1,
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 2,
                custom_error_loc_opt: None,
                name: "T2",
                tparams: Mono,
                body: Annot(
                    Number(
                        3,
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 4,
                custom_error_loc_opt: None,
                name: "T3",
                tparams: Mono,
                body: Annot(
                    Number(
                        5,
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        ClassBinding(
            DefClassBinding {
                id_loc: 6,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {
                        "m": ObjValueMethod(
                            [
                                ObjValueMethodData {
                                    id_loc: 10,
                                    fn_loc: 9,
                                    async_: false,
                                    generator: false,
                                    uses_this: false,
                                    def: FunSig {
                                        tparams: Mono,
                                        params: [
                                            FunParam {
                                                name: Some(
                                                    "x",
                                                ),
                                                t: TyRef(
                                                    Unqualified(
                                                        LocalRef(
                                                            PackedRefLocal {
                                                                ref_loc: 11,
                                                                index: 1,
                                                            },
                                                        ),
                                                    ),
                                                ),
                                            },
                                        ],
                                        rest_param: None,
                                        this_param: None,
                                        return_: TyRef(
                                            Unqualified(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 12,
                                                        index: 2,
                                                    },
                                                ),
                                            ),
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                },
                            ],
                        ),
                    },
                    own_props: {
                        "f": ObjValueField(
                            (
                                7,
                                TyRef(
                                    Unqualified(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 8,
                                                index: 0,
                                            },
                                        ),
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn class_dependencies() {
    let input = r#"
            class D { f: number = 0; }
    class C {
      f: D = new D;
      m(x: D): D { return x; }
    }
    export default C;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [1:18-19]
2. [1:21-27]
3. [2:6-7]
4. [3:2-3]
5. [3:5-6]
6. [4:2-26]
7. [4:2-3]
8. [4:7-8]
9. [4:11-12]
10. [6:7-14]
11. [6:15-16]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 10,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 11,
                                index: 1,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "D",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {
                        "f": ObjValueField(
                            (
                                1,
                                Annot(
                                    Number(
                                        2,
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
        ClassBinding(
            DefClassBinding {
                id_loc: 3,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {
                        "m": ObjValueMethod(
                            [
                                ObjValueMethodData {
                                    id_loc: 7,
                                    fn_loc: 6,
                                    async_: false,
                                    generator: false,
                                    uses_this: false,
                                    def: FunSig {
                                        tparams: Mono,
                                        params: [
                                            FunParam {
                                                name: Some(
                                                    "x",
                                                ),
                                                t: TyRef(
                                                    Unqualified(
                                                        LocalRef(
                                                            PackedRefLocal {
                                                                ref_loc: 8,
                                                                index: 0,
                                                            },
                                                        ),
                                                    ),
                                                ),
                                            },
                                        ],
                                        rest_param: None,
                                        this_param: None,
                                        return_: TyRef(
                                            Unqualified(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 9,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                },
                            ],
                        ),
                    },
                    own_props: {
                        "f": ObjValueField(
                            (
                                4,
                                TyRef(
                                    Unqualified(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 5,
                                                index: 0,
                                            },
                                        ),
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn class_dependencies_check() {
    let input = r#"
            class D { f = 0; }
    class C {
      f: D = new D;
      m(x: D): D { return x; }
    }
    export default C;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [1:18-19]
2. [1:22-23]
3. [2:6-7]
4. [3:2-3]
5. [3:5-6]
6. [4:2-26]
7. [4:2-3]
8. [4:7-8]
9. [4:11-12]
10. [6:7-14]
11. [6:15-16]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 10,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 11,
                                index: 1,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "D",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {
                        "f": ObjValueField(
                            (
                                1,
                                Value(
                                    NumberLit(
                                        (
                                            2,
                                            0.0,
                                            "0",
                                        ),
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
        ClassBinding(
            DefClassBinding {
                id_loc: 3,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {
                        "m": ObjValueMethod(
                            [
                                ObjValueMethodData {
                                    id_loc: 7,
                                    fn_loc: 6,
                                    async_: false,
                                    generator: false,
                                    uses_this: false,
                                    def: FunSig {
                                        tparams: Mono,
                                        params: [
                                            FunParam {
                                                name: Some(
                                                    "x",
                                                ),
                                                t: TyRef(
                                                    Unqualified(
                                                        LocalRef(
                                                            PackedRefLocal {
                                                                ref_loc: 8,
                                                                index: 0,
                                                            },
                                                        ),
                                                    ),
                                                ),
                                            },
                                        ],
                                        rest_param: None,
                                        this_param: None,
                                        return_: TyRef(
                                            Unqualified(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 9,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                },
                            ],
                        ),
                    },
                    own_props: {
                        "f": ObjValueField(
                            (
                                4,
                                TyRef(
                                    Unqualified(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 5,
                                                index: 0,
                                            },
                                        ),
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_new_typecast() {
    let input = r#"
            class D { f: number = 0; }
    class C {
      f: D = new D;
      m(x: D): D { return x; }
    }
    export default (new C: C);
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [1:18-19]
2. [1:21-27]
3. [2:6-7]
4. [3:2-3]
5. [3:5-6]
6. [4:2-26]
7. [4:2-3]
8. [4:7-8]
9. [4:11-12]
10. [6:7-14]
11. [6:23-24]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 10,
                    def: TyRef(
                        Unqualified(
                            LocalRef(
                                PackedRefLocal {
                                    ref_loc: 11,
                                    index: 1,
                                },
                            ),
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "D",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {
                        "f": ObjValueField(
                            (
                                1,
                                Annot(
                                    Number(
                                        2,
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
        ClassBinding(
            DefClassBinding {
                id_loc: 3,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {
                        "m": ObjValueMethod(
                            [
                                ObjValueMethodData {
                                    id_loc: 7,
                                    fn_loc: 6,
                                    async_: false,
                                    generator: false,
                                    uses_this: false,
                                    def: FunSig {
                                        tparams: Mono,
                                        params: [
                                            FunParam {
                                                name: Some(
                                                    "x",
                                                ),
                                                t: TyRef(
                                                    Unqualified(
                                                        LocalRef(
                                                            PackedRefLocal {
                                                                ref_loc: 8,
                                                                index: 0,
                                                            },
                                                        ),
                                                    ),
                                                ),
                                            },
                                        ],
                                        rest_param: None,
                                        this_param: None,
                                        return_: TyRef(
                                            Unqualified(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 9,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                },
                            ],
                        ),
                    },
                    own_props: {
                        "f": ObjValueField(
                            (
                                4,
                                TyRef(
                                    Unqualified(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 5,
                                                index: 0,
                                            },
                                        ),
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_new_typecast_check() {
    let input = r#"
            class D { f = 0; }
    class C {
      f: D = new D;
      m(x: D): D { return x; }
    }
    export default (new C: C);
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [1:18-19]
2. [1:22-23]
3. [2:6-7]
4. [3:2-3]
5. [3:5-6]
6. [4:2-26]
7. [4:2-3]
8. [4:7-8]
9. [4:11-12]
10. [6:7-14]
11. [6:23-24]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 10,
                    def: TyRef(
                        Unqualified(
                            LocalRef(
                                PackedRefLocal {
                                    ref_loc: 11,
                                    index: 1,
                                },
                            ),
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "D",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {
                        "f": ObjValueField(
                            (
                                1,
                                Value(
                                    NumberLit(
                                        (
                                            2,
                                            0.0,
                                            "0",
                                        ),
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
        ClassBinding(
            DefClassBinding {
                id_loc: 3,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {
                        "m": ObjValueMethod(
                            [
                                ObjValueMethodData {
                                    id_loc: 7,
                                    fn_loc: 6,
                                    async_: false,
                                    generator: false,
                                    uses_this: false,
                                    def: FunSig {
                                        tparams: Mono,
                                        params: [
                                            FunParam {
                                                name: Some(
                                                    "x",
                                                ),
                                                t: TyRef(
                                                    Unqualified(
                                                        LocalRef(
                                                            PackedRefLocal {
                                                                ref_loc: 8,
                                                                index: 0,
                                                            },
                                                        ),
                                                    ),
                                                ),
                                            },
                                        ],
                                        rest_param: None,
                                        this_param: None,
                                        return_: TyRef(
                                            Unqualified(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 9,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                },
                            ],
                        ),
                    },
                    own_props: {
                        "f": ObjValueField(
                            (
                                4,
                                TyRef(
                                    Unqualified(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 5,
                                                index: 0,
                                            },
                                        ),
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn recursive_dependencies() {
    let input = r#"
            class C {
      f: C = new C;
      m(x: C): C { return x; }
    }
    export default C;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [2:2-3]
2. [2:5-6]
3. [3:2-26]
4. [3:2-3]
5. [3:7-8]
6. [3:11-12]
7. [5:7-14]
8. [5:15-16]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 7,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 8,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {
                        "m": ObjValueMethod(
                            [
                                ObjValueMethodData {
                                    id_loc: 4,
                                    fn_loc: 3,
                                    async_: false,
                                    generator: false,
                                    uses_this: false,
                                    def: FunSig {
                                        tparams: Mono,
                                        params: [
                                            FunParam {
                                                name: Some(
                                                    "x",
                                                ),
                                                t: TyRef(
                                                    Unqualified(
                                                        LocalRef(
                                                            PackedRefLocal {
                                                                ref_loc: 5,
                                                                index: 0,
                                                            },
                                                        ),
                                                    ),
                                                ),
                                            },
                                        ],
                                        rest_param: None,
                                        this_param: None,
                                        return_: TyRef(
                                            Unqualified(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 6,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                },
                            ],
                        ),
                    },
                    own_props: {
                        "f": ObjValueField(
                            (
                                1,
                                TyRef(
                                    Unqualified(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 2,
                                                index: 0,
                                            },
                                        ),
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn recursive_dependencies_check() {
    let input = r#"
            class C {
      f = new C;
      m(x: C): C { return x; }
    }
    export default C;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [2:2-3]
2. [2:6-11]
3. [3:2-26]
4. [3:2-3]
5. [3:7-8]
6. [3:11-12]
7. [5:7-14]
8. [5:15-16]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 7,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 8,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {
                        "m": ObjValueMethod(
                            [
                                ObjValueMethodData {
                                    id_loc: 4,
                                    fn_loc: 3,
                                    async_: false,
                                    generator: false,
                                    uses_this: false,
                                    def: FunSig {
                                        tparams: Mono,
                                        params: [
                                            FunParam {
                                                name: Some(
                                                    "x",
                                                ),
                                                t: TyRef(
                                                    Unqualified(
                                                        LocalRef(
                                                            PackedRefLocal {
                                                                ref_loc: 5,
                                                                index: 0,
                                                            },
                                                        ),
                                                    ),
                                                ),
                                            },
                                        ],
                                        rest_param: None,
                                        this_param: None,
                                        return_: TyRef(
                                            Unqualified(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 6,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                },
                            ],
                        ),
                    },
                    own_props: {
                        "f": ObjValueField(
                            (
                                1,
                                Err(
                                    2,
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(UnexpectedExpression(2, New))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn typeof_dependencies() {
    let input = r#"
            var x: number = 0;
    class C {
      p: typeof x = 0;
    }
    export default (new C: C);
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:15-21]
2. [2:6-7]
3. [3:2-3]
4. [3:5-13]
5. [3:12-13]
6. [5:7-14]
7. [5:23-24]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 6,
                    def: TyRef(
                        Unqualified(
                            LocalRef(
                                PackedRefLocal {
                                    ref_loc: 7,
                                    index: 1,
                                },
                            ),
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "x",
                def: Annot(
                    Number(
                        1,
                    ),
                ),
            },
        ),
        ClassBinding(
            DefClassBinding {
                id_loc: 2,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {
                        "p": ObjValueField(
                            (
                                3,
                                Annot(
                                    Typeof(
                                        AnnotTypeof {
                                            loc: 4,
                                            qname: [
                                                "x",
                                            ],
                                            t: Ref(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 5,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                            targs: None,
                                        },
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input))) //
}

#[test]
fn typeof_dependencies_check() {
    let input = r#"
            var x = 0;
    class C {
      p: typeof x = 0;
    }
    export default (new C: C);
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [2:6-7]
2. [3:2-3]
3. [3:5-13]
4. [3:12-13]
5. [5:7-14]
6. [5:23-24]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 5,
                    def: TyRef(
                        Unqualified(
                            LocalRef(
                                PackedRefLocal {
                                    ref_loc: 6,
                                    index: 1,
                                },
                            ),
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "x",
                def: Err(
                    0,
                ),
            },
        ),
        ClassBinding(
            DefClassBinding {
                id_loc: 1,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {
                        "p": ObjValueField(
                            (
                                2,
                                Annot(
                                    Typeof(
                                        AnnotTypeof {
                                            loc: 3,
                                            qname: [
                                                "x",
                                            ],
                                            t: Ref(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 4,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                            targs: None,
                                        },
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(0, VariableDefinition { name: "x" }))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn const_initializer() {
    let input = r#"
            const x = 0;
    export default { x };
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [1:18-19]
2. [2:7-14]
3. [2:15-20]
4. [2:17-18]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 2,
                    def: Value(
                        ObjLit(
                            ValueObjLit {
                                loc: 3,
                                frozen: false,
                                proto: None,
                                props: {
                                    "x": ObjValueField(
                                        (
                                            4,
                                            Ref(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 4,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                            Neutral,
                                        ),
                                    ),
                                },
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "x",
                def: Value(
                    NumberLit(
                        (
                            1,
                            0.0,
                            "0",
                        ),
                    ),
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn empty_array_literal() {
    let input = r#"
            export default [ ];
        "#;
    let expected_output = r#"
Locs:
0. [1:7-14]
1. [1:15-18]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 0,
                    def: Err(
                        1,
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(EmptyArray(1))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn non_empty_array_literal() {
    let input = r#"
            const x = 0;
    var y = false;
    export default [ x, y ];
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [1:18-19]
2. [2:4-5]
3. [3:7-14]
4. [3:15-23]
5. [3:17-18]
6. [3:20-21]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 3,
                    def: Value(
                        ArrayLit(
                            ValueArrayLit {
                                loc: 4,
                                elem_t: Ref(
                                    LocalRef(
                                        PackedRefLocal {
                                            ref_loc: 5,
                                            index: 0,
                                        },
                                    ),
                                ),
                                elements: [
                                    Ref(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 6,
                                                index: 1,
                                            },
                                        ),
                                    ),
                                ],
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "x",
                def: Value(
                    NumberLit(
                        (
                            1,
                            0.0,
                            "0",
                        ),
                    ),
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 2,
                name: "y",
                def: Err(
                    2,
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(2, VariableDefinition { name: "y" }))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn void_function() {
    let input = r#"
            function foo() {}
    export default foo;
        "#;
    let expected_output = r#"
Locs:
0. [1:8-22]
1. [1:17-20]
2. [1:22]
3. [2:7-14]
4. [2:15-18]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 3,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 4,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "foo",
                async_: false,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Void(
                            2,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn void_generator() {
    let input = r#"
            function* foo() { yield 0; }
    export default foo;
        "#;
    let expected_output = r#"
Locs:
0. [1:8-23]
1. [1:18-21]
2. [1:23]
3. [2:7-14]
4. [2:15-18]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 3,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 4,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "foo",
                async_: false,
                generator: true,
                fn_loc: 0,
                def: FunSig {
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    this_param: None,
                    return_: Err(
                        2,
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(2, FunctionReturn))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn import_default_dependencies() {
    let input = r#"
            import x from './import_default_dependencies_helper';
    class C {
      p: typeof x = 0;
    }
    export default (new C: C);
        "#;
    let expected_output = r#"
Locs:
0. [1:15-16]
1. [2:6-7]
2. [3:2-3]
3. [3:5-13]
4. [3:12-13]
5. [5:7-14]
6. [5:23-24]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 5,
                    def: TyRef(
                        Unqualified(
                            LocalRef(
                                PackedRefLocal {
                                    ref_loc: 6,
                                    index: 0,
                                },
                            ),
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [
        Userland(
            "./import_default_dependencies_helper",
        ),
    ],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 1,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {
                        "p": ObjValueField(
                            (
                                2,
                                Annot(
                                    Typeof(
                                        AnnotTypeof {
                                            loc: 3,
                                            qname: [
                                                "x",
                                            ],
                                            t: Ref(
                                                RemoteRef(
                                                    PackedRefRemote {
                                                        ref_loc: 4,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                            targs: None,
                                        },
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [
        Import {
            id_loc: 0,
            name: "x",
            index: 0,
            remote: "default",
            strictness_kind: Flow,
        },
    ],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn import_type_dependencies() {
    let input = r#"
            import type { T1, T2, T3 } from './import_type_dependencies_helper';
    class C {
      f: T1 = 0;
      m(x: T2): T3 { return x; }
    }
    export default C;
        "#;
    let expected_output = r#"
Locs:
0. [1:22-24]
1. [1:26-28]
2. [1:30-32]
3. [2:6-7]
4. [3:2-3]
5. [3:5-7]
6. [4:2-28]
7. [4:2-3]
8. [4:7-9]
9. [4:12-14]
10. [6:7-14]
11. [6:15-16]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 10,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 11,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [
        Userland(
            "./import_type_dependencies_helper",
        ),
    ],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 3,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {
                        "m": ObjValueMethod(
                            [
                                ObjValueMethodData {
                                    id_loc: 7,
                                    fn_loc: 6,
                                    async_: false,
                                    generator: false,
                                    uses_this: false,
                                    def: FunSig {
                                        tparams: Mono,
                                        params: [
                                            FunParam {
                                                name: Some(
                                                    "x",
                                                ),
                                                t: TyRef(
                                                    Unqualified(
                                                        RemoteRef(
                                                            PackedRefRemote {
                                                                ref_loc: 8,
                                                                index: 1,
                                                            },
                                                        ),
                                                    ),
                                                ),
                                            },
                                        ],
                                        rest_param: None,
                                        this_param: None,
                                        return_: TyRef(
                                            Unqualified(
                                                RemoteRef(
                                                    PackedRefRemote {
                                                        ref_loc: 9,
                                                        index: 2,
                                                    },
                                                ),
                                            ),
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                },
                            ],
                        ),
                    },
                    own_props: {
                        "f": ObjValueField(
                            (
                                4,
                                TyRef(
                                    Unqualified(
                                        RemoteRef(
                                            PackedRefRemote {
                                                ref_loc: 5,
                                                index: 0,
                                            },
                                        ),
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [
        ImportType {
            id_loc: 0,
            name: "T1",
            index: 0,
            remote: "T1",
            strictness_kind: Flow,
        },
        ImportType {
            id_loc: 1,
            name: "T2",
            index: 0,
            remote: "T2",
            strictness_kind: Flow,
        },
        ImportType {
            id_loc: 2,
            name: "T3",
            index: 0,
            remote: "T3",
            strictness_kind: Flow,
        },
    ],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn qualified_references() {
    let input = r#"
            import M1 from './qualified_references_helper';
    import type M2 from './qualified_references_helper';
    class C {
      m(x: M1.T): M2.T { return x; }
    }
    export default C;
        "#;
    let expected_output = r#"
Locs:
0. [1:15-17]
1. [2:12-14]
2. [3:6-7]
3. [4:2-32]
4. [4:2-3]
5. [4:7-11]
6. [4:7-9]
7. [4:10-11]
8. [4:14-18]
9. [4:14-16]
10. [4:17-18]
11. [6:7-14]
12. [6:15-16]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 11,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 12,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [
        Userland(
            "./qualified_references_helper",
        ),
    ],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 2,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {
                        "m": ObjValueMethod(
                            [
                                ObjValueMethodData {
                                    id_loc: 4,
                                    fn_loc: 3,
                                    async_: false,
                                    generator: false,
                                    uses_this: false,
                                    def: FunSig {
                                        tparams: Mono,
                                        params: [
                                            FunParam {
                                                name: Some(
                                                    "x",
                                                ),
                                                t: TyRef(
                                                    Qualified(
                                                        TyRefQualified {
                                                            loc: 5,
                                                            id_loc: 7,
                                                            name: "T",
                                                            qualification: Unqualified(
                                                                RemoteRef(
                                                                    PackedRefRemote {
                                                                        ref_loc: 6,
                                                                        index: 0,
                                                                    },
                                                                ),
                                                            ),
                                                            strictness_kind: Flow,
                                                        },
                                                    ),
                                                ),
                                            },
                                        ],
                                        rest_param: None,
                                        this_param: None,
                                        return_: TyRef(
                                            Qualified(
                                                TyRefQualified {
                                                    loc: 8,
                                                    id_loc: 10,
                                                    name: "T",
                                                    qualification: Unqualified(
                                                        RemoteRef(
                                                            PackedRefRemote {
                                                                ref_loc: 9,
                                                                index: 1,
                                                            },
                                                        ),
                                                    ),
                                                    strictness_kind: Flow,
                                                },
                                            ),
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                },
                            ],
                        ),
                    },
                    own_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [
        Import {
            id_loc: 0,
            name: "M1",
            index: 0,
            remote: "default",
            strictness_kind: Flow,
        },
        ImportType {
            id_loc: 1,
            name: "M2",
            index: 0,
            remote: "default",
            strictness_kind: Flow,
        },
    ],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn invalid_qualified_references() {
    let input = r#"
export type T<U> = U.V;
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:13-16]
2. [1:14-15]
3. [1:19-20]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "T",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "T",
                tparams: Poly(
                    (
                        1,
                        [
                            TParam {
                                name_loc: 2,
                                name: "U",
                                polarity: None,
                                bound: None,
                                default: None,
                                is_const: false,
                            },
                        ],
                    ),
                ),
                body: Err(
                    3,
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
CheckError
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn hoisted_requires() {
    let input = r#"
            const M = require('./hoisted_requires_helper');
    if (Math.random() < 0.5) {
      var { D } = require('./hoisted_requires_helper');
    } else {
      var { D } = require('./hoisted_requires_helper');
    }
    var D = 0;
    class C extends M.D {
      f: D = 0;
    }
    module.exports = C;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [1:18-54]
2. [3:8-9]
3. [3:11]
4. [8:6-7]
5. [8:16-19]
6. [8:16-17]
7. [9:2-3]
8. [9:5-6]
9. [11:17-18]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Ref(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 9,
                        index: 2,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [
        Userland(
            "./hoisted_requires_helper",
        ),
    ],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "M",
                def: Require(
                    PackedLocIndex {
                        loc: 1,
                        index: 0,
                        strictness_kind: Flow,
                    },
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 2,
                name: "D",
                def: Pattern(
                    1,
                ),
            },
        ),
        ClassBinding(
            DefClassBinding {
                id_loc: 4,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassExplicitExtends(
                        (
                            5,
                            Eval(
                                PackedEval {
                                    loc: 5,
                                    packed: Ref(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 6,
                                                index: 0,
                                            },
                                        ),
                                    ),
                                    op: GetProp(
                                        "D",
                                    ),
                                    strictness_kind: Flow,
                                },
                            ),
                        ),
                    ),
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {
                        "f": ObjValueField(
                            (
                                7,
                                TyRef(
                                    Unqualified(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 8,
                                                index: 1,
                                            },
                                        ),
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [
        Err(
            3,
        ),
    ],
    dirty_pattern_defs: [],
    patterns: [
        PDef(
            0,
        ),
        PropP {
            id_loc: 2,
            name: "D",
            def: 0,
            strictness_kind: Flow,
        },
    ],
}
Errors:
SigError(ExpectedAnnotation(3, ArrayPattern))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn hoisted_locals() {
    let input = r#"
            const M = require('./hoisted_locals_helper');
    if (Math.random() < 0.5) {
      var D = 0;
    } else {
      var D = false;
    }
    class C extends M.D {
      f: D = 0;
    }
    module.exports = C;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [1:18-52]
2. [3:6-7]
3. [7:6-7]
4. [7:16-19]
5. [7:16-17]
6. [8:2-3]
7. [8:5-6]
8. [10:17-18]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Ref(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 8,
                        index: 2,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [
        Userland(
            "./hoisted_locals_helper",
        ),
    ],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "M",
                def: Require(
                    PackedLocIndex {
                        loc: 1,
                        index: 0,
                        strictness_kind: Flow,
                    },
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 2,
                name: "D",
                def: Err(
                    2,
                ),
            },
        ),
        ClassBinding(
            DefClassBinding {
                id_loc: 3,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassExplicitExtends(
                        (
                            4,
                            Eval(
                                PackedEval {
                                    loc: 4,
                                    packed: Ref(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 5,
                                                index: 0,
                                            },
                                        ),
                                    ),
                                    op: GetProp(
                                        "D",
                                    ),
                                    strictness_kind: Flow,
                                },
                            ),
                        ),
                    ),
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {
                        "f": ObjValueField(
                            (
                                6,
                                TyRef(
                                    Unqualified(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 7,
                                                index: 1,
                                            },
                                        ),
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(2, VariableDefinition { name: "D" }))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn dynamic_requires() {
    let input = r#"
            module.exports = require('./dynamic_requires_helper');
        "#;
    let expected_output = r#"
Locs:
0. [1:17-53]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Require(
                PackedLocIndex {
                    loc: 0,
                    index: 0,
                    strictness_kind: Flow,
                },
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [
        Userland(
            "./dynamic_requires_helper",
        ),
    ],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn import_dynamic() {
    let input = r#"
            module.exports = import('foo');
        "#;
    let expected_output = r#"
Locs:
0. [1:17-30]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            ImportDynamic(
                PackedLocIndex {
                    loc: 0,
                    index: 0,
                    strictness_kind: Flow,
                },
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [
        Userland(
            "foo",
        ),
    ],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn enable_relay_integration() {
    let input = r#"
            module.exports = graphql`query foo {}`;
        "#;
    let expected_output = r#"
Locs:
0. [1:17-38]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Require(
                PackedLocIndex {
                    loc: 0,
                    index: 0,
                    strictness_kind: Flow,
                },
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [
        Userland(
            "foo.graphql",
        ),
    ],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_options(
            input,
            Some(|opts: &mut TypeSigOptions| {
                opts.enable_relay_integration = true;
            })
        ))
    )
}

#[test]
fn relay_integration_module_prefix() {
    let input = r#"
            module.exports = graphql`query foo {}`;
        "#;
    let expected_output = r#"
Locs:
0. [1:17-38]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Require(
                PackedLocIndex {
                    loc: 0,
                    index: 0,
                    strictness_kind: Flow,
                },
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [
        Userland(
            "./__generated__/foo.graphql",
        ),
    ],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_options(
            input,
            Some(|opts: &mut TypeSigOptions| {
                opts.enable_relay_integration = true;
                opts.relay_integration_module_prefix = Some("./__generated__/".to_string());
            })
        ))
    )
}

#[test]
fn scope_extrusion() {
    let input = r#"
            {
      class C {}
      var x: C = new C;
    }
    class C {
      f = 0;
    }
    module.exports = x;
        "#;
    let expected_output = r#"
Locs:
0. [2:8-9]
1. [3:6-7]
2. [3:9-10]
3. [8:17-18]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Ref(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 3,
                        index: 1,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
        Variable(
            DefVariable {
                id_loc: 1,
                name: "x",
                def: TyRef(
                    Unqualified(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 2,
                                index: 0,
                            },
                        ),
                    ),
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn scope_extrusion_nested() {
    let input = r#"
            {
      class C {}
      let y = 0;
      if (b) {
        var x: C = new C;
      }
    }
    class C {
      f = 0;
    }
    module.exports = { x, y };
        "#;
    let expected_output = r#"
Locs:
0. [2:8-9]
1. [5:8-9]
2. [5:11-12]
3. [11:17-25]
4. [11:19-20]
5. [11:22-23]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Value(
                ObjLit(
                    ValueObjLit {
                        loc: 3,
                        frozen: false,
                        proto: None,
                        props: {
                            "x": ObjValueField(
                                (
                                    4,
                                    Ref(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 4,
                                                index: 1,
                                            },
                                        ),
                                    ),
                                    Neutral,
                                ),
                            ),
                            "y": ObjValueField(
                                (
                                    5,
                                    Ref(
                                        BuiltinRef(
                                            PackedRefBuiltin {
                                                ref_loc: 5,
                                                type_ref: false,
                                                name: "y",
                                            },
                                        ),
                                    ),
                                    Neutral,
                                ),
                            ),
                        },
                        strictness_kind: Flow,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
        Variable(
            DefVariable {
                id_loc: 1,
                name: "x",
                def: TyRef(
                    Unqualified(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 2,
                                index: 0,
                            },
                        ),
                    ),
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn report_all_errors() {
    let input = r#"
            class A {
      f = (x: number) => x;
    }
    module.exports = {
      a: A,
      b: (x: string) => x,
    };
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [2:2-3]
2. [2:6-22]
3. [2:10-16]
4. [2:17]
5. [4:17-7:1]
6. [5:2-3]
7. [5:5-6]
8. [6:2-3]
9. [6:5-21]
10. [6:9-15]
11. [6:16]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Value(
                ObjLit(
                    ValueObjLit {
                        loc: 5,
                        frozen: false,
                        proto: None,
                        props: {
                            "a": ObjValueField(
                                (
                                    6,
                                    Ref(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 7,
                                                index: 0,
                                            },
                                        ),
                                    ),
                                    Neutral,
                                ),
                            ),
                            "b": ObjValueField(
                                (
                                    8,
                                    Value(
                                        FunExpr(
                                            ValueFunExpr {
                                                loc: 9,
                                                async_: false,
                                                generator: false,
                                                def: FunSig {
                                                    tparams: Mono,
                                                    params: [
                                                        FunParam {
                                                            name: Some(
                                                                "x",
                                                            ),
                                                            t: Annot(
                                                                String(
                                                                    10,
                                                                ),
                                                            ),
                                                        },
                                                    ],
                                                    rest_param: None,
                                                    this_param: None,
                                                    return_: Err(
                                                        11,
                                                    ),
                                                    type_guard: None,
                                                    effect_: ArbitraryEffect,
                                                    strictness_kind: Flow,
                                                },
                                                statics: {},
                                            },
                                        ),
                                    ),
                                    Neutral,
                                ),
                            ),
                        },
                        strictness_kind: Flow,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "A",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {
                        "f": ObjValueField(
                            (
                                1,
                                Value(
                                    FunExpr(
                                        ValueFunExpr {
                                            loc: 2,
                                            async_: false,
                                            generator: false,
                                            def: FunSig {
                                                tparams: Mono,
                                                params: [
                                                    FunParam {
                                                        name: Some(
                                                            "x",
                                                        ),
                                                        t: Annot(
                                                            Number(
                                                                3,
                                                            ),
                                                        ),
                                                    },
                                                ],
                                                rest_param: None,
                                                this_param: None,
                                                return_: Err(
                                                    4,
                                                ),
                                                type_guard: None,
                                                effect_: ArbitraryEffect,
                                                strictness_kind: Flow,
                                            },
                                            statics: {},
                                        },
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(4, FunctionReturn))
SigError(ExpectedAnnotation(11, FunctionReturn))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn munged_methods_ignored_if_directive() {
    let input = r#"
            class C {
      _method() { return 1; }
    }
    export default C;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [4:7-14]
2. [4:15-16]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 1,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 2,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_options(
            input,
            Some(|opts: &mut TypeSigOptions| {
                opts.munge = true;
            })
        ))
    )
}

#[test]
fn munged_methods_not_ignored() {
    let input = r#"
            class C {
      _method() { return 1; }
    }
    export default C;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [2:2-25]
2. [2:2-9]
3. [2:11]
4. [4:7-14]
5. [4:15-16]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 4,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 5,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {
                        "_method": ObjValueMethod(
                            [
                                ObjValueMethodData {
                                    id_loc: 2,
                                    fn_loc: 1,
                                    async_: false,
                                    generator: false,
                                    uses_this: false,
                                    def: FunSig {
                                        tparams: Mono,
                                        params: [],
                                        rest_param: None,
                                        this_param: None,
                                        return_: Err(
                                            3,
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                },
                            ],
                        ),
                    },
                    own_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(3, FunctionReturn))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn munged_fields_ignored_if_directive() {
    let input = r#"
            class C {
      _method = () => { return 1; }
    }
    export default C;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [4:7-14]
2. [4:15-16]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 1,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 2,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_options(
            input,
            Some(|opts: &mut TypeSigOptions| {
                opts.munge = true;
            })
        ))
    )
}

#[test]
fn munged_fields_not_ignored() {
    let input = r#"
            class C {
      _method = () => { return 1; }
    }
    export default C;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [2:2-9]
2. [2:12-31]
3. [2:14]
4. [4:7-14]
5. [4:15-16]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 4,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 5,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {
                        "_method": ObjValueField(
                            (
                                1,
                                Value(
                                    FunExpr(
                                        ValueFunExpr {
                                            loc: 2,
                                            async_: false,
                                            generator: false,
                                            def: FunSig {
                                                tparams: Mono,
                                                params: [],
                                                rest_param: None,
                                                this_param: None,
                                                return_: Err(
                                                    3,
                                                ),
                                                type_guard: None,
                                                effect_: ArbitraryEffect,
                                                strictness_kind: Flow,
                                            },
                                            statics: {},
                                        },
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(3, FunctionReturn))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn prop_types_static_failure() {
    let input = r#"
            class C {
      static propTypes = {}
    }
    export default C;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [2:9-18]
2. [2:21-23]
3. [4:7-14]
4. [4:15-16]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 3,
                    def: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 4,
                                index: 0,
                            },
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {
                        "propTypes": ObjValueField(
                            (
                                1,
                                Value(
                                    ObjLit(
                                        ValueObjLit {
                                            loc: 2,
                                            frozen: false,
                                            proto: None,
                                            props: {},
                                            strictness_kind: Flow,
                                        },
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    proto_props: {},
                    own_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn array_spread() {
    let input = r#"
            module.exports = [1, ...[2, 3], 4];
        "#;
    let expected_output = r#"
Locs:
0. [1:17-34]
1. [1:21-30]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Err(
                0,
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(UnexpectedArraySpread(0, 1))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn array_hole() {
    let input = r#"
            module.exports = [,];
        "#;
    let expected_output = r#"
Locs:
0. [1:17-20]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Err(
                0,
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(UnexpectedArrayHole(0))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn object_spread() {
    let input = r#"
            module.exports = { x: 'x', ...{ y: 'y' }, z: 'z' };
        "#;
    let expected_output = r#"
Locs:
0. [1:17-50]
1. [1:19-20]
2. [1:22-25]
3. [1:30-40]
4. [1:32-33]
5. [1:35-38]
6. [1:42-43]
7. [1:45-48]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Value(
                ObjSpreadLit(
                    ValueObjSpreadLit {
                        loc: 0,
                        frozen: false,
                        proto: None,
                        elems: [
                            ObjValueSpreadSlice(
                                {
                                    "x": ObjValueField(
                                        (
                                            1,
                                            Value(
                                                StringLit(
                                                    (
                                                        2,
                                                        "x",
                                                    ),
                                                ),
                                            ),
                                            Neutral,
                                        ),
                                    ),
                                },
                            ),
                            ObjValueSpreadElem(
                                Value(
                                    ObjLit(
                                        ValueObjLit {
                                            loc: 3,
                                            frozen: false,
                                            proto: None,
                                            props: {
                                                "y": ObjValueField(
                                                    (
                                                        4,
                                                        Value(
                                                            StringLit(
                                                                (
                                                                    5,
                                                                    "y",
                                                                ),
                                                            ),
                                                        ),
                                                        Neutral,
                                                    ),
                                                ),
                                            },
                                            strictness_kind: Flow,
                                        },
                                    ),
                                ),
                            ),
                            ObjValueSpreadSlice(
                                {
                                    "z": ObjValueField(
                                        (
                                            6,
                                            Value(
                                                StringLit(
                                                    (
                                                        7,
                                                        "z",
                                                    ),
                                                ),
                                            ),
                                            Neutral,
                                        ),
                                    ),
                                },
                            ),
                        ],
                        strictness_kind: Flow,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn reference_expression1() {
    let input = r#"
            module.exports = Number.NaN;
        "#;
    let expected_output = r#"
Locs:
0. [1:17-27]
1. [1:17-23]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Eval(
                PackedEval {
                    loc: 0,
                    packed: Ref(
                        BuiltinRef(
                            PackedRefBuiltin {
                                ref_loc: 1,
                                type_ref: false,
                                name: "Number",
                            },
                        ),
                    ),
                    op: GetProp(
                        "NaN",
                    ),
                    strictness_kind: Flow,
                },
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn reference_expression2() {
    let input = r#"
module.exports = 'x'.length;
        "#;
    let expected_output = r#"
Locs:
0. [1:17-27]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Err(
                0,
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(UnexpectedExpression(0, Member))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn member_expression() {
    let input = r#"
            module.exports = a[0];
        "#;
    let expected_output = r#"
Locs:
0. [1:17-21]
1. [1:17-18]
2. [1:19-20]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Eval(
                PackedEval {
                    loc: 0,
                    packed: Ref(
                        BuiltinRef(
                            PackedRefBuiltin {
                                ref_loc: 1,
                                type_ref: false,
                                name: "a",
                            },
                        ),
                    ),
                    op: GetElem(
                        Value(
                            NumberLit(
                                (
                                    2,
                                    0.0,
                                    "0",
                                ),
                            ),
                        ),
                    ),
                    strictness_kind: Flow,
                },
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn chained_member_expression() {
    // Regression test: chained member expressions like `a.b.c` must nest
    // as `Eval(_, Eval(_, a, GetProp(b)), GetProp(c))` (left-to-right),
    // not `Eval(_, Eval(_, a, GetProp(c)), GetProp(b))` (reversed).
    let input = r#"
            module.exports = a.b.c;
        "#;
    let expected_output = r#"
Locs:
0. [1:17-22]
1. [1:17-20]
2. [1:17-18]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Eval(
                PackedEval {
                    loc: 0,
                    packed: Eval(
                        PackedEval {
                            loc: 1,
                            packed: Ref(
                                BuiltinRef(
                                    PackedRefBuiltin {
                                        ref_loc: 2,
                                        type_ref: false,
                                        name: "a",
                                    },
                                ),
                            ),
                            op: GetProp(
                                "b",
                            ),
                            strictness_kind: Flow,
                        },
                    ),
                    op: GetProp(
                        "c",
                    ),
                    strictness_kind: Flow,
                },
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn arith_expression1() {
    let input = r#"
            module.exports = 6*7;
        "#;
    let expected_output = r#"
Locs:
0. [1:17-20]
1. [1:17-18]
2. [1:19-20]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Eval(
                PackedEval {
                    loc: 0,
                    packed: Value(
                        NumberLit(
                            (
                                1,
                                6.0,
                                "6",
                            ),
                        ),
                    ),
                    op: Arith(
                        (
                            Mult,
                            Value(
                                NumberLit(
                                    (
                                        2,
                                        7.0,
                                        "7",
                                    ),
                                ),
                            ),
                        ),
                    ),
                    strictness_kind: Flow,
                },
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn arith_expression2() {
    let input = r#"
            module.exports = 6+7;
        "#;
    let expected_output = r#"
Locs:
0. [1:17-20]
1. [1:17-18]
2. [1:19-20]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Eval(
                PackedEval {
                    loc: 0,
                    packed: Value(
                        NumberLit(
                            (
                                1,
                                6.0,
                                "6",
                            ),
                        ),
                    ),
                    op: Arith(
                        (
                            Plus,
                            Value(
                                NumberLit(
                                    (
                                        2,
                                        7.0,
                                        "7",
                                    ),
                                ),
                            ),
                        ),
                    ),
                    strictness_kind: Flow,
                },
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn update_expression() {
    let input = r#"
            import {foo} from 'bar';
    export const pre_incr = ++foo;
    export const pre_decr = --foo;
    export const post_incr = foo++;
    export const post_decr = foo--;
        "#;
    let expected_output = r#"
Locs:
0. [1:16-19]
1. [2:13-21]
2. [2:24-29]
3. [2:26-29]
4. [3:13-21]
5. [3:24-29]
6. [3:26-29]
7. [4:13-22]
8. [4:25-30]
9. [4:25-28]
10. [5:13-22]
11. [5:25-30]
12. [5:25-28]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                3,
            ),
            ExportBinding(
                2,
            ),
            ExportBinding(
                1,
            ),
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "post_decr",
                "post_incr",
                "pre_decr",
                "pre_incr",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [
        Userland(
            "bar",
        ),
    ],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 1,
                name: "pre_incr",
                def: Eval(
                    PackedEval {
                        loc: 2,
                        packed: Ref(
                            RemoteRef(
                                PackedRefRemote {
                                    ref_loc: 3,
                                    index: 0,
                                },
                            ),
                        ),
                        op: Update,
                        strictness_kind: Flow,
                    },
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 4,
                name: "pre_decr",
                def: Eval(
                    PackedEval {
                        loc: 5,
                        packed: Ref(
                            RemoteRef(
                                PackedRefRemote {
                                    ref_loc: 6,
                                    index: 0,
                                },
                            ),
                        ),
                        op: Update,
                        strictness_kind: Flow,
                    },
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 7,
                name: "post_incr",
                def: Eval(
                    PackedEval {
                        loc: 8,
                        packed: Ref(
                            RemoteRef(
                                PackedRefRemote {
                                    ref_loc: 9,
                                    index: 0,
                                },
                            ),
                        ),
                        op: Update,
                        strictness_kind: Flow,
                    },
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 10,
                name: "post_decr",
                def: Eval(
                    PackedEval {
                        loc: 11,
                        packed: Ref(
                            RemoteRef(
                                PackedRefRemote {
                                    ref_loc: 12,
                                    index: 0,
                                },
                            ),
                        ),
                        op: Update,
                        strictness_kind: Flow,
                    },
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [
        Import {
            id_loc: 0,
            name: "foo",
            index: 0,
            remote: "foo",
            strictness_kind: Flow,
        },
    ],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn sequence_expression() {
    let input = r#"
            var x;
    export default (x, null);
        "#;
    let expected_output = r#"
Locs:
0. [2:7-14]
1. [2:19-23]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 0,
                    def: Value(
                        NullLit(
                            1,
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn named_class_expression() {
    let input = r#"
            module.exports = class C { };
        "#;
    let expected_output = r#"
Locs:
0. [1:23-24]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Ref(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 0,
                        index: 0,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn named_function_expression() {
    let input = r#"
            module.exports = function foo() { };
        "#;
    let expected_output = r#"
Locs:
0. [1:17-31]
1. [1:26-29]
2. [1:31]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Ref(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 1,
                        index: 0,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "foo",
                async_: false,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Void(
                            2,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn interface_coverage() {
    let input = r#"
            declare interface Foo<X> { }
    declare export class C {
      foo: Foo<any>;
    }
        "#;
    let expected_output = r#"
Locs:
0. [1:26-29]
1. [1:29-32]
2. [1:30-31]
3. [2:21-22]
4. [3:2-5]
5. [3:7-15]
6. [3:7-10]
7. [3:11-14]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                1,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "C",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Interface(
            DefInterface {
                id_loc: 0,
                name: "Foo",
                tparams: Poly(
                    (
                        1,
                        [
                            TParam {
                                name_loc: 2,
                                name: "X",
                                polarity: None,
                                bound: None,
                                default: None,
                                is_const: false,
                            },
                        ],
                    ),
                ),
                def: InterfaceSig {
                    extends: [],
                    props: {},
                    computed_props: [],
                    calls: [],
                    constructs: [],
                    dict: None,
                    abstract_: false,
                    strictness_kind: Flow,
                },
            },
        ),
        DeclareClassBinding(
            DefDeclareClassBinding {
                id_loc: 3,
                name: "C",
                def: DeclareClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    mixins: [],
                    implements: [],
                    static_props: {},
                    own_props: {
                        "foo": InterfaceField(
                            (
                                Some(
                                    4,
                                ),
                                TyRefApp(
                                    PackedTyRefApp {
                                        loc: 5,
                                        name: Unqualified(
                                            LocalRef(
                                                PackedRefLocal {
                                                    ref_loc: 6,
                                                    index: 0,
                                                },
                                            ),
                                        ),
                                        targs: [
                                            Annot(
                                                Any(
                                                    7,
                                                ),
                                            ),
                                        ],
                                    },
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    proto_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    static_calls: [],
                    calls: [],
                    constructs: [],
                    dict: None,
                    static_dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn bound_coverage() {
    let input = r#"
            type Foo = number;
    export type T = <X: Foo> (X) => void;
        "#;
    let expected_output = r#"
Locs:
0. [1:13-16]
1. [1:19-25]
2. [2:12-13]
3. [2:16-36]
4. [2:16-24]
5. [2:17-18]
6. [2:20-23]
7. [2:26-27]
8. [2:32-36]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                1,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "T",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "Foo",
                tparams: Mono,
                body: Annot(
                    Number(
                        1,
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 2,
                custom_error_loc_opt: None,
                name: "T",
                tparams: Mono,
                body: Annot(
                    FunAnnot(
                        (
                            3,
                            FunSig {
                                tparams: Poly(
                                    (
                                        4,
                                        [
                                            TParam {
                                                name_loc: 5,
                                                name: "X",
                                                polarity: None,
                                                bound: Some(
                                                    TyRef(
                                                        Unqualified(
                                                            LocalRef(
                                                                PackedRefLocal {
                                                                    ref_loc: 6,
                                                                    index: 0,
                                                                },
                                                            ),
                                                        ),
                                                    ),
                                                ),
                                                default: None,
                                                is_const: false,
                                            },
                                        ],
                                    ),
                                ),
                                params: [
                                    FunParam {
                                        name: None,
                                        t: Annot(
                                            Bound(
                                                AnnotBound {
                                                    ref_loc: 7,
                                                    name: "X",
                                                },
                                            ),
                                        ),
                                    },
                                ],
                                rest_param: None,
                                this_param: None,
                                return_: Annot(
                                    Void(
                                        8,
                                    ),
                                ),
                                type_guard: None,
                                effect_: ArbitraryEffect,
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn recursive_class_coverage() {
    let input = r#"
            module.exports = class C { x: C; };
        "#;
    let expected_output = r#"
Locs:
0. [1:23-24]
1. [1:27-28]
2. [1:30-31]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Ref(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 0,
                        index: 0,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {
                        "x": ObjValueField(
                            (
                                1,
                                TyRef(
                                    Unqualified(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 2,
                                                index: 0,
                                            },
                                        ),
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn shadowed_class_expression() {
    let input = r#"
            class C { }
    module.exports = class C { }
        "#;
    let expected_output = r#"
Locs:
0. [2:23-24]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Ref(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 0,
                        index: 0,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn frozen_object() {
    let input = r#"
            module.exports = Object.freeze({ foo: 42, bar: 'hello' });
        "#;
    let expected_output = r#"
Locs:
0. [1:31-56]
1. [1:33-36]
2. [1:38-40]
3. [1:42-45]
4. [1:47-54]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Value(
                ObjLit(
                    ValueObjLit {
                        loc: 0,
                        frozen: true,
                        proto: None,
                        props: {
                            "bar": ObjValueField(
                                (
                                    3,
                                    Annot(
                                        SingletonString(
                                            (
                                                4,
                                                "hello",
                                            ),
                                        ),
                                    ),
                                    Positive,
                                ),
                            ),
                            "foo": ObjValueField(
                                (
                                    1,
                                    Annot(
                                        SingletonNumber(
                                            (
                                                2,
                                                42.0,
                                                "42",
                                            ),
                                        ),
                                    ),
                                    Positive,
                                ),
                            ),
                        },
                        strictness_kind: Flow,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn frozen_object_empty() {
    let input = r#"
            module.exports = Object.freeze({});
        "#;
    let expected_output = r#"
Locs:
0. [1:31-33]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Value(
                ObjLit(
                    ValueObjLit {
                        loc: 0,
                        frozen: true,
                        proto: None,
                        props: {},
                        strictness_kind: Flow,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn fbt_empty_open_close() {
    let input = r#"
            module.exports = <fbt></fbt>;
        "#;
    let expected_output = r#"
Locs:
0. [1:18-21]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            TyRef(
                Unqualified(
                    BuiltinRef(
                        PackedRefBuiltin {
                            ref_loc: 0,
                            type_ref: true,
                            name: "FbtElement",
                        },
                    ),
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_options(
            input,
            Some(|opts: &mut TypeSigOptions| {
                opts.facebook_fbt = Some(FlowSmolStr::new("FbtElement"));
            })
        ))
    )
}

#[test]
fn fbt_empty_open() {
    let input = r#"
            module.exports = <fbt/>;
        "#;
    let expected_output = r#"
Locs:
0. [1:18-21]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            TyRef(
                Unqualified(
                    BuiltinRef(
                        PackedRefBuiltin {
                            ref_loc: 0,
                            type_ref: true,
                            name: "FbtElement",
                        },
                    ),
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_options(
            input,
            Some(|opts: &mut TypeSigOptions| {
                opts.facebook_fbt = Some(FlowSmolStr::new("FbtElement"));
            })
        ))
    )
}

#[test]
fn fbt_with_child() {
    let input = r#"
            function foo() {}
    module.exports = <fbt desc={foo()}></fbt>;
        "#;
    let expected_output = r#"
Locs:
0. [2:18-21]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            TyRef(
                Unqualified(
                    BuiltinRef(
                        PackedRefBuiltin {
                            ref_loc: 0,
                            type_ref: true,
                            name: "FbtElement",
                        },
                    ),
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_options(
            input,
            Some(|opts: &mut TypeSigOptions| {
                opts.facebook_fbt = Some(FlowSmolStr::new("FbtElement"));
            })
        ))
    )
}

#[test]
fn keymirror() {
    let input = r#"
            module.exports = keyMirror({
      a: null,
      b: null,
    })
        "#;
    let expected_output = r#"
Locs:
0. [1:35-4:1]
1. [2:2-3]
2. [3:2-3]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Value(
                ObjLit(
                    ValueObjLit {
                        loc: 0,
                        frozen: false,
                        proto: None,
                        props: {
                            "a": ObjValueField(
                                (
                                    1,
                                    Annot(
                                        SingletonString(
                                            (
                                                1,
                                                "a",
                                            ),
                                        ),
                                    ),
                                    Neutral,
                                ),
                            ),
                            "b": ObjValueField(
                                (
                                    2,
                                    Annot(
                                        SingletonString(
                                            (
                                                2,
                                                "b",
                                            ),
                                        ),
                                    ),
                                    Neutral,
                                ),
                            ),
                        },
                        strictness_kind: Flow,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_options(
            input,
            Some(|opts: &mut TypeSigOptions| {
                opts.facebook_key_mirror = true;
            })
        ))
    )
}

#[test]
fn jsx_div() {
    let input = r#"
            module.exports = <div></div>;
        "#;
    let expected_output = r#"
Locs:
0. [1:17-28]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Err(
                0,
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(UnexpectedExpression(0, JSXElement))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn function_return() {
    let input = r#"
            var n = false;
    export function foo<X: typeof n>(x: X) { return 1; };
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [2:7-38]
2. [2:16-19]
3. [2:19-32]
4. [2:20-21]
5. [2:23-31]
6. [2:30-31]
7. [2:36-37]
8. [2:38]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                1,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "foo",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "n",
                def: Err(
                    0,
                ),
            },
        ),
        FunBinding(
            DefFunBinding {
                id_loc: 2,
                name: "foo",
                async_: false,
                generator: false,
                fn_loc: 1,
                def: FunSig {
                    tparams: Poly(
                        (
                            3,
                            [
                                TParam {
                                    name_loc: 4,
                                    name: "X",
                                    polarity: None,
                                    bound: Some(
                                        Annot(
                                            Typeof(
                                                AnnotTypeof {
                                                    loc: 5,
                                                    qname: [
                                                        "n",
                                                    ],
                                                    t: Ref(
                                                        LocalRef(
                                                            PackedRefLocal {
                                                                ref_loc: 6,
                                                                index: 0,
                                                            },
                                                        ),
                                                    ),
                                                    targs: None,
                                                },
                                            ),
                                        ),
                                    ),
                                    default: None,
                                    is_const: false,
                                },
                            ],
                        ),
                    ),
                    params: [
                        FunParam {
                            name: Some(
                                "x",
                            ),
                            t: Annot(
                                Bound(
                                    AnnotBound {
                                        ref_loc: 7,
                                        name: "X",
                                    },
                                ),
                            ),
                        },
                    ],
                    rest_param: None,
                    this_param: None,
                    return_: Err(
                        8,
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(0, VariableDefinition { name: "n" }))
SigError(ExpectedAnnotation(8, FunctionReturn))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn function_return_2() {
    let input = r#"
            var n = false;
    export function bar(x: (typeof n) => void) { return 1; };
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [2:7-42]
2. [2:16-19]
3. [2:23-41]
4. [2:24-32]
5. [2:31-32]
6. [2:37-41]
7. [2:42]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                1,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "bar",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "n",
                def: Err(
                    0,
                ),
            },
        ),
        FunBinding(
            DefFunBinding {
                id_loc: 2,
                name: "bar",
                async_: false,
                generator: false,
                fn_loc: 1,
                def: FunSig {
                    tparams: Mono,
                    params: [
                        FunParam {
                            name: Some(
                                "x",
                            ),
                            t: Annot(
                                FunAnnot(
                                    (
                                        3,
                                        FunSig {
                                            tparams: Mono,
                                            params: [
                                                FunParam {
                                                    name: None,
                                                    t: Annot(
                                                        Typeof(
                                                            AnnotTypeof {
                                                                loc: 4,
                                                                qname: [
                                                                    "n",
                                                                ],
                                                                t: Ref(
                                                                    LocalRef(
                                                                        PackedRefLocal {
                                                                            ref_loc: 5,
                                                                            index: 0,
                                                                        },
                                                                    ),
                                                                ),
                                                                targs: None,
                                                            },
                                                        ),
                                                    ),
                                                },
                                            ],
                                            rest_param: None,
                                            this_param: None,
                                            return_: Annot(
                                                Void(
                                                    6,
                                                ),
                                            ),
                                            type_guard: None,
                                            effect_: ArbitraryEffect,
                                            strictness_kind: Flow,
                                        },
                                    ),
                                ),
                            ),
                        },
                    ],
                    rest_param: None,
                    this_param: None,
                    return_: Err(
                        7,
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(0, VariableDefinition { name: "n" }))
SigError(ExpectedAnnotation(7, FunctionReturn))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn function_statics() {
    let input = r#"
            function bar(): void { };
    const x = 42;
    bar.x = x;
    module.exports = bar;
        "#;
    let expected_output = r#"
Locs:
0. [1:8-28]
1. [1:17-20]
2. [1:24-28]
3. [2:6-7]
4. [2:10-12]
5. [3:4-5]
6. [3:8-9]
7. [4:17-20]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Ref(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 7,
                        index: 0,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "bar",
                async_: false,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Void(
                            2,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {
                    "x": (
                        5,
                        Ref(
                            LocalRef(
                                PackedRefLocal {
                                    ref_loc: 6,
                                    index: 1,
                                },
                            ),
                        ),
                        Neutral,
                    ),
                },
                namespace_types: {},
            },
        ),
        Variable(
            DefVariable {
                id_loc: 3,
                name: "x",
                def: Value(
                    NumberLit(
                        (
                            4,
                            42.0,
                            "42",
                        ),
                    ),
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn function_statics_conditional() {
    let input = r#"
            export function bar(): void { };
    declare var b: boolean;
    if (b) {
      bar.x = 42;
    }
        "#;
    let expected_output = r#"
Locs:
0. [1:15-35]
1. [1:24-27]
2. [1:31-35]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "bar",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "bar",
                async_: false,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Void(
                            2,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn function_predicates() {
    let input = r#"
            class A {};
    export function foo(x: mixed): boolean %checks {
      return x === new A;
    }
        "#;
    let expected_output = r#"
Locs:
0. [2:7-46]
1. [2:16-19]
2. [2:23-28]
3. [2:31-38]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "foo",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "foo",
                async_: false,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Mono,
                    params: [
                        FunParam {
                            name: Some(
                                "x",
                            ),
                            t: Annot(
                                Mixed(
                                    2,
                                ),
                            ),
                        },
                    ],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Boolean(
                            3,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn async_function_1() {
    let input = r#"
            async function foo() {};
    module.exports = foo;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-28]
1. [1:23-26]
2. [1:28]
3. [2:17-20]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Ref(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 3,
                        index: 0,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "foo",
                async_: true,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    this_param: None,
                    return_: AsyncVoidReturn(
                        2,
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn async_function_2() {
    let input = r#"
            async function foo() { return 1; };
    module.exports = foo;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-28]
1. [1:23-26]
2. [1:28]
3. [2:17-20]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Ref(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 3,
                        index: 0,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "foo",
                async_: true,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    this_param: None,
                    return_: Err(
                        2,
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(2, FunctionReturn))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn async_function_3() {
    let input = r#"
            module.exports = async () => await 1;
        "#;
    let expected_output = r#"
Locs:
0. [1:17-36]
1. [1:25]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Value(
                FunExpr(
                    ValueFunExpr {
                        loc: 0,
                        async_: true,
                        generator: false,
                        def: FunSig {
                            tparams: Mono,
                            params: [],
                            rest_param: None,
                            this_param: None,
                            return_: Err(
                                1,
                            ),
                            type_guard: None,
                            effect_: ArbitraryEffect,
                            strictness_kind: Flow,
                        },
                        statics: {},
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
Errors:
SigError(ExpectedAnnotation(1, FunctionReturn))
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn type_spread() {
    let input = r#"
            type A = { a: string };
    type B = { b: number };
    export type C = { ...A, ...B, c: null }
        "#;
    let expected_output = r#"
Locs:
0. [1:13-14]
1. [1:17-30]
2. [1:19-20]
3. [1:22-28]
4. [2:5-6]
5. [2:9-22]
6. [2:11-12]
7. [2:14-20]
8. [3:12-13]
9. [3:16-39]
10. [3:21-22]
11. [3:27-28]
12. [3:30-31]
13. [3:33-37]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                2,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "C",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "A",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 1,
                            obj_kind: ExactObj,
                            props: {
                                "a": ObjAnnotField(
                                    (
                                        2,
                                        Annot(
                                            String(
                                                3,
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                            },
                            computed_props: [],
                            proto: ObjAnnotImplicitProto,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 4,
                custom_error_loc_opt: None,
                name: "B",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 5,
                            obj_kind: ExactObj,
                            props: {
                                "b": ObjAnnotField(
                                    (
                                        6,
                                        Annot(
                                            Number(
                                                7,
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                            },
                            computed_props: [],
                            proto: ObjAnnotImplicitProto,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 8,
                custom_error_loc_opt: None,
                name: "C",
                tparams: Mono,
                body: Annot(
                    ObjSpreadAnnot(
                        AnnotObjSpreadAnnot {
                            loc: 9,
                            exact: true,
                            elems: [
                                ObjSpreadAnnotElem(
                                    TyRef(
                                        Unqualified(
                                            LocalRef(
                                                PackedRefLocal {
                                                    ref_loc: 10,
                                                    index: 0,
                                                },
                                            ),
                                        ),
                                    ),
                                ),
                                ObjSpreadAnnotElem(
                                    TyRef(
                                        Unqualified(
                                            LocalRef(
                                                PackedRefLocal {
                                                    ref_loc: 11,
                                                    index: 1,
                                                },
                                            ),
                                        ),
                                    ),
                                ),
                                ObjSpreadAnnotSlice(
                                    ObjSpreadAnnotSliceData {
                                        dict: None,
                                        props: {
                                            "c": ObjAnnotField(
                                                (
                                                    12,
                                                    Annot(
                                                        Null(
                                                            13,
                                                        ),
                                                    ),
                                                    Neutral,
                                                ),
                                            ),
                                        },
                                        computed_props: [],
                                    },
                                ),
                            ],
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn inline_interface() {
    let input = r#"
            type A = interface {};
    export type B = interface extends A { p: string };
        "#;
    let expected_output = r#"
Locs:
0. [1:13-14]
1. [1:17-29]
2. [2:12-13]
3. [2:16-49]
4. [2:34-35]
5. [2:38-39]
6. [2:41-47]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                1,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "B",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "A",
                tparams: Mono,
                body: Annot(
                    InlineInterface(
                        (
                            1,
                            InterfaceSig {
                                extends: [],
                                props: {},
                                computed_props: [],
                                calls: [],
                                constructs: [],
                                dict: None,
                                abstract_: false,
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 2,
                custom_error_loc_opt: None,
                name: "B",
                tparams: Mono,
                body: Annot(
                    InlineInterface(
                        (
                            3,
                            InterfaceSig {
                                extends: [
                                    TyRef(
                                        Unqualified(
                                            LocalRef(
                                                PackedRefLocal {
                                                    ref_loc: 4,
                                                    index: 0,
                                                },
                                            ),
                                        ),
                                    ),
                                ],
                                props: {
                                    "p": InterfaceField(
                                        (
                                            Some(
                                                5,
                                            ),
                                            Annot(
                                                String(
                                                    6,
                                                ),
                                            ),
                                            Neutral,
                                        ),
                                    ),
                                },
                                computed_props: [],
                                calls: [],
                                constructs: [],
                                dict: None,
                                abstract_: false,
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

// In a TypeScript file an object type carrying `new(): T` has no `ObjAnnot`
// slot to hold the construct sig, so it is lowered to an inline interface. A
// quoted `"new"()` stays an ordinary method on an `ObjAnnot`. The Flow-file
// counterpart is [object_annot_construct_sig_flow_file].
#[test]
fn object_annot_construct_sig() {
    let input = r#"
    export type A = { new (x: string): string };
    export type B = { "new"(x: string): string };
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-43]
2. [1:18-41]
3. [1:26-32]
4. [1:35-41]
5. [2:12-13]
6. [2:16-44]
7. [2:18-42]
8. [2:18-23]
9. [2:27-33]
10. [2:36-42]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
            ExportTypeBinding(
                1,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "A",
                "B",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: TypeScriptLoose,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "A",
                tparams: Mono,
                body: Annot(
                    InlineInterface(
                        (
                            1,
                            InterfaceSig {
                                extends: [],
                                props: {},
                                computed_props: [],
                                calls: [],
                                constructs: [
                                    Annot(
                                        FunAnnot(
                                            (
                                                2,
                                                FunSig {
                                                    tparams: Mono,
                                                    params: [
                                                        FunParam {
                                                            name: Some(
                                                                "x",
                                                            ),
                                                            t: Annot(
                                                                String(
                                                                    3,
                                                                ),
                                                            ),
                                                        },
                                                    ],
                                                    rest_param: None,
                                                    this_param: None,
                                                    return_: Annot(
                                                        String(
                                                            4,
                                                        ),
                                                    ),
                                                    type_guard: None,
                                                    effect_: ArbitraryEffect,
                                                    strictness_kind: TypeScriptLoose,
                                                },
                                            ),
                                        ),
                                    ),
                                ],
                                dict: None,
                                abstract_: false,
                                strictness_kind: TypeScriptLoose,
                            },
                        ),
                    ),
                ),
                strictness_kind: TypeScriptLoose,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 5,
                custom_error_loc_opt: None,
                name: "B",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 6,
                            obj_kind: ExactObj,
                            props: {
                                "new": ObjAnnotMethod(
                                    ObjAnnotMethodData {
                                        id_loc: 8,
                                        fn_loc: 7,
                                        def: FunSig {
                                            tparams: Mono,
                                            params: [
                                                FunParam {
                                                    name: Some(
                                                        "x",
                                                    ),
                                                    t: Annot(
                                                        String(
                                                            9,
                                                        ),
                                                    ),
                                                },
                                            ],
                                            rest_param: None,
                                            this_param: None,
                                            return_: Annot(
                                                String(
                                                    10,
                                                ),
                                            ),
                                            type_guard: None,
                                            effect_: ArbitraryEffect,
                                            strictness_kind: TypeScriptLoose,
                                        },
                                    },
                                ),
                            },
                            computed_props: [],
                            proto: ObjAnnotImplicitProto,
                            strictness_kind: TypeScriptLoose,
                        },
                    ),
                ),
                strictness_kind: TypeScriptLoose,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_options(
            input,
            Some(|opts: &mut TypeSigOptions| {
                opts.is_ts_file = true;
                opts.is_dts_file = true;
            })
        ))
    )
}

// The same annotation in a Flow file keeps its long-standing meaning: an
// `ObjAnnot` with a method named `new`.
#[test]
fn object_annot_construct_sig_flow_file() {
    let input = r#"
    export type A = { new (x: string): string };
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-43]
2. [1:18-41]
3. [1:18-21]
4. [1:26-32]
5. [1:35-41]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "A",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "A",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 1,
                            obj_kind: ExactObj,
                            props: {
                                "new": ObjAnnotMethod(
                                    ObjAnnotMethodData {
                                        id_loc: 3,
                                        fn_loc: 2,
                                        def: FunSig {
                                            tparams: Mono,
                                            params: [
                                                FunParam {
                                                    name: Some(
                                                        "x",
                                                    ),
                                                    t: Annot(
                                                        String(
                                                            4,
                                                        ),
                                                    ),
                                                },
                                            ],
                                            rest_param: None,
                                            this_param: None,
                                            return_: Annot(
                                                String(
                                                    5,
                                                ),
                                            ),
                                            type_guard: None,
                                            effect_: ArbitraryEffect,
                                            strictness_kind: Flow,
                                        },
                                    },
                                ),
                            },
                            computed_props: [],
                            proto: ObjAnnotImplicitProto,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn object_annot_optional() {
    let input = r#"
            export type A = { p?: string };
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-30]
2. [1:18-19]
3. [1:22-28]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "A",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "A",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 1,
                            obj_kind: ExactObj,
                            props: {
                                "p": ObjAnnotField(
                                    (
                                        2,
                                        Annot(
                                            Optional(
                                                Annot(
                                                    String(
                                                        3,
                                                    ),
                                                ),
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                            },
                            computed_props: [],
                            proto: ObjAnnotImplicitProto,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn interface_optional() {
    let input = r#"
            export interface I { p?: string }
        "#;
    let expected_output = r#"
Locs:
0. [1:17-18]
1. [1:21-22]
2. [1:25-31]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "I",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Interface(
            DefInterface {
                id_loc: 0,
                name: "I",
                tparams: Mono,
                def: InterfaceSig {
                    extends: [],
                    props: {
                        "p": InterfaceField(
                            (
                                Some(
                                    1,
                                ),
                                Annot(
                                    Optional(
                                        Annot(
                                            String(
                                                2,
                                            ),
                                        ),
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_props: [],
                    calls: [],
                    constructs: [],
                    dict: None,
                    abstract_: false,
                    strictness_kind: Flow,
                },
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn interface_method() {
    let input = r#"
            export interface I { m(): void }
        "#;
    let expected_output = r#"
Locs:
0. [1:17-18]
1. [1:21-30]
2. [1:21-22]
3. [1:26-30]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "I",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Interface(
            DefInterface {
                id_loc: 0,
                name: "I",
                tparams: Mono,
                def: InterfaceSig {
                    extends: [],
                    props: {
                        "m": InterfaceMethod(
                            [
                                (
                                    2,
                                    1,
                                    FunSig {
                                        tparams: Mono,
                                        params: [],
                                        rest_param: None,
                                        this_param: None,
                                        return_: Annot(
                                            Void(
                                                3,
                                            ),
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                ),
                            ],
                        ),
                    },
                    computed_props: [],
                    calls: [],
                    constructs: [],
                    dict: None,
                    abstract_: false,
                    strictness_kind: Flow,
                },
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn interface_indexer() {
    let input = r#"
            export interface I {[key: string]: number}
        "#;
    let expected_output = r#"
Locs:
0. [1:17-18]
1. [1:26-32]
2. [1:35-41]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "I",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Interface(
            DefInterface {
                id_loc: 0,
                name: "I",
                tparams: Mono,
                def: InterfaceSig {
                    extends: [],
                    props: {},
                    computed_props: [],
                    calls: [],
                    constructs: [],
                    dict: Some(
                        ObjAnnotDict {
                            name: Some(
                                "key",
                            ),
                            polarity: Neutral,
                            key: Annot(
                                String(
                                    1,
                                ),
                            ),
                            value: Annot(
                                Number(
                                    2,
                                ),
                            ),
                        },
                    ),
                    abstract_: false,
                    strictness_kind: Flow,
                },
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn object_annot_method() {
    let input = r#"
            export type A = { m(): void };
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-29]
2. [1:18-27]
3. [1:18-19]
4. [1:23-27]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "A",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "A",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 1,
                            obj_kind: ExactObj,
                            props: {
                                "m": ObjAnnotMethod(
                                    ObjAnnotMethodData {
                                        id_loc: 3,
                                        fn_loc: 2,
                                        def: FunSig {
                                            tparams: Mono,
                                            params: [],
                                            rest_param: None,
                                            this_param: None,
                                            return_: Annot(
                                                Void(
                                                    4,
                                                ),
                                            ),
                                            type_guard: None,
                                            effect_: ArbitraryEffect,
                                            strictness_kind: Flow,
                                        },
                                    },
                                ),
                            },
                            computed_props: [],
                            proto: ObjAnnotImplicitProto,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn object_annot_call_poly() {
    let input = r#"
            export type A = { <T>(X): X };
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-29]
2. [1:18-27]
3. [1:18-21]
4. [1:19-20]
5. [1:22-23]
6. [1:26-27]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "A",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "A",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 1,
                            obj_kind: ExactObj,
                            props: {},
                            computed_props: [],
                            proto: ObjAnnotCallable(
                                [
                                    Annot(
                                        FunAnnot(
                                            (
                                                2,
                                                FunSig {
                                                    tparams: Poly(
                                                        (
                                                            3,
                                                            [
                                                                TParam {
                                                                    name_loc: 4,
                                                                    name: "T",
                                                                    polarity: None,
                                                                    bound: None,
                                                                    default: None,
                                                                    is_const: false,
                                                                },
                                                            ],
                                                        ),
                                                    ),
                                                    params: [
                                                        FunParam {
                                                            name: None,
                                                            t: TyRef(
                                                                Unqualified(
                                                                    BuiltinRef(
                                                                        PackedRefBuiltin {
                                                                            ref_loc: 5,
                                                                            type_ref: true,
                                                                            name: "X",
                                                                        },
                                                                    ),
                                                                ),
                                                            ),
                                                        },
                                                    ],
                                                    rest_param: None,
                                                    this_param: None,
                                                    return_: TyRef(
                                                        Unqualified(
                                                            BuiltinRef(
                                                                PackedRefBuiltin {
                                                                    ref_loc: 6,
                                                                    type_ref: true,
                                                                    name: "X",
                                                                },
                                                            ),
                                                        ),
                                                    ),
                                                    type_guard: None,
                                                    effect_: ArbitraryEffect,
                                                    strictness_kind: Flow,
                                                },
                                            ),
                                        ),
                                    ),
                                ],
                            ),
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn object_annot_multiple_call() {
    let input = r#"
            export type A = { (): number, (): string };
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-42]
2. [1:18-28]
3. [1:22-28]
4. [1:30-40]
5. [1:34-40]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "A",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "A",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 1,
                            obj_kind: ExactObj,
                            props: {},
                            computed_props: [],
                            proto: ObjAnnotCallable(
                                [
                                    Annot(
                                        FunAnnot(
                                            (
                                                2,
                                                FunSig {
                                                    tparams: Mono,
                                                    params: [],
                                                    rest_param: None,
                                                    this_param: None,
                                                    return_: Annot(
                                                        Number(
                                                            3,
                                                        ),
                                                    ),
                                                    type_guard: None,
                                                    effect_: ArbitraryEffect,
                                                    strictness_kind: Flow,
                                                },
                                            ),
                                        ),
                                    ),
                                    Annot(
                                        FunAnnot(
                                            (
                                                4,
                                                FunSig {
                                                    tparams: Mono,
                                                    params: [],
                                                    rest_param: None,
                                                    this_param: None,
                                                    return_: Annot(
                                                        String(
                                                            5,
                                                        ),
                                                    ),
                                                    type_guard: None,
                                                    effect_: ArbitraryEffect,
                                                    strictness_kind: Flow,
                                                },
                                            ),
                                        ),
                                    ),
                                ],
                            ),
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn destruct_object_shared() {
    let input = r#"
            export const {a, b: {c, d}} = e;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [1:17-18]
2. [1:21-22]
3. [1:24-25]
4. [1:30-31]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
            ExportBinding(
                1,
            ),
            ExportBinding(
                2,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "a",
                "c",
                "d",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "a",
                def: Pattern(
                    1,
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 2,
                name: "c",
                def: Pattern(
                    3,
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 3,
                name: "d",
                def: Pattern(
                    4,
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [
        Ref(
            BuiltinRef(
                PackedRefBuiltin {
                    ref_loc: 4,
                    type_ref: false,
                    name: "e",
                },
            ),
        ),
    ],
    dirty_pattern_defs: [],
    patterns: [
        PDef(
            0,
        ),
        PropP {
            id_loc: 0,
            name: "a",
            def: 0,
            strictness_kind: Flow,
        },
        PropP {
            id_loc: 1,
            name: "b",
            def: 0,
            strictness_kind: Flow,
        },
        PropP {
            id_loc: 2,
            name: "c",
            def: 2,
            strictness_kind: Flow,
        },
        PropP {
            id_loc: 3,
            name: "d",
            def: 2,
            strictness_kind: Flow,
        },
    ],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn destruct_array_shared() {
    let input = r#"
            export const [a, b, {c, d}] = e;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [1:17-18]
2. [1:20-26]
3. [1:21-22]
4. [1:24-25]
5. [1:30-31]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
            ExportBinding(
                1,
            ),
            ExportBinding(
                2,
            ),
            ExportBinding(
                3,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "a",
                "b",
                "c",
                "d",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "a",
                def: Pattern(
                    1,
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 1,
                name: "b",
                def: Pattern(
                    2,
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 3,
                name: "c",
                def: Pattern(
                    4,
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 4,
                name: "d",
                def: Pattern(
                    5,
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [
        Ref(
            BuiltinRef(
                PackedRefBuiltin {
                    ref_loc: 5,
                    type_ref: false,
                    name: "e",
                },
            ),
        ),
    ],
    dirty_pattern_defs: [],
    patterns: [
        PDef(
            0,
        ),
        IndexP {
            loc: 0,
            i: 0,
            def: 0,
            strictness_kind: Flow,
        },
        IndexP {
            loc: 1,
            i: 1,
            def: 0,
            strictness_kind: Flow,
        },
        IndexP {
            loc: 2,
            i: 2,
            def: 0,
            strictness_kind: Flow,
        },
        PropP {
            id_loc: 3,
            name: "c",
            def: 3,
            strictness_kind: Flow,
        },
        PropP {
            id_loc: 4,
            name: "d",
            def: 3,
            strictness_kind: Flow,
        },
    ],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn tuple_annot() {
    let input = r#"
            export type A = [string, number];
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-32]
2. [1:17-23]
3. [1:25-31]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "A",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "A",
                tparams: Mono,
                body: Annot(
                    Tuple(
                        AnnotTuple {
                            loc: 1,
                            elems: [
                                TupleElement(
                                    TupleElementData {
                                        loc: 2,
                                        name: None,
                                        t: Annot(
                                            String(
                                                2,
                                            ),
                                        ),
                                        polarity: Neutral,
                                        optional: false,
                                    },
                                ),
                                TupleElement(
                                    TupleElementData {
                                        loc: 3,
                                        name: None,
                                        t: Annot(
                                            Number(
                                                3,
                                            ),
                                        ),
                                        polarity: Neutral,
                                        optional: false,
                                    },
                                ),
                            ],
                            inexact: false,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn tuple_annot_labeled() {
    let input = r#"
            export type A = [foo: string, bar: number];
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-42]
2. [1:17-28]
3. [1:22-28]
4. [1:30-41]
5. [1:35-41]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "A",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "A",
                tparams: Mono,
                body: Annot(
                    Tuple(
                        AnnotTuple {
                            loc: 1,
                            elems: [
                                TupleElement(
                                    TupleElementData {
                                        loc: 2,
                                        name: Some(
                                            "foo",
                                        ),
                                        t: Annot(
                                            String(
                                                3,
                                            ),
                                        ),
                                        polarity: Neutral,
                                        optional: false,
                                    },
                                ),
                                TupleElement(
                                    TupleElementData {
                                        loc: 4,
                                        name: Some(
                                            "bar",
                                        ),
                                        t: Annot(
                                            Number(
                                                5,
                                            ),
                                        ),
                                        polarity: Neutral,
                                        optional: false,
                                    },
                                ),
                            ],
                            inexact: false,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn tuple_annot_variance() {
    let input = r#"
            export type A = [+foo: string, -bar: number];
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-44]
2. [1:17-29]
3. [1:23-29]
4. [1:31-43]
5. [1:37-43]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "A",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "A",
                tparams: Mono,
                body: Annot(
                    Tuple(
                        AnnotTuple {
                            loc: 1,
                            elems: [
                                TupleElement(
                                    TupleElementData {
                                        loc: 2,
                                        name: Some(
                                            "foo",
                                        ),
                                        t: Annot(
                                            String(
                                                3,
                                            ),
                                        ),
                                        polarity: Positive,
                                        optional: false,
                                    },
                                ),
                                TupleElement(
                                    TupleElementData {
                                        loc: 4,
                                        name: Some(
                                            "bar",
                                        ),
                                        t: Annot(
                                            Number(
                                                5,
                                            ),
                                        ),
                                        polarity: Negative,
                                        optional: false,
                                    },
                                ),
                            ],
                            inexact: false,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn tuple_annot_inexact() {
    let input = r#"
            export type T = [...];
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-21]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "T",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "T",
                tparams: Mono,
                body: Annot(
                    Tuple(
                        AnnotTuple {
                            loc: 1,
                            elems: [],
                            inexact: true,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn cycle() {
    let input = r#"
            export type A = { p: ?B };
    export type B = { p: ?A };
        "#;
    let expected_output = r#"
Locs:
0. [1:20-21]
1. [1:24-33]
2. [1:26-27]
3. [1:29-31]
4. [1:30-31]
5. [2:12-13]
6. [2:16-25]
7. [2:18-19]
8. [2:21-23]
9. [2:22-23]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
            ExportTypeBinding(
                1,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "A",
                "B",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "A",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 1,
                            obj_kind: ExactObj,
                            props: {
                                "p": ObjAnnotField(
                                    (
                                        2,
                                        Annot(
                                            Maybe(
                                                (
                                                    3,
                                                    TyRef(
                                                        Unqualified(
                                                            LocalRef(
                                                                PackedRefLocal {
                                                                    ref_loc: 4,
                                                                    index: 1,
                                                                },
                                                            ),
                                                        ),
                                                    ),
                                                ),
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                            },
                            computed_props: [],
                            proto: ObjAnnotImplicitProto,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 5,
                custom_error_loc_opt: None,
                name: "B",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 6,
                            obj_kind: ExactObj,
                            props: {
                                "p": ObjAnnotField(
                                    (
                                        7,
                                        Annot(
                                            Maybe(
                                                (
                                                    8,
                                                    TyRef(
                                                        Unqualified(
                                                            LocalRef(
                                                                PackedRefLocal {
                                                                    ref_loc: 9,
                                                                    index: 0,
                                                                },
                                                            ),
                                                        ),
                                                    ),
                                                ),
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                            },
                            computed_props: [],
                            proto: ObjAnnotImplicitProto,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn typeof_loc() {
    let input = r#"
            export var a: typeof o.p.q;
        "#;
    let expected_output = r#"
Locs:
0. [1:11-12]
1. [1:14-26]
2. [1:21-22]
3. [1:23-24]
4. [1:25-26]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "a",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "a",
                def: Annot(
                    Typeof(
                        AnnotTypeof {
                            loc: 1,
                            qname: [
                                "o",
                                "p",
                                "q",
                            ],
                            t: Eval(
                                PackedEval {
                                    loc: 4,
                                    packed: Eval(
                                        PackedEval {
                                            loc: 3,
                                            packed: Ref(
                                                BuiltinRef(
                                                    PackedRefBuiltin {
                                                        ref_loc: 2,
                                                        type_ref: false,
                                                        name: "o",
                                                    },
                                                ),
                                            ),
                                            op: GetProp(
                                                "p",
                                            ),
                                            strictness_kind: Flow,
                                        },
                                    ),
                                    op: GetProp(
                                        "q",
                                    ),
                                    strictness_kind: Flow,
                                },
                            ),
                            targs: None,
                        },
                    ),
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn qualified_generic_typeapp_loc() {
    let input = r#"
            declare export var a: O.P.Q<T>;
        "#;
    let expected_output = r#"
Locs:
0. [1:19-20]
1. [1:22-30]
2. [1:22-27]
3. [1:22-25]
4. [1:22-23]
5. [1:24-25]
6. [1:26-27]
7. [1:28-29]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "a",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "a",
                def: TyRefApp(
                    PackedTyRefApp {
                        loc: 1,
                        name: Qualified(
                            TyRefQualified {
                                loc: 2,
                                id_loc: 6,
                                name: "Q",
                                qualification: Qualified(
                                    TyRefQualified {
                                        loc: 3,
                                        id_loc: 5,
                                        name: "P",
                                        qualification: Unqualified(
                                            BuiltinRef(
                                                PackedRefBuiltin {
                                                    ref_loc: 4,
                                                    type_ref: true,
                                                    name: "O",
                                                },
                                            ),
                                        ),
                                        strictness_kind: Flow,
                                    },
                                ),
                                strictness_kind: Flow,
                            },
                        ),
                        targs: [
                            TyRef(
                                Unqualified(
                                    BuiltinRef(
                                        PackedRefBuiltin {
                                            ref_loc: 7,
                                            type_ref: true,
                                            name: "T",
                                        },
                                    ),
                                ),
                            ),
                        ],
                    },
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_ref_renaming() {
    let input = r#"
            declare var a: string;
    export {a as b};
        "#;
    let expected_output = r#"
Locs:
0. [1:20-21]
1. [1:23-29]
2. [2:13-14]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportRef(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 2,
                        index: 0,
                    },
                ),
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "b",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "a",
                def: Annot(
                    String(
                        1,
                    ),
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn intersection_annot() {
    let input = r#"
            declare export var a: string & number & null;
        "#;
    let expected_output = r#"
Locs:
0. [1:19-20]
1. [1:22-44]
2. [1:22-28]
3. [1:31-37]
4. [1:40-44]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "a",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "a",
                def: Annot(
                    Intersection(
                        AnnotIntersection {
                            loc: 1,
                            t0: Annot(
                                String(
                                    2,
                                ),
                            ),
                            t1: Annot(
                                Number(
                                    3,
                                ),
                            ),
                            ts: [
                                Annot(
                                    Null(
                                        4,
                                    ),
                                ),
                            ],
                        },
                    ),
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn class_extends() {
    let input = r#"
            declare class C {};
    const M = {C};
    export class C1 extends C {};
    export class C2 extends M.C {};
    declare export class C3 extends C {};
    declare export class C4 extends M.C {};
        "#;
    let expected_output = r#"
Locs:
0. [1:22-23]
1. [2:6-7]
2. [2:10-13]
3. [2:11-12]
4. [3:13-15]
5. [3:24-25]
6. [4:13-15]
7. [4:24-27]
8. [4:24-25]
9. [5:21-23]
10. [5:32-33]
11. [6:21-23]
12. [6:32-35]
13. [6:32-33]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                2,
            ),
            ExportBinding(
                3,
            ),
            ExportBinding(
                4,
            ),
            ExportBinding(
                5,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "C1",
                "C2",
                "C3",
                "C4",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        DeclareClassBinding(
            DefDeclareClassBinding {
                id_loc: 0,
                name: "C",
                def: DeclareClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    mixins: [],
                    implements: [],
                    static_props: {},
                    own_props: {},
                    proto_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    static_calls: [],
                    calls: [],
                    constructs: [],
                    dict: None,
                    static_dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
        Variable(
            DefVariable {
                id_loc: 1,
                name: "M",
                def: Value(
                    ObjLit(
                        ValueObjLit {
                            loc: 2,
                            frozen: false,
                            proto: None,
                            props: {
                                "C": ObjValueField(
                                    (
                                        3,
                                        Ref(
                                            LocalRef(
                                                PackedRefLocal {
                                                    ref_loc: 3,
                                                    index: 0,
                                                },
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                            },
                            strictness_kind: Flow,
                        },
                    ),
                ),
            },
        ),
        ClassBinding(
            DefClassBinding {
                id_loc: 4,
                name: "C1",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassExplicitExtends(
                        (
                            5,
                            Ref(
                                LocalRef(
                                    PackedRefLocal {
                                        ref_loc: 5,
                                        index: 0,
                                    },
                                ),
                            ),
                        ),
                    ),
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
        ClassBinding(
            DefClassBinding {
                id_loc: 6,
                name: "C2",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassExplicitExtends(
                        (
                            7,
                            Eval(
                                PackedEval {
                                    loc: 7,
                                    packed: Ref(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 8,
                                                index: 1,
                                            },
                                        ),
                                    ),
                                    op: GetProp(
                                        "C",
                                    ),
                                    strictness_kind: Flow,
                                },
                            ),
                        ),
                    ),
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
        DeclareClassBinding(
            DefDeclareClassBinding {
                id_loc: 9,
                name: "C3",
                def: DeclareClassSig {
                    tparams: Mono,
                    extends: ClassExplicitExtends(
                        (
                            10,
                            Ref(
                                LocalRef(
                                    PackedRefLocal {
                                        ref_loc: 10,
                                        index: 0,
                                    },
                                ),
                            ),
                        ),
                    ),
                    mixins: [],
                    implements: [],
                    static_props: {},
                    own_props: {},
                    proto_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    static_calls: [],
                    calls: [],
                    constructs: [],
                    dict: None,
                    static_dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
        DeclareClassBinding(
            DefDeclareClassBinding {
                id_loc: 11,
                name: "C4",
                def: DeclareClassSig {
                    tparams: Mono,
                    extends: ClassExplicitExtends(
                        (
                            12,
                            Eval(
                                PackedEval {
                                    loc: 12,
                                    packed: Ref(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 13,
                                                index: 1,
                                            },
                                        ),
                                    ),
                                    op: GetProp(
                                        "C",
                                    ),
                                    strictness_kind: Flow,
                                },
                            ),
                        ),
                    ),
                    mixins: [],
                    implements: [],
                    static_props: {},
                    own_props: {},
                    proto_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    static_calls: [],
                    calls: [],
                    constructs: [],
                    dict: None,
                    static_dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn class_this() {
    let input = r#"
            export class C {
      m(): this { return this };
    }
        "#;
    let expected_output = r#"
Locs:
0. [1:21-22]
1. [2:2-27]
2. [2:2-3]
3. [2:7-11]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "C",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {
                        "m": ObjValueMethod(
                            [
                                ObjValueMethodData {
                                    id_loc: 2,
                                    fn_loc: 1,
                                    async_: false,
                                    generator: false,
                                    uses_this: false,
                                    def: FunSig {
                                        tparams: Mono,
                                        params: [],
                                        rest_param: None,
                                        this_param: None,
                                        return_: Annot(
                                            Bound(
                                                AnnotBound {
                                                    ref_loc: 3,
                                                    name: "this",
                                                },
                                            ),
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                },
                            ],
                        ),
                    },
                    own_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn declare_class_this() {
    let input = r#"
            declare export class C {
      m(): this;
    }
        "#;
    let expected_output = r#"
Locs:
0. [1:29-30]
1. [2:2-11]
2. [2:2-3]
3. [2:7-11]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "C",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        DeclareClassBinding(
            DefDeclareClassBinding {
                id_loc: 0,
                name: "C",
                def: DeclareClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    mixins: [],
                    implements: [],
                    static_props: {},
                    own_props: {},
                    proto_props: {
                        "m": InterfaceMethod(
                            [
                                (
                                    2,
                                    1,
                                    FunSig {
                                        tparams: Mono,
                                        params: [],
                                        rest_param: None,
                                        this_param: None,
                                        return_: Annot(
                                            Bound(
                                                AnnotBound {
                                                    ref_loc: 3,
                                                    name: "this",
                                                },
                                            ),
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                ),
                            ],
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    static_calls: [],
                    calls: [],
                    constructs: [],
                    dict: None,
                    static_dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn existential() {
    let input = r#"
            class C<T> {
      p: *;
    };
    declare export default C<*>;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [1:15-18]
2. [1:16-17]
3. [2:2-3]
4. [2:5-6]
5. [4:15-22]
6. [4:23-27]
7. [4:23-24]
8. [4:25-26]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefault(
                ExportDefaultData {
                    default_loc: 5,
                    def: TyRefApp(
                        PackedTyRefApp {
                            loc: 6,
                            name: Unqualified(
                                LocalRef(
                                    PackedRefLocal {
                                        ref_loc: 7,
                                        index: 0,
                                    },
                                ),
                            ),
                            targs: [
                                Annot(
                                    Exists(
                                        8,
                                    ),
                                ),
                            ],
                        },
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "C",
                def: ClassSig {
                    tparams: Poly(
                        (
                            1,
                            [
                                TParam {
                                    name_loc: 2,
                                    name: "T",
                                    polarity: None,
                                    bound: None,
                                    default: None,
                                    is_const: false,
                                },
                            ],
                        ),
                    ),
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {
                        "p": ObjValueField(
                            (
                                3,
                                Annot(
                                    Exists(
                                        4,
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn exact_by_default() {
    let input = r#"
            export type T = { p: string }
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-29]
2. [1:18-19]
3. [1:21-27]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "T",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "T",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 1,
                            obj_kind: ExactObj,
                            props: {
                                "p": ObjAnnotField(
                                    (
                                        2,
                                        Annot(
                                            String(
                                                3,
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                            },
                            computed_props: [],
                            proto: ObjAnnotImplicitProto,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn cjs_export_props() {
    let input = r#"
            module.exports.foo = 0;
    exports.bar = 1;
        "#;
    let expected_output = r#"
Locs:
0. [0:0]
1. [1:23-26]
2. [1:29-30]
3. [2:8-11]
4. [2:14-15]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Value(
                ObjLit(
                    ValueObjLit {
                        loc: 0,
                        frozen: true,
                        proto: None,
                        props: {
                            "bar": ObjValueField(
                                (
                                    3,
                                    Value(
                                        NumberLit(
                                            (
                                                4,
                                                1.0,
                                                "1",
                                            ),
                                        ),
                                    ),
                                    Neutral,
                                ),
                            ),
                            "foo": ObjValueField(
                                (
                                    1,
                                    Value(
                                        NumberLit(
                                            (
                                                2,
                                                0.0,
                                                "0",
                                            ),
                                        ),
                                    ),
                                    Neutral,
                                ),
                            ),
                        },
                        strictness_kind: Flow,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn cjs_exports_clobber_shadowed_module_global() {
    let input = r#"
            var module;
    module.exports = 0;
        "#;
    let expected_output = r#"
Locs:
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn cjs_exports_assign_shadowed_exports_global() {
    let input = r#"
            var exports;
    exports.foo = 0;
        "#;
    let expected_output = r#"
Locs:
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn cjs_exports_assign_shadowed_module_global() {
    let input = r#"
            var module;
    module.exports.foo = 0;
        "#;
    let expected_output = r#"
Locs:
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn cjs_export_shadowed_hoisted_todo() {
    let input = r#"
            module.exports.foo = 0;
    function module() {}
        "#;
    let expected_output = r#"
Locs:
0. [0:0]
1. [1:23-26]
2. [1:29-30]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Value(
                ObjLit(
                    ValueObjLit {
                        loc: 0,
                        frozen: true,
                        proto: None,
                        props: {
                            "foo": ObjValueField(
                                (
                                    1,
                                    Value(
                                        NumberLit(
                                            (
                                                2,
                                                0.0,
                                                "0",
                                            ),
                                        ),
                                    ),
                                    Neutral,
                                ),
                            ),
                        },
                        strictness_kind: Flow,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn cjs_export_fun_expr_props() {
    let input = r#"
            module.exports = function() {}
    module.exports.foo = 0;
    exports.bar = 1;
        "#;
    let expected_output = r#"
Locs:
0. [1:25-35]
1. [1:35]
2. [2:15-18]
3. [2:21-22]
4. [3:8-11]
5. [3:14-15]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Value(
                FunExpr(
                    ValueFunExpr {
                        loc: 0,
                        async_: false,
                        generator: false,
                        def: FunSig {
                            tparams: Mono,
                            params: [],
                            rest_param: None,
                            this_param: None,
                            return_: Annot(
                                Void(
                                    1,
                                ),
                            ),
                            type_guard: None,
                            effect_: ArbitraryEffect,
                            strictness_kind: Flow,
                        },
                        statics: {
                            "bar": (
                                4,
                                Value(
                                    NumberLit(
                                        (
                                            5,
                                            1.0,
                                            "1",
                                        ),
                                    ),
                                ),
                            ),
                            "foo": (
                                2,
                                Value(
                                    NumberLit(
                                        (
                                            3,
                                            0.0,
                                            "0",
                                        ),
                                    ),
                                ),
                            ),
                        },
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn cjs_export_fun_binding_props() {
    let input = r#"
module.exports = function foo() {}
module.exports.foo = 0;
exports.bar = 1;
        "#;
    let expected_output = r#"
Locs:
0. [1:17-31]
1. [1:26-29]
2. [1:31]
3. [2:15-18]
4. [2:21-22]
5. [3:8-11]
6. [3:14-15]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Ref(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 1,
                        index: 0,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "foo",
                async_: false,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Void(
                            2,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {
                    "bar": (
                        5,
                        Value(
                            NumberLit(
                                (
                                    6,
                                    1.0,
                                    "1",
                                ),
                            ),
                        ),
                        Neutral,
                    ),
                    "foo": (
                        3,
                        Value(
                            NumberLit(
                                (
                                    4,
                                    0.0,
                                    "0",
                                ),
                            ),
                        ),
                        Neutral,
                    ),
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn es_export_named_fun_props() {
    let input = r#"
            export function foo() {}
    foo.bar = 1;
        "#;
    let expected_output = r#"
Locs:
0. [1:15-29]
1. [1:24-27]
2. [1:29]
3. [2:4-7]
4. [2:10-11]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "foo",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "foo",
                async_: false,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Void(
                            2,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {
                    "bar": (
                        3,
                        Value(
                            NumberLit(
                                (
                                    4,
                                    1.0,
                                    "1",
                                ),
                            ),
                        ),
                        Neutral,
                    ),
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn es_export_default_fun_props() {
    let input = r#"
            export default function foo() {}
    foo.bar = 1;
        "#;
    let expected_output = r#"
Locs:
0. [1:15-22]
1. [1:23-37]
2. [1:32-35]
3. [1:37]
4. [2:4-7]
5. [2:10-11]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefaultBinding(
                ExportDefaultBindingData {
                    default_loc: 0,
                    index: 0,
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 2,
                name: "foo",
                async_: false,
                generator: false,
                fn_loc: 1,
                def: FunSig {
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Void(
                            3,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {
                    "bar": (
                        4,
                        Value(
                            NumberLit(
                                (
                                    5,
                                    1.0,
                                    "1",
                                ),
                            ),
                        ),
                        Neutral,
                    ),
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn fun_binding_assign() {
    let input = r#"
            function foo() {}
    foo.bar = 0;
    module.exports = foo;
        "#;
    let expected_output = r#"
Locs:
0. [1:8-22]
1. [1:17-20]
2. [1:22]
3. [2:4-7]
4. [2:10-11]
5. [3:17-20]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Ref(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 5,
                        index: 0,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "foo",
                async_: false,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Void(
                            2,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {
                    "bar": (
                        3,
                        Value(
                            NumberLit(
                                (
                                    4,
                                    0.0,
                                    "0",
                                ),
                            ),
                        ),
                        Neutral,
                    ),
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn fun_const_assign() {
    let input = r#"
            const foo = function() {};
    foo.bar = 0;
    module.exports = foo;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-17]
1. [1:20-30]
2. [1:30]
3. [2:4-7]
4. [2:10-11]
5. [3:17-20]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Ref(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 5,
                        index: 0,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "foo",
                def: Value(
                    FunExpr(
                        ValueFunExpr {
                            loc: 1,
                            async_: false,
                            generator: false,
                            def: FunSig {
                                tparams: Mono,
                                params: [],
                                rest_param: None,
                                this_param: None,
                                return_: Annot(
                                    Void(
                                        2,
                                    ),
                                ),
                                type_guard: None,
                                effect_: ArbitraryEffect,
                                strictness_kind: Flow,
                            },
                            statics: {
                                "bar": (
                                    3,
                                    Value(
                                        NumberLit(
                                            (
                                                4,
                                                0.0,
                                                "0",
                                            ),
                                        ),
                                    ),
                                ),
                            },
                        },
                    ),
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn ref_const_assign() {
    let input = r#"
            const foo = function f() {};
    foo.bar = 0;
    module.exports = foo;
        "#;
    let expected_output = r#"
Locs:
0. [1:14-17]
1. [1:20-32]
2. [1:29-30]
3. [1:32]
4. [2:4-7]
5. [2:10-11]
6. [3:17-20]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Ref(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 6,
                        index: 1,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 2,
                name: "f",
                async_: false,
                generator: false,
                fn_loc: 1,
                def: FunSig {
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Void(
                            3,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {
                    "bar": (
                        4,
                        Value(
                            NumberLit(
                                (
                                    5,
                                    0.0,
                                    "0",
                                ),
                            ),
                        ),
                        Neutral,
                    ),
                },
                namespace_types: {},
            },
        ),
        Variable(
            DefVariable {
                id_loc: 0,
                name: "foo",
                def: Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 2,
                            index: 0,
                        },
                    ),
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn obj_annot_proto() {
    let input = r#"
            declare export var o: { __proto__: null };
        "#;
    let expected_output = r#"
Locs:
0. [1:19-20]
1. [1:22-41]
2. [1:35-39]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "o",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "o",
                def: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 1,
                            obj_kind: ExactObj,
                            props: {},
                            computed_props: [],
                            proto: ObjAnnotExplicitProto(
                                (
                                    2,
                                    Annot(
                                        Null(
                                            2,
                                        ),
                                    ),
                                ),
                            ),
                            strictness_kind: Flow,
                        },
                    ),
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn getter_setter() {
    let input = r#"
            export const a = { get p(): number { return 0 } };
    export const b = { set p(x: number): void {} };
    export const c = { get p(): number { return 0 }, set p(x: number): void {} };
    export const d = { get p(): number { return 0 }, get p(): string { return "" } };
    export const e = { set p(x: number): void {}, set p(x: string): void {} };
    export const f = {
      get p(): number { return 0 },
      set p(x: number): void {},
      get p(): string { return "" },
    };
    export const g = {
      get p(): number { return 0 },
      set p(x: number): void {},
      set p(x: string): void {},
    };
        "#;
    let expected_output = r#"
Locs:
0. [1:21-22]
1. [1:25-57]
2. [1:31-32]
3. [1:36-42]
4. [2:13-14]
5. [2:17-46]
6. [2:23-24]
7. [2:28-34]
8. [3:13-14]
9. [3:17-76]
10. [3:23-24]
11. [3:28-34]
12. [3:53-54]
13. [3:58-64]
14. [4:13-14]
15. [4:17-80]
16. [4:53-54]
17. [4:58-64]
18. [5:13-14]
19. [5:17-73]
20. [5:50-51]
21. [5:55-61]
22. [6:13-14]
23. [6:17-10:1]
24. [8:6-7]
25. [8:11-17]
26. [9:6-7]
27. [9:11-17]
28. [11:13-14]
29. [11:17-15:1]
30. [12:6-7]
31. [12:11-17]
32. [14:6-7]
33. [14:11-17]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
            ExportBinding(
                1,
            ),
            ExportBinding(
                2,
            ),
            ExportBinding(
                3,
            ),
            ExportBinding(
                4,
            ),
            ExportBinding(
                5,
            ),
            ExportBinding(
                6,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "a",
                "b",
                "c",
                "d",
                "e",
                "f",
                "g",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "a",
                def: Value(
                    ObjLit(
                        ValueObjLit {
                            loc: 1,
                            frozen: false,
                            proto: None,
                            props: {
                                "p": ObjValueAccess(
                                    Get(
                                        (
                                            2,
                                            Annot(
                                                Number(
                                                    3,
                                                ),
                                            ),
                                        ),
                                    ),
                                ),
                            },
                            strictness_kind: Flow,
                        },
                    ),
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 4,
                name: "b",
                def: Value(
                    ObjLit(
                        ValueObjLit {
                            loc: 5,
                            frozen: false,
                            proto: None,
                            props: {
                                "p": ObjValueAccess(
                                    Set(
                                        (
                                            6,
                                            Annot(
                                                Number(
                                                    7,
                                                ),
                                            ),
                                        ),
                                    ),
                                ),
                            },
                            strictness_kind: Flow,
                        },
                    ),
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 8,
                name: "c",
                def: Value(
                    ObjLit(
                        ValueObjLit {
                            loc: 9,
                            frozen: false,
                            proto: None,
                            props: {
                                "p": ObjValueAccess(
                                    GetSet(
                                        (
                                            10,
                                            Annot(
                                                Number(
                                                    11,
                                                ),
                                            ),
                                            12,
                                            Annot(
                                                Number(
                                                    13,
                                                ),
                                            ),
                                        ),
                                    ),
                                ),
                            },
                            strictness_kind: Flow,
                        },
                    ),
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 14,
                name: "d",
                def: Value(
                    ObjLit(
                        ValueObjLit {
                            loc: 15,
                            frozen: false,
                            proto: None,
                            props: {
                                "p": ObjValueAccess(
                                    Get(
                                        (
                                            16,
                                            Annot(
                                                String(
                                                    17,
                                                ),
                                            ),
                                        ),
                                    ),
                                ),
                            },
                            strictness_kind: Flow,
                        },
                    ),
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 18,
                name: "e",
                def: Value(
                    ObjLit(
                        ValueObjLit {
                            loc: 19,
                            frozen: false,
                            proto: None,
                            props: {
                                "p": ObjValueAccess(
                                    Set(
                                        (
                                            20,
                                            Annot(
                                                String(
                                                    21,
                                                ),
                                            ),
                                        ),
                                    ),
                                ),
                            },
                            strictness_kind: Flow,
                        },
                    ),
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 22,
                name: "f",
                def: Value(
                    ObjLit(
                        ValueObjLit {
                            loc: 23,
                            frozen: false,
                            proto: None,
                            props: {
                                "p": ObjValueAccess(
                                    GetSet(
                                        (
                                            26,
                                            Annot(
                                                String(
                                                    27,
                                                ),
                                            ),
                                            24,
                                            Annot(
                                                Number(
                                                    25,
                                                ),
                                            ),
                                        ),
                                    ),
                                ),
                            },
                            strictness_kind: Flow,
                        },
                    ),
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 28,
                name: "g",
                def: Value(
                    ObjLit(
                        ValueObjLit {
                            loc: 29,
                            frozen: false,
                            proto: None,
                            props: {
                                "p": ObjValueAccess(
                                    GetSet(
                                        (
                                            30,
                                            Annot(
                                                Number(
                                                    31,
                                                ),
                                            ),
                                            32,
                                            Annot(
                                                String(
                                                    33,
                                                ),
                                            ),
                                        ),
                                    ),
                                ),
                            },
                            strictness_kind: Flow,
                        },
                    ),
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_default_function_binding() {
    let input = r#"
            export default function f(): void {}
        "#;
    let expected_output = r#"
Locs:
0. [1:7-14]
1. [1:15-33]
2. [1:24-25]
3. [1:29-33]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefaultBinding(
                ExportDefaultBindingData {
                    default_loc: 0,
                    index: 0,
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 2,
                name: "f",
                async_: false,
                generator: false,
                fn_loc: 1,
                def: FunSig {
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Void(
                            3,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn export_default_class_binding() {
    let input = r#"
            export default class C {}
        "#;
    let expected_output = r#"
Locs:
0. [1:7-14]
1. [1:21-22]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefaultBinding(
                ExportDefaultBindingData {
                    default_loc: 0,
                    index: 0,
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 1,
                name: "C",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {},
                    own_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn declared_export_default_function_binding() {
    let input = r#"
            declare export default function f(): void;
        "#;
    let expected_output = r#"
Locs:
0. [1:15-22]
1. [1:32-33]
2. [1:33-41]
3. [1:37-41]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefaultBinding(
                ExportDefaultBindingData {
                    default_loc: 0,
                    index: 0,
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        DeclareFun(
            DefDeclareFun {
                id_loc: 1,
                name: "f",
                fn_loc: 2,
                def: FunSig {
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Void(
                            3,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
                tail: [],
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn declared_export_default_class_binding() {
    let input = r#"
            declare export default class C {};
        "#;
    let expected_output = r#"
Locs:
0. [1:15-22]
1. [1:29-30]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefaultBinding(
                ExportDefaultBindingData {
                    default_loc: 0,
                    index: 0,
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        DeclareClassBinding(
            DefDeclareClassBinding {
                id_loc: 1,
                name: "C",
                def: DeclareClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    mixins: [],
                    implements: [],
                    static_props: {},
                    own_props: {},
                    proto_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    static_calls: [],
                    calls: [],
                    constructs: [],
                    dict: None,
                    static_dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn module_ref_prefix() {
    let input = r#"
            module.exports = "m#foo";
        "#;
    let expected_output = r#"
Locs:
0. [1:17-24]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            ModuleRef(
                PackedLocIndex {
                    loc: 0,
                    index: 0,
                    strictness_kind: Flow,
                },
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [
        Userland(
            "foo",
        ),
    ],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    let parse_opts = flow_parser::ParseOptions {
        module_ref_prefix: Some(FlowSmolStr::new("m#")),
        ..Default::default()
    };
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_parse_and_sig_options(
            input,
            Some(parse_opts),
            None::<fn(&mut TypeSigOptions)>
        ))
    )
}

#[test]
fn enum_export() {
    let input = r#"
            export enum E { A, B };
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-17]
2. [1:19-20]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "E",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        EnumBinding(
            DefEnumBinding {
                id_loc: 0,
                name: "E",
                rep: Some(
                    StringRep {
                        truthy: true,
                    },
                ),
                members: {
                    "A": 1,
                    "B": 2,
                },
                member_values: None,
                has_unknown_members: false,
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn enum_default_export() {
    let input = r#"
            export default enum E { A, B }
        "#;
    let expected_output = r#"
Locs:
0. [1:7-14]
1. [1:20-21]
2. [1:24-25]
3. [1:27-28]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportDefaultBinding(
                ExportDefaultBindingData {
                    default_loc: 0,
                    index: 0,
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        EnumBinding(
            DefEnumBinding {
                id_loc: 1,
                name: "E",
                rep: Some(
                    StringRep {
                        truthy: true,
                    },
                ),
                members: {
                    "A": 2,
                    "B": 3,
                },
                member_values: None,
                has_unknown_members: false,
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn enum_stmt() {
    let input = r#"
            enum E { A, B };
    export {E}
        "#;
    let expected_output = r#"
Locs:
0. [1:13-14]
1. [1:17-18]
2. [1:20-21]
3. [2:8-9]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportRef(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 3,
                        index: 0,
                    },
                ),
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "E",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        EnumBinding(
            DefEnumBinding {
                id_loc: 0,
                name: "E",
                rep: Some(
                    StringRep {
                        truthy: true,
                    },
                ),
                members: {
                    "A": 1,
                    "B": 2,
                },
                member_values: None,
                has_unknown_members: false,
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn enum_bool_lit() {
    let input = r#"
            export enum E { A = true }
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-24]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "E",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        EnumBinding(
            DefEnumBinding {
                id_loc: 0,
                name: "E",
                rep: Some(
                    BoolRep(
                        Some(
                            true,
                        ),
                    ),
                ),
                members: {
                    "A": 1,
                },
                member_values: None,
                has_unknown_members: false,
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn enum_bool() {
    let input = r#"
            export enum E { A = true, B = false }
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-24]
2. [1:26-35]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "E",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        EnumBinding(
            DefEnumBinding {
                id_loc: 0,
                name: "E",
                rep: Some(
                    BoolRep(
                        None,
                    ),
                ),
                members: {
                    "A": 1,
                    "B": 2,
                },
                member_values: None,
                has_unknown_members: false,
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn enum_number_truthy() {
    let input = r#"
            export enum E { A = 1, B = 2 }
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-21]
2. [1:23-28]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "E",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        EnumBinding(
            DefEnumBinding {
                id_loc: 0,
                name: "E",
                rep: Some(
                    NumberRep {
                        truthy: true,
                    },
                ),
                members: {
                    "A": 1,
                    "B": 2,
                },
                member_values: None,
                has_unknown_members: false,
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn enum_number_any() {
    let input = r#"
            export enum E { A = 0, B = 1 }
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-21]
2. [1:23-28]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "E",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        EnumBinding(
            DefEnumBinding {
                id_loc: 0,
                name: "E",
                rep: Some(
                    NumberRep {
                        truthy: false,
                    },
                ),
                members: {
                    "A": 1,
                    "B": 2,
                },
                member_values: None,
                has_unknown_members: false,
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn enum_string_any() {
    let input = r#"
            export enum E { A = "", B = "B" }
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-22]
2. [1:24-31]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "E",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        EnumBinding(
            DefEnumBinding {
                id_loc: 0,
                name: "E",
                rep: Some(
                    StringRep {
                        truthy: false,
                    },
                ),
                members: {
                    "A": 1,
                    "B": 2,
                },
                member_values: None,
                has_unknown_members: false,
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn enum_symbol() {
    let input = r#"
            export enum E of symbol { A, B }
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:26-27]
2. [1:29-30]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "E",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        EnumBinding(
            DefEnumBinding {
                id_loc: 0,
                name: "E",
                rep: Some(
                    SymbolRep,
                ),
                members: {
                    "A": 1,
                    "B": 2,
                },
                member_values: None,
                has_unknown_members: false,
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn enum_unknown_members() {
    let input = r#"
            export enum E { A, B, ... };
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-17]
2. [1:19-20]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "E",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        EnumBinding(
            DefEnumBinding {
                id_loc: 0,
                name: "E",
                rep: Some(
                    StringRep {
                        truthy: true,
                    },
                ),
                members: {
                    "A": 1,
                    "B": 2,
                },
                member_values: None,
                has_unknown_members: true,
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn enum_declared() {
    let input = r#"
            declare export enum E { A, B };
        "#;
    let expected_output = r#"
Locs:
0. [1:20-21]
1. [1:24-25]
2. [1:27-28]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "E",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        EnumBinding(
            DefEnumBinding {
                id_loc: 0,
                name: "E",
                rep: Some(
                    StringRep {
                        truthy: true,
                    },
                ),
                members: {
                    "A": 1,
                    "B": 2,
                },
                member_values: None,
                has_unknown_members: false,
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn enum_disabled() {
    let input = r#"
            export enum E {}
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "E",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        DisabledEnumBinding(
            DefDisabledEnumBinding {
                id_loc: 0,
                name: "E",
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_options(
            input,
            Some(|opts: &mut TypeSigOptions| {
                opts.enable_enums = false;
            })
        ))
    )
}

#[test]
fn ts_enum_member_values() {
    // In a .ts/.d.ts file, the enum signature additionally retains each member's
    // literal value (member_values = Some ...), so consumers can model the enum as
    // an object of literal members + union type.
    let input = r#"
            export enum Color { Red = 1, Green = 2 }
            export enum S { A = "a", B = "b" }
        "#;
    let expected_output = r#"
Locs:
0. [1:12-17]
1. [1:20-27]
2. [1:29-38]
3. [2:12-13]
4. [2:16-23]
5. [2:25-32]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
            ExportBinding(
                1,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "Color",
                "S",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: TypeScriptLoose,
        },
    },
    module_refs: [],
    local_defs: [
        EnumBinding(
            DefEnumBinding {
                id_loc: 0,
                name: "Color",
                rep: Some(
                    NumberRep {
                        truthy: true,
                    },
                ),
                members: {
                    "Green": 2,
                    "Red": 1,
                },
                member_values: Some(
                    {
                        "Green": TSEnumMemberNumber(
                            2.0,
                            "2",
                        ),
                        "Red": TSEnumMemberNumber(
                            1.0,
                            "1",
                        ),
                    },
                ),
                has_unknown_members: false,
                strictness_kind: TypeScriptLoose,
            },
        ),
        EnumBinding(
            DefEnumBinding {
                id_loc: 3,
                name: "S",
                rep: Some(
                    StringRep {
                        truthy: true,
                    },
                ),
                members: {
                    "A": 4,
                    "B": 5,
                },
                member_values: Some(
                    {
                        "A": TSEnumMemberString(
                            "a",
                        ),
                        "B": TSEnumMemberString(
                            "b",
                        ),
                    },
                ),
                has_unknown_members: false,
                strictness_kind: TypeScriptLoose,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_options(
            input,
            Some(|opts: &mut TypeSigOptions| {
                opts.is_ts_file = true;
            })
        ))
    )
}

#[test]
fn ts_enum_ambient_member_values() {
    // In an ambient TS file (.d.ts), uninitialized members are computed rather than
    // auto-numbered literals, so they are recorded as TSEnumMemberComputed. Explicit
    // initializers are still retained.
    let input = r#"
            export enum E { A, B }
            export enum F { A = 1, B }
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-17]
2. [1:19-20]
3. [2:12-13]
4. [2:16-21]
5. [2:23-24]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
            ExportBinding(
                1,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "E",
                "F",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: TypeScriptLoose,
        },
    },
    module_refs: [],
    local_defs: [
        EnumBinding(
            DefEnumBinding {
                id_loc: 0,
                name: "E",
                rep: Some(
                    StringRep {
                        truthy: true,
                    },
                ),
                members: {
                    "A": 1,
                    "B": 2,
                },
                member_values: Some(
                    {
                        "A": TSEnumMemberComputed,
                        "B": TSEnumMemberComputed,
                    },
                ),
                has_unknown_members: false,
                strictness_kind: TypeScriptLoose,
            },
        ),
        EnumBinding(
            DefEnumBinding {
                id_loc: 3,
                name: "F",
                rep: Some(
                    NumberRep {
                        truthy: true,
                    },
                ),
                members: {
                    "A": 4,
                    "B": 5,
                },
                member_values: Some(
                    {
                        "A": TSEnumMemberNumber(
                            1.0,
                            "1",
                        ),
                        "B": TSEnumMemberComputed,
                    },
                ),
                has_unknown_members: false,
                strictness_kind: TypeScriptLoose,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_options(
            input,
            Some(|opts: &mut TypeSigOptions| {
                opts.is_ts_file = true;
                opts.is_dts_file = true;
            })
        ))
    )
}

#[test]
fn this_param_1() {
    let input = r#"
            export function foo(this : mixed) : void {}
        "#;
    let expected_output = r#"
Locs:
0. [1:7-40]
1. [1:16-19]
2. [1:27-32]
3. [1:36-40]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "foo",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "foo",
                async_: false,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    this_param: Some(
                        Annot(
                            Mixed(
                                2,
                            ),
                        ),
                    ),
                    return_: Annot(
                        Void(
                            3,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn this_param_2() {
    let input = r#"
            export class A {
      foo(this : mixed) : void {}
    }
        "#;
    let expected_output = r#"
Locs:
0. [1:21-22]
1. [2:2-29]
2. [2:2-5]
3. [2:13-18]
4. [2:22-26]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "A",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ClassBinding(
            DefClassBinding {
                id_loc: 0,
                name: "A",
                def: ClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    implements: [],
                    static_props: {},
                    proto_props: {
                        "foo": ObjValueMethod(
                            [
                                ObjValueMethodData {
                                    id_loc: 2,
                                    fn_loc: 1,
                                    async_: false,
                                    generator: false,
                                    uses_this: false,
                                    def: FunSig {
                                        tparams: Mono,
                                        params: [],
                                        rest_param: None,
                                        this_param: Some(
                                            Annot(
                                                Mixed(
                                                    3,
                                                ),
                                            ),
                                        ),
                                        return_: Annot(
                                            Void(
                                                4,
                                            ),
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                },
                            ],
                        ),
                    },
                    own_props: {},
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn this_param_3() {
    let input = r#"
            declare export function foo(this : mixed) : void;
        "#;
    let expected_output = r#"
Locs:
0. [1:24-27]
1. [1:27-48]
2. [1:35-40]
3. [1:44-48]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "foo",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        DeclareFun(
            DefDeclareFun {
                id_loc: 0,
                name: "foo",
                fn_loc: 1,
                def: FunSig {
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    this_param: Some(
                        Annot(
                            Mixed(
                                2,
                            ),
                        ),
                    ),
                    return_: Annot(
                        Void(
                            3,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
                tail: [],
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn this_param_4() {
    let input = r#"
            declare export class A {
      foo(this : mixed) : void;
    }
        "#;
    let expected_output = r#"
Locs:
0. [1:29-30]
1. [2:2-26]
2. [2:2-5]
3. [2:13-18]
4. [2:22-26]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "A",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        DeclareClassBinding(
            DefDeclareClassBinding {
                id_loc: 0,
                name: "A",
                def: DeclareClassSig {
                    tparams: Mono,
                    extends: ClassImplicitExtends,
                    mixins: [],
                    implements: [],
                    static_props: {},
                    own_props: {},
                    proto_props: {
                        "foo": InterfaceMethod(
                            [
                                (
                                    2,
                                    1,
                                    FunSig {
                                        tparams: Mono,
                                        params: [],
                                        rest_param: None,
                                        this_param: Some(
                                            Annot(
                                                Mixed(
                                                    3,
                                                ),
                                            ),
                                        ),
                                        return_: Annot(
                                            Void(
                                                4,
                                            ),
                                        ),
                                        type_guard: None,
                                        effect_: ArbitraryEffect,
                                        strictness_kind: Flow,
                                    },
                                ),
                            ],
                        ),
                    },
                    computed_own_props: [],
                    computed_proto_props: [],
                    computed_static_props: [],
                    static_calls: [],
                    calls: [],
                    constructs: [],
                    dict: None,
                    static_dict: None,
                    abstract_: false,
                    abstract_props: {},
                    strictness_kind: Flow,
                },
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn this_param_5() {
    let input = r#"
            export type Foo = (this : mixed) => void
        "#;
    let expected_output = r#"
Locs:
0. [1:12-15]
1. [1:18-40]
2. [1:26-31]
3. [1:36-40]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "Foo",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "Foo",
                tparams: Mono,
                body: Annot(
                    FunAnnot(
                        (
                            1,
                            FunSig {
                                tparams: Mono,
                                params: [],
                                rest_param: None,
                                this_param: Some(
                                    Annot(
                                        Mixed(
                                            2,
                                        ),
                                    ),
                                ),
                                return_: Annot(
                                    Void(
                                        3,
                                    ),
                                ),
                                type_guard: None,
                                effect_: ArbitraryEffect,
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn this_param_6() {
    let input = r#"
            export type O = { f : (this : mixed) => void, a : number }
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-58]
2. [1:18-19]
3. [1:22-44]
4. [1:30-35]
5. [1:40-44]
6. [1:46-47]
7. [1:50-56]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "O",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "O",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 1,
                            obj_kind: ExactObj,
                            props: {
                                "a": ObjAnnotField(
                                    (
                                        6,
                                        Annot(
                                            Number(
                                                7,
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                                "f": ObjAnnotField(
                                    (
                                        2,
                                        Annot(
                                            FunAnnot(
                                                (
                                                    3,
                                                    FunSig {
                                                        tparams: Mono,
                                                        params: [],
                                                        rest_param: None,
                                                        this_param: Some(
                                                            Annot(
                                                                Mixed(
                                                                    4,
                                                                ),
                                                            ),
                                                        ),
                                                        return_: Annot(
                                                            Void(
                                                                5,
                                                            ),
                                                        ),
                                                        type_guard: None,
                                                        effect_: ArbitraryEffect,
                                                        strictness_kind: Flow,
                                                    },
                                                ),
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                            },
                            computed_props: [],
                            proto: ObjAnnotImplicitProto,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn optional_indexed_access() {
    let input = r#"
            export type T = Obj?.['a']['b'];
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-31]
2. [1:16-26]
3. [1:16-19]
4. [1:22-25]
5. [1:27-30]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "T",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "T",
                tparams: Mono,
                body: Annot(
                    OptionalIndexedAccessResultType(
                        AnnotOptionalIndexedAccessResultType {
                            loc: 1,
                            non_maybe_result: Annot(
                                ElementType(
                                    AnnotElementType {
                                        loc: 1,
                                        obj: Annot(
                                            OptionalIndexedAccessNonMaybeType(
                                                AnnotOptionalIndexedAccessNonMaybeType {
                                                    loc: 2,
                                                    obj: TyRef(
                                                        Unqualified(
                                                            BuiltinRef(
                                                                PackedRefBuiltin {
                                                                    ref_loc: 3,
                                                                    type_ref: true,
                                                                    name: "Obj",
                                                                },
                                                            ),
                                                        ),
                                                    ),
                                                    index: Annot(
                                                        SingletonString(
                                                            (
                                                                4,
                                                                "a",
                                                            ),
                                                        ),
                                                    ),
                                                },
                                            ),
                                        ),
                                        elem: Annot(
                                            SingletonString(
                                                (
                                                    5,
                                                    "b",
                                                ),
                                            ),
                                        ),
                                    },
                                ),
                            ),
                            void_loc: 2,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn cjs_export_type_star() {
    let input = r#"
            export type * from 'foo';
    export type * from 'bar';
        "#;
    let expected_output = r#"
Locs:
0. [1:20-21]
1. [2:12-13]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [
                (
                    1,
                    1,
                ),
                (
                    0,
                    0,
                ),
            ],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [
        Userland(
            "foo",
        ),
        Userland(
            "bar",
        ),
    ],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn es_export_star() {
    let input = r#"
            export type * from 'foo';
    export type * from 'bar';
    export * from 'baz';
    export * from 'qux';
        "#;
    let expected_output = r#"
Locs:
0. [1:20-21]
1. [2:12-13]
2. [3:7-8]
3. [4:7-8]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [
                (
                    1,
                    1,
                ),
                (
                    0,
                    0,
                ),
            ],
            export_keys: [],
            stars: [
                (
                    3,
                    3,
                ),
                (
                    2,
                    2,
                ),
            ],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [
        Userland(
            "foo",
        ),
        Userland(
            "bar",
        ),
        Userland(
            "baz",
        ),
        Userland(
            "qux",
        ),
    ],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn duplicate_binding() {
    let input = r#"
            import type {T} from 'foo';
    export type T = any;
        "#;
    let expected_output = r#"
Locs:
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn duplicate_binding2() {
    let input = r#"
            const foo = 1;
    export var {foo, bar}: {foo: number, bar: number} = {foo: 2, bar: 3};
        "#;
    let expected_output = r#"
Locs:
0. [2:17-20]
1. [2:23-49]
2. [2:24-27]
3. [2:29-35]
4. [2:37-40]
5. [2:42-48]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "bar",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "bar",
                def: Pattern(
                    1,
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [
        Annot(
            ObjAnnot(
                AnnotObjAnnot {
                    loc: 1,
                    obj_kind: ExactObj,
                    props: {
                        "bar": ObjAnnotField(
                            (
                                4,
                                Annot(
                                    Number(
                                        5,
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                        "foo": ObjAnnotField(
                            (
                                2,
                                Annot(
                                    Number(
                                        3,
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    computed_props: [],
                    proto: ObjAnnotImplicitProto,
                    strictness_kind: Flow,
                },
            ),
        ),
    ],
    dirty_pattern_defs: [],
    patterns: [
        PDef(
            0,
        ),
        PropP {
            id_loc: 0,
            name: "bar",
            def: 0,
            strictness_kind: Flow,
        },
    ],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn duplicate_binding2_2() {
    let input = r#"
            class C {}
    export default class C {}
        "#;
    let expected_output = r#"
Locs:
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn fun_shadow_declare_fun() {
    let input = r#"
declare function f(x: string): number;
declare function f(x: number): string;
export function f() {}
        "#;
    let expected_output = r#"
Locs:
0. [1:17-18]
1. [1:18-37]
2. [1:22-28]
3. [1:31-37]
4. [2:17-18]
5. [2:18-37]
6. [2:22-28]
7. [2:31-37]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "f",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        DeclareFun(
            DefDeclareFun {
                id_loc: 0,
                name: "f",
                fn_loc: 1,
                def: FunSig {
                    tparams: Mono,
                    params: [
                        FunParam {
                            name: Some(
                                "x",
                            ),
                            t: Annot(
                                String(
                                    2,
                                ),
                            ),
                        },
                    ],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Number(
                            3,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
                tail: [
                    (
                        4,
                        5,
                        FunSig {
                            tparams: Mono,
                            params: [
                                FunParam {
                                    name: Some(
                                        "x",
                                    ),
                                    t: Annot(
                                        Number(
                                            6,
                                        ),
                                    ),
                                },
                            ],
                            rest_param: None,
                            this_param: None,
                            return_: Annot(
                                String(
                                    7,
                                ),
                            ),
                            type_guard: None,
                            effect_: ArbitraryEffect,
                            strictness_kind: Flow,
                        },
                    ),
                ],
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn optional_tuple_elements() {
    let input = r#"
            export type T = [a: number, b?: string];
        "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:16-39]
2. [1:17-26]
3. [1:20-26]
4. [1:28-38]
5. [1:32-38]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "T",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "T",
                tparams: Mono,
                body: Annot(
                    Tuple(
                        AnnotTuple {
                            loc: 1,
                            elems: [
                                TupleElement(
                                    TupleElementData {
                                        loc: 2,
                                        name: Some(
                                            "a",
                                        ),
                                        t: Annot(
                                            Number(
                                                3,
                                            ),
                                        ),
                                        polarity: Neutral,
                                        optional: false,
                                    },
                                ),
                                TupleElement(
                                    TupleElementData {
                                        loc: 4,
                                        name: Some(
                                            "b",
                                        ),
                                        t: Annot(
                                            Optional(
                                                Annot(
                                                    String(
                                                        5,
                                                    ),
                                                ),
                                            ),
                                        ),
                                        polarity: Neutral,
                                        optional: true,
                                    },
                                ),
                            ],
                            inexact: false,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn tuple_spread() {
    let input = r#"
            type S = [string, boolean];
    export type T = [number, ...S];
        "#;
    let expected_output = r#"
Locs:
0. [1:13-14]
1. [1:17-34]
2. [1:18-24]
3. [1:26-33]
4. [2:12-13]
5. [2:16-30]
6. [2:17-23]
7. [2:25-29]
8. [2:28-29]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                1,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "T",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "S",
                tparams: Mono,
                body: Annot(
                    Tuple(
                        AnnotTuple {
                            loc: 1,
                            elems: [
                                TupleElement(
                                    TupleElementData {
                                        loc: 2,
                                        name: None,
                                        t: Annot(
                                            String(
                                                2,
                                            ),
                                        ),
                                        polarity: Neutral,
                                        optional: false,
                                    },
                                ),
                                TupleElement(
                                    TupleElementData {
                                        loc: 3,
                                        name: None,
                                        t: Annot(
                                            Boolean(
                                                3,
                                            ),
                                        ),
                                        polarity: Neutral,
                                        optional: false,
                                    },
                                ),
                            ],
                            inexact: false,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 4,
                custom_error_loc_opt: None,
                name: "T",
                tparams: Mono,
                body: Annot(
                    Tuple(
                        AnnotTuple {
                            loc: 5,
                            elems: [
                                TupleElement(
                                    TupleElementData {
                                        loc: 6,
                                        name: None,
                                        t: Annot(
                                            Number(
                                                6,
                                            ),
                                        ),
                                        polarity: Neutral,
                                        optional: false,
                                    },
                                ),
                                TupleSpread(
                                    TupleSpreadData {
                                        loc: 7,
                                        name: None,
                                        t: TyRef(
                                            Unqualified(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 8,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                        ),
                                    },
                                ),
                            ],
                            inexact: false,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn mapped_types() {
    let input = r#"
            type O = {foo: number, bar: string};
    export type T1 = {[key in keyof O]: O[key]};
    export type T2 = {[key in keyof O]?: O[key]};
    export type T3 = {+[key in keyof O]: O[key]};
    export type T4 = {-[key in keyof O]?: O[key]};
    export type T5 = {[key in O]: O[key]};
        "#;
    let expected_output = r#"
Locs:
0. [1:13-14]
1. [1:17-43]
2. [1:18-21]
3. [1:23-29]
4. [1:31-34]
5. [1:36-42]
6. [2:12-14]
7. [2:18-42]
8. [2:19-22]
9. [2:32-33]
10. [2:36-42]
11. [2:36-37]
12. [2:38-41]
13. [3:12-14]
14. [3:18-43]
15. [3:19-22]
16. [3:32-33]
17. [3:37-43]
18. [3:37-38]
19. [3:39-42]
20. [4:12-14]
21. [4:18-43]
22. [4:20-23]
23. [4:33-34]
24. [4:37-43]
25. [4:37-38]
26. [4:39-42]
27. [5:12-14]
28. [5:18-44]
29. [5:20-23]
30. [5:33-34]
31. [5:38-44]
32. [5:38-39]
33. [5:40-43]
34. [6:12-14]
35. [6:18-36]
36. [6:19-22]
37. [6:26-27]
38. [6:30-36]
39. [6:30-31]
40. [6:32-35]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                1,
            ),
            ExportTypeBinding(
                2,
            ),
            ExportTypeBinding(
                3,
            ),
            ExportTypeBinding(
                4,
            ),
            ExportTypeBinding(
                5,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "T1",
                "T2",
                "T3",
                "T4",
                "T5",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "O",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 1,
                            obj_kind: ExactObj,
                            props: {
                                "bar": ObjAnnotField(
                                    (
                                        4,
                                        Annot(
                                            String(
                                                5,
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                                "foo": ObjAnnotField(
                                    (
                                        2,
                                        Annot(
                                            Number(
                                                3,
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                            },
                            computed_props: [],
                            proto: ObjAnnotImplicitProto,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 6,
                custom_error_loc_opt: None,
                name: "T1",
                tparams: Mono,
                body: Annot(
                    MappedTypeAnnot(
                        AnnotMappedTypeAnnot {
                            loc: 7,
                            source_type: TyRef(
                                Unqualified(
                                    LocalRef(
                                        PackedRefLocal {
                                            ref_loc: 9,
                                            index: 0,
                                        },
                                    ),
                                ),
                            ),
                            property_type: Annot(
                                ElementType(
                                    AnnotElementType {
                                        loc: 10,
                                        obj: TyRef(
                                            Unqualified(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 11,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                        ),
                                        elem: Annot(
                                            Bound(
                                                AnnotBound {
                                                    ref_loc: 12,
                                                    name: "key",
                                                },
                                            ),
                                        ),
                                    },
                                ),
                            ),
                            name_type: None,
                            key_tparam: TParam {
                                name_loc: 8,
                                name: "key",
                                polarity: None,
                                bound: None,
                                default: None,
                                is_const: false,
                            },
                            variance: Neutral,
                            variance_op: None,
                            optional: NoOptionalFlag,
                            inline_keyof: true,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 13,
                custom_error_loc_opt: None,
                name: "T2",
                tparams: Mono,
                body: Annot(
                    MappedTypeAnnot(
                        AnnotMappedTypeAnnot {
                            loc: 14,
                            source_type: TyRef(
                                Unqualified(
                                    LocalRef(
                                        PackedRefLocal {
                                            ref_loc: 16,
                                            index: 0,
                                        },
                                    ),
                                ),
                            ),
                            property_type: Annot(
                                ElementType(
                                    AnnotElementType {
                                        loc: 17,
                                        obj: TyRef(
                                            Unqualified(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 18,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                        ),
                                        elem: Annot(
                                            Bound(
                                                AnnotBound {
                                                    ref_loc: 19,
                                                    name: "key",
                                                },
                                            ),
                                        ),
                                    },
                                ),
                            ),
                            name_type: None,
                            key_tparam: TParam {
                                name_loc: 15,
                                name: "key",
                                polarity: None,
                                bound: None,
                                default: None,
                                is_const: false,
                            },
                            variance: Neutral,
                            variance_op: None,
                            optional: Optional,
                            inline_keyof: true,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 20,
                custom_error_loc_opt: None,
                name: "T3",
                tparams: Mono,
                body: Annot(
                    MappedTypeAnnot(
                        AnnotMappedTypeAnnot {
                            loc: 21,
                            source_type: TyRef(
                                Unqualified(
                                    LocalRef(
                                        PackedRefLocal {
                                            ref_loc: 23,
                                            index: 0,
                                        },
                                    ),
                                ),
                            ),
                            property_type: Annot(
                                ElementType(
                                    AnnotElementType {
                                        loc: 24,
                                        obj: TyRef(
                                            Unqualified(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 25,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                        ),
                                        elem: Annot(
                                            Bound(
                                                AnnotBound {
                                                    ref_loc: 26,
                                                    name: "key",
                                                },
                                            ),
                                        ),
                                    },
                                ),
                            ),
                            name_type: None,
                            key_tparam: TParam {
                                name_loc: 22,
                                name: "key",
                                polarity: None,
                                bound: None,
                                default: None,
                                is_const: false,
                            },
                            variance: Positive,
                            variance_op: None,
                            optional: NoOptionalFlag,
                            inline_keyof: true,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 27,
                custom_error_loc_opt: None,
                name: "T4",
                tparams: Mono,
                body: Annot(
                    MappedTypeAnnot(
                        AnnotMappedTypeAnnot {
                            loc: 28,
                            source_type: TyRef(
                                Unqualified(
                                    LocalRef(
                                        PackedRefLocal {
                                            ref_loc: 30,
                                            index: 0,
                                        },
                                    ),
                                ),
                            ),
                            property_type: Annot(
                                ElementType(
                                    AnnotElementType {
                                        loc: 31,
                                        obj: TyRef(
                                            Unqualified(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 32,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                        ),
                                        elem: Annot(
                                            Bound(
                                                AnnotBound {
                                                    ref_loc: 33,
                                                    name: "key",
                                                },
                                            ),
                                        ),
                                    },
                                ),
                            ),
                            name_type: None,
                            key_tparam: TParam {
                                name_loc: 29,
                                name: "key",
                                polarity: None,
                                bound: None,
                                default: None,
                                is_const: false,
                            },
                            variance: Negative,
                            variance_op: None,
                            optional: Optional,
                            inline_keyof: true,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 34,
                custom_error_loc_opt: None,
                name: "T5",
                tparams: Mono,
                body: Annot(
                    MappedTypeAnnot(
                        AnnotMappedTypeAnnot {
                            loc: 35,
                            source_type: TyRef(
                                Unqualified(
                                    LocalRef(
                                        PackedRefLocal {
                                            ref_loc: 37,
                                            index: 0,
                                        },
                                    ),
                                ),
                            ),
                            property_type: Annot(
                                ElementType(
                                    AnnotElementType {
                                        loc: 38,
                                        obj: TyRef(
                                            Unqualified(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 39,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                        ),
                                        elem: Annot(
                                            Bound(
                                                AnnotBound {
                                                    ref_loc: 40,
                                                    name: "key",
                                                },
                                            ),
                                        ),
                                    },
                                ),
                            ),
                            name_type: None,
                            key_tparam: TParam {
                                name_loc: 36,
                                name: "key",
                                polarity: None,
                                bound: None,
                                default: None,
                                is_const: false,
                            },
                            variance: Neutral,
                            variance_op: None,
                            optional: NoOptionalFlag,
                            inline_keyof: false,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn mapped_types_invalid() {
    let input = r#"
            type O = {foo: number, bar: string};
    export type T = {[key in keyof O]: O[key], foo: number};
        "#;
    let expected_output = r#"
Locs:
0. [2:12-13]
1. [2:16-55]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "T",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "T",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 1,
                            obj_kind: ExactObj,
                            props: {},
                            computed_props: [],
                            proto: ObjAnnotImplicitProto,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn mapped_types_as_gate_off() {
    // Without `experimental.tslib_syntax`, the `as` key-remapping clause is gated off in
    // type_annotation and resolves to `any`. type_sig must mirror that, so an `as` mapped type
    // defined in an imported file does not silently produce a different result than a locally
    // defined one.
    let input = r#"
            type O = {foo: number, bar: string};
    export type Id = {[key in keyof O as key]: O[key]};
        "#;
    let expected_output = r#"
Locs:
0. [2:12-14]
1. [2:18-49]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "Id",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "Id",
                tparams: Mono,
                body: Annot(
                    Any(
                        1,
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn mapped_types_minus_optional() {
    let input = r#"
            type O = {foo: number, bar: string};
    export type T = {[key in keyof O]-?: O[key]};
        "#;
    let expected_output = r#"
Locs:
0. [1:13-14]
1. [1:17-43]
2. [1:18-21]
3. [1:23-29]
4. [1:31-34]
5. [1:36-42]
6. [2:12-13]
7. [2:17-43]
8. [2:18-21]
9. [2:31-32]
10. [2:37-43]
11. [2:37-38]
12. [2:39-42]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                1,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "T",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "O",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 1,
                            obj_kind: ExactObj,
                            props: {
                                "bar": ObjAnnotField(
                                    (
                                        4,
                                        Annot(
                                            String(
                                                5,
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                                "foo": ObjAnnotField(
                                    (
                                        2,
                                        Annot(
                                            Number(
                                                3,
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                            },
                            computed_props: [],
                            proto: ObjAnnotImplicitProto,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 6,
                custom_error_loc_opt: None,
                name: "T",
                tparams: Mono,
                body: Annot(
                    MappedTypeAnnot(
                        AnnotMappedTypeAnnot {
                            loc: 7,
                            source_type: TyRef(
                                Unqualified(
                                    LocalRef(
                                        PackedRefLocal {
                                            ref_loc: 9,
                                            index: 0,
                                        },
                                    ),
                                ),
                            ),
                            property_type: Annot(
                                ElementType(
                                    AnnotElementType {
                                        loc: 10,
                                        obj: TyRef(
                                            Unqualified(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 11,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                        ),
                                        elem: Annot(
                                            Bound(
                                                AnnotBound {
                                                    ref_loc: 12,
                                                    name: "key",
                                                },
                                            ),
                                        ),
                                    },
                                ),
                            ),
                            name_type: None,
                            key_tparam: TParam {
                                name_loc: 8,
                                name: "key",
                                polarity: None,
                                bound: None,
                                default: None,
                                is_const: false,
                            },
                            variance: Neutral,
                            variance_op: None,
                            optional: MinusOptional,
                            inline_keyof: true,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn dirtify_defs() {
    use flow_parser::loc::Loc;
    use flow_parser::loc::Position;

    let input = r#"
            export type OOOOOO = {foo: number, bar: string};
export const aaaaa = {foo: 3, bar: ""};
export const {foo} = {foo: 3};
        "#;
    let expected_output = r#"
Locs:
0. [1:24-30]
1. [1:33-59]
2. [1:34-37]
3. [1:39-45]
4. [1:47-50]
5. [1:52-58]
6. [2:13-18]
7. [2:21-38]
8. [2:22-25]
9. [2:27-28]
10. [2:30-33]
11. [2:35-37]
12. [3:14-17]
13. [3:21-29]
14. [3:22-25]
15. [3:27-28]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
        ],
        exports: [
            ExportBinding(
                1,
            ),
            ExportBinding(
                2,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [
                "OOOOOO",
            ],
            type_stars: [],
            export_keys: [
                "aaaaa",
                "foo",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "OOOOOO",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 1,
                            obj_kind: ExactObj,
                            props: {
                                "bar": ObjAnnotField(
                                    (
                                        4,
                                        Annot(
                                            String(
                                                5,
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                                "foo": ObjAnnotField(
                                    (
                                        2,
                                        Annot(
                                            Number(
                                                3,
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                            },
                            computed_props: [],
                            proto: ObjAnnotImplicitProto,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        Variable(
            DefVariable {
                id_loc: 6,
                name: "aaaaa",
                def: Value(
                    ObjLit(
                        ValueObjLit {
                            loc: 7,
                            frozen: false,
                            proto: None,
                            props: {
                                "bar": ObjValueField(
                                    (
                                        10,
                                        Value(
                                            StringLit(
                                                (
                                                    11,
                                                    "",
                                                ),
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                                "foo": ObjValueField(
                                    (
                                        8,
                                        Value(
                                            NumberLit(
                                                (
                                                    9,
                                                    3.0,
                                                    "3",
                                                ),
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                            },
                            strictness_kind: Flow,
                        },
                    ),
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 12,
                name: "foo",
                def: Pattern(
                    1,
                ),
            },
        ),
    ],
    dirty_local_defs: [
        0,
        1,
    ],
    remote_refs: [],
    pattern_defs: [
        Value(
            ObjLit(
                ValueObjLit {
                    loc: 13,
                    frozen: false,
                    proto: None,
                    props: {
                        "foo": ObjValueField(
                            (
                                14,
                                Value(
                                    NumberLit(
                                        (
                                            15,
                                            3.0,
                                            "3",
                                        ),
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    strictness_kind: Flow,
                },
            ),
        ),
    ],
    dirty_pattern_defs: [
        0,
    ],
    patterns: [
        PDef(
            0,
        ),
        PropP {
            id_loc: 12,
            name: "foo",
            def: 0,
            strictness_kind: Flow,
        },
    ],
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_options(
            input,
            Some(|opts: &mut TypeSigOptions| {
                // Mark locations as dirty:
                // [1:25-26] corresponds to "OOOOOO" type alias id_loc
                // [2:25-26] corresponds to location within "aaaaa" variable
                // [3:25-26] corresponds to location within pattern def
                opts.locs_to_dirtify = vec![
                    Loc {
                        source: None,
                        start: Position {
                            line: 1,
                            column: 25,
                        },
                        end: Position {
                            line: 1,
                            column: 26,
                        },
                    },
                    Loc {
                        source: None,
                        start: Position {
                            line: 2,
                            column: 25,
                        },
                        end: Position {
                            line: 2,
                            column: 26,
                        },
                    },
                    Loc {
                        source: None,
                        start: Position {
                            line: 3,
                            column: 25,
                        },
                        end: Position {
                            line: 3,
                            column: 26,
                        },
                    },
                ];
            })
        ))
    )
}

#[test]
fn component() {
    let input = r#"
            component Baz() {};
    module.exports = { Baz };
        "#;
    let expected_output = r#"
Locs:
0. [1:8-23]
1. [1:18-21]
2. [1:21-23]
3. [1:23]
4. [2:17-24]
5. [2:19-22]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Value(
                ObjLit(
                    ValueObjLit {
                        loc: 4,
                        frozen: false,
                        proto: None,
                        props: {
                            "Baz": ObjValueField(
                                (
                                    5,
                                    Ref(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 5,
                                                index: 0,
                                            },
                                        ),
                                    ),
                                    Neutral,
                                ),
                            ),
                        },
                        strictness_kind: Flow,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ComponentBinding(
            DefComponentBinding {
                id_loc: 1,
                name: "Baz",
                fn_loc: 0,
                def: ComponentSig {
                    params_loc: 2,
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    renders: Annot(
                        ComponentMissingRenders(
                            3,
                        ),
                    ),
                    strictness_kind: Flow,
                },
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn component2() {
    let input = r#"
            type Rest = { x: number }
    component Baz(x: string, ...props: Rest) {};
    module.exports = { Baz };
        "#;
    let expected_output = r#"
Locs:
0. [1:13-17]
1. [1:20-33]
2. [1:22-23]
3. [1:25-31]
4. [2:0-40]
5. [2:10-13]
6. [2:13-40]
7. [2:14-15]
8. [2:17-23]
9. [2:35-39]
10. [2:40]
11. [3:17-24]
12. [3:19-22]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Value(
                ObjLit(
                    ValueObjLit {
                        loc: 11,
                        frozen: false,
                        proto: None,
                        props: {
                            "Baz": ObjValueField(
                                (
                                    12,
                                    Ref(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 12,
                                                index: 1,
                                            },
                                        ),
                                    ),
                                    Neutral,
                                ),
                            ),
                        },
                        strictness_kind: Flow,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "Rest",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 1,
                            obj_kind: ExactObj,
                            props: {
                                "x": ObjAnnotField(
                                    (
                                        2,
                                        Annot(
                                            Number(
                                                3,
                                            ),
                                        ),
                                        Neutral,
                                    ),
                                ),
                            },
                            computed_props: [],
                            proto: ObjAnnotImplicitProto,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        ComponentBinding(
            DefComponentBinding {
                id_loc: 5,
                name: "Baz",
                fn_loc: 4,
                def: ComponentSig {
                    params_loc: 6,
                    tparams: Mono,
                    params: [
                        ComponentParam {
                            name: "x",
                            name_loc: 7,
                            t: Annot(
                                String(
                                    8,
                                ),
                            ),
                        },
                    ],
                    rest_param: Some(
                        ComponentRestParam {
                            t: TyRef(
                                Unqualified(
                                    LocalRef(
                                        PackedRefLocal {
                                            ref_loc: 9,
                                            index: 0,
                                        },
                                    ),
                                ),
                            ),
                        },
                    ),
                    renders: Annot(
                        ComponentMissingRenders(
                            10,
                        ),
                    ),
                    strictness_kind: Flow,
                },
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn component3() {
    let input = r#"
            component RadComp() { };
    component Baz('lets go' as x: string, ...props: Rest) renders RadComp {};
    Baz.static = "amazing";
    module.exports = { Baz };
        "#;
    let expected_output = r#"
Locs:
0. [1:8-27]
1. [1:18-25]
2. [1:25-27]
3. [1:27]
4. [2:0-69]
5. [2:10-13]
6. [2:13-53]
7. [2:14-23]
8. [2:30-36]
9. [2:48-52]
10. [2:54-69]
11. [2:62-69]
12. [4:17-24]
13. [4:19-22]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Value(
                ObjLit(
                    ValueObjLit {
                        loc: 12,
                        frozen: false,
                        proto: None,
                        props: {
                            "Baz": ObjValueField(
                                (
                                    13,
                                    Ref(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 13,
                                                index: 1,
                                            },
                                        ),
                                    ),
                                    Neutral,
                                ),
                            ),
                        },
                        strictness_kind: Flow,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ComponentBinding(
            DefComponentBinding {
                id_loc: 1,
                name: "RadComp",
                fn_loc: 0,
                def: ComponentSig {
                    params_loc: 2,
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    renders: Annot(
                        ComponentMissingRenders(
                            3,
                        ),
                    ),
                    strictness_kind: Flow,
                },
            },
        ),
        ComponentBinding(
            DefComponentBinding {
                id_loc: 5,
                name: "Baz",
                fn_loc: 4,
                def: ComponentSig {
                    params_loc: 6,
                    tparams: Mono,
                    params: [
                        ComponentParam {
                            name: "lets go",
                            name_loc: 7,
                            t: Annot(
                                String(
                                    8,
                                ),
                            ),
                        },
                    ],
                    rest_param: Some(
                        ComponentRestParam {
                            t: TyRef(
                                Unqualified(
                                    BuiltinRef(
                                        PackedRefBuiltin {
                                            ref_loc: 9,
                                            type_ref: true,
                                            name: "Rest",
                                        },
                                    ),
                                ),
                            ),
                        },
                    ),
                    renders: Annot(
                        Renders(
                            AnnotRenders {
                                loc: 10,
                                arg: TyRef(
                                    Unqualified(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 11,
                                                index: 0,
                                            },
                                        ),
                                    ),
                                ),
                                variant: Normal,
                            },
                        ),
                    ),
                    strictness_kind: Flow,
                },
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn component4() {
    let input = r#"
            component Baz<T>(prop: T) renders T {};
    module.exports = { Baz };
        "#;
    let expected_output = r#"
Locs:
0. [1:8-43]
1. [1:18-21]
2. [1:21-24]
3. [1:22-23]
4. [1:24-33]
5. [1:25-29]
6. [1:31-32]
7. [1:34-43]
8. [1:42-43]
9. [2:17-24]
10. [2:19-22]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Value(
                ObjLit(
                    ValueObjLit {
                        loc: 9,
                        frozen: false,
                        proto: None,
                        props: {
                            "Baz": ObjValueField(
                                (
                                    10,
                                    Ref(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 10,
                                                index: 0,
                                            },
                                        ),
                                    ),
                                    Neutral,
                                ),
                            ),
                        },
                        strictness_kind: Flow,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ComponentBinding(
            DefComponentBinding {
                id_loc: 1,
                name: "Baz",
                fn_loc: 0,
                def: ComponentSig {
                    params_loc: 4,
                    tparams: Poly(
                        (
                            2,
                            [
                                TParam {
                                    name_loc: 3,
                                    name: "T",
                                    polarity: None,
                                    bound: None,
                                    default: None,
                                    is_const: false,
                                },
                            ],
                        ),
                    ),
                    params: [
                        ComponentParam {
                            name: "prop",
                            name_loc: 5,
                            t: Annot(
                                Bound(
                                    AnnotBound {
                                        ref_loc: 6,
                                        name: "T",
                                    },
                                ),
                            ),
                        },
                    ],
                    rest_param: None,
                    renders: Annot(
                        Renders(
                            AnnotRenders {
                                loc: 7,
                                arg: Annot(
                                    Bound(
                                        AnnotBound {
                                            ref_loc: 8,
                                            name: "T",
                                        },
                                    ),
                                ),
                                variant: Normal,
                            },
                        ),
                    ),
                    strictness_kind: Flow,
                },
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn component5() {
    let input = r#"
            export component Baz () { }
    export default component Bar () { }
        "#;
    let expected_output = r#"
Locs:
0. [1:15-31]
1. [1:25-28]
2. [1:29-31]
3. [1:31]
4. [2:7-14]
5. [2:15-31]
6. [2:25-28]
7. [2:29-31]
8. [2:31]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
            ExportDefaultBinding(
                ExportDefaultBindingData {
                    default_loc: 4,
                    index: 1,
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "Baz",
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ComponentBinding(
            DefComponentBinding {
                id_loc: 1,
                name: "Baz",
                fn_loc: 0,
                def: ComponentSig {
                    params_loc: 2,
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    renders: Annot(
                        ComponentMissingRenders(
                            3,
                        ),
                    ),
                    strictness_kind: Flow,
                },
            },
        ),
        ComponentBinding(
            DefComponentBinding {
                id_loc: 6,
                name: "Bar",
                fn_loc: 5,
                def: ComponentSig {
                    params_loc: 7,
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    renders: Annot(
                        ComponentMissingRenders(
                            8,
                        ),
                    ),
                    strictness_kind: Flow,
                },
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn component_disabled() {
    let input = r#"
            component Baz() {};
    module.exports = { Baz };
        "#;
    let expected_output = r#"
Locs:
0. [1:18-21]
1. [2:17-24]
2. [2:19-22]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [],
        exports: Some(
            Value(
                ObjLit(
                    ValueObjLit {
                        loc: 1,
                        frozen: false,
                        proto: None,
                        props: {
                            "Baz": ObjValueField(
                                (
                                    2,
                                    Ref(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 2,
                                                index: 0,
                                            },
                                        ),
                                    ),
                                    Neutral,
                                ),
                            ),
                        },
                        strictness_kind: Flow,
                    },
                ),
            ),
        ),
        info: CJSModuleInfo {
            type_export_keys: [],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        DisabledComponentBinding(
            DefDisabledComponentBinding {
                id_loc: 0,
                name: "Baz",
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_options(
            input,
            Some(|opts: &mut TypeSigOptions| {
                opts.enable_component_syntax = false;
                opts.component_syntax_enabled_in_config = false;
            })
        ))
    )
}

#[test]
fn component_disabled2() {
    let input = r#"
            export component Baz () { }
    export default component Bar () { }
        "#;
    let expected_output = r#"
Locs:
0. [1:25-28]
1. [2:7-14]
2. [2:25-28]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
            ExportDefaultBinding(
                ExportDefaultBindingData {
                    default_loc: 1,
                    index: 1,
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "Baz",
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        DisabledComponentBinding(
            DefDisabledComponentBinding {
                id_loc: 0,
                name: "Baz",
            },
        ),
        DisabledComponentBinding(
            DefDisabledComponentBinding {
                id_loc: 2,
                name: "Bar",
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_sig_with_options(
            input,
            Some(|opts: &mut TypeSigOptions| {
                opts.enable_component_syntax = false;
                opts.component_syntax_enabled_in_config = false;
            })
        ))
    )
}

#[test]
fn declare_component() {
    let input = r#"
            declare export component Baz ();
    declare export default component Bar ();
        "#;
    let expected_output = r#"
Locs:
0. [1:23-40]
1. [1:33-36]
2. [1:37-39]
3. [1:39]
4. [2:15-22]
5. [2:23-40]
6. [2:33-36]
7. [2:37-39]
8. [2:39]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
            ExportDefaultBinding(
                ExportDefaultBindingData {
                    default_loc: 4,
                    index: 1,
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "Baz",
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        ComponentBinding(
            DefComponentBinding {
                id_loc: 1,
                name: "Baz",
                fn_loc: 0,
                def: ComponentSig {
                    params_loc: 2,
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    renders: Annot(
                        ComponentMissingRenders(
                            3,
                        ),
                    ),
                    strictness_kind: Flow,
                },
            },
        ),
        ComponentBinding(
            DefComponentBinding {
                id_loc: 6,
                name: "Bar",
                fn_loc: 5,
                def: ComponentSig {
                    params_loc: 7,
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    renders: Annot(
                        ComponentMissingRenders(
                            8,
                        ),
                    ),
                    strictness_kind: Flow,
                },
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn component_type() {
    let input = r#"
            type A = number
    type B = string
    declare export var Baz: component(x: A) renders B;
    declare var Bar: component();
    type Props = {};
    export type Mono = component(...Props);
    export type Poly<Props> = component(...Props);
    declare export default Bar;
        "#;
    let expected_output = r#"
Locs:
0. [1:13-14]
1. [1:17-23]
2. [2:5-6]
3. [2:9-15]
4. [3:19-22]
5. [3:24-49]
6. [3:33-39]
7. [3:34-35]
8. [3:37-38]
9. [3:40-49]
10. [3:48-49]
11. [4:12-15]
12. [4:17-28]
13. [4:26-28]
14. [4:28]
15. [5:5-10]
16. [5:13-15]
17. [6:12-16]
18. [6:19-38]
19. [6:28-38]
20. [6:32-37]
21. [6:38]
22. [7:12-16]
23. [7:16-23]
24. [7:17-22]
25. [7:26-45]
26. [7:35-45]
27. [7:39-44]
28. [7:45]
29. [8:15-22]
30. [8:23-26]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [
            ExportTypeBinding(
                5,
            ),
            ExportTypeBinding(
                6,
            ),
        ],
        exports: [
            ExportBinding(
                2,
            ),
            ExportDefault(
                ExportDefaultData {
                    default_loc: 29,
                    def: TyRef(
                        Unqualified(
                            LocalRef(
                                PackedRefLocal {
                                    ref_loc: 30,
                                    index: 3,
                                },
                            ),
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [
                "Mono",
                "Poly",
            ],
            type_stars: [],
            export_keys: [
                "Baz",
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "A",
                tparams: Mono,
                body: Annot(
                    Number(
                        1,
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 2,
                custom_error_loc_opt: None,
                name: "B",
                tparams: Mono,
                body: Annot(
                    String(
                        3,
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        Variable(
            DefVariable {
                id_loc: 4,
                name: "Baz",
                def: Annot(
                    ComponentAnnot(
                        (
                            5,
                            ComponentSig {
                                params_loc: 6,
                                tparams: Mono,
                                params: [
                                    ComponentParam {
                                        name: "x",
                                        name_loc: 7,
                                        t: TyRef(
                                            Unqualified(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 8,
                                                        index: 0,
                                                    },
                                                ),
                                            ),
                                        ),
                                    },
                                ],
                                rest_param: None,
                                renders: Annot(
                                    Renders(
                                        AnnotRenders {
                                            loc: 9,
                                            arg: TyRef(
                                                Unqualified(
                                                    LocalRef(
                                                        PackedRefLocal {
                                                            ref_loc: 10,
                                                            index: 1,
                                                        },
                                                    ),
                                                ),
                                            ),
                                            variant: Normal,
                                        },
                                    ),
                                ),
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 11,
                name: "Bar",
                def: Annot(
                    ComponentAnnot(
                        (
                            12,
                            ComponentSig {
                                params_loc: 13,
                                tparams: Mono,
                                params: [],
                                rest_param: None,
                                renders: Annot(
                                    ComponentMissingRenders(
                                        14,
                                    ),
                                ),
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                ),
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 15,
                custom_error_loc_opt: None,
                name: "Props",
                tparams: Mono,
                body: Annot(
                    ObjAnnot(
                        AnnotObjAnnot {
                            loc: 16,
                            obj_kind: ExactObj,
                            props: {},
                            computed_props: [],
                            proto: ObjAnnotImplicitProto,
                            strictness_kind: Flow,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 17,
                custom_error_loc_opt: None,
                name: "Mono",
                tparams: Mono,
                body: Annot(
                    ComponentAnnot(
                        (
                            18,
                            ComponentSig {
                                params_loc: 19,
                                tparams: Mono,
                                params: [],
                                rest_param: Some(
                                    ComponentRestParam {
                                        t: TyRef(
                                            Unqualified(
                                                LocalRef(
                                                    PackedRefLocal {
                                                        ref_loc: 20,
                                                        index: 4,
                                                    },
                                                ),
                                            ),
                                        ),
                                    },
                                ),
                                renders: Annot(
                                    ComponentMissingRenders(
                                        21,
                                    ),
                                ),
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 22,
                custom_error_loc_opt: None,
                name: "Poly",
                tparams: Poly(
                    (
                        23,
                        [
                            TParam {
                                name_loc: 24,
                                name: "Props",
                                polarity: None,
                                bound: None,
                                default: None,
                                is_const: false,
                            },
                        ],
                    ),
                ),
                body: Annot(
                    ComponentAnnot(
                        (
                            25,
                            ComponentSig {
                                params_loc: 26,
                                tparams: Mono,
                                params: [],
                                rest_param: Some(
                                    ComponentRestParam {
                                        t: Annot(
                                            Bound(
                                                AnnotBound {
                                                    ref_loc: 27,
                                                    name: "Props",
                                                },
                                            ),
                                        ),
                                    },
                                ),
                                renders: Annot(
                                    ComponentMissingRenders(
                                        28,
                                    ),
                                ),
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn declare_component_2() {
    let input = r#"
            declare export var Baz: component();
    declare var Bar: component();
    declare export default Bar;
        "#;
    let expected_output = r#"
Locs:
0. [1:27-30]
1. [1:32-43]
2. [1:41-43]
3. [1:43]
4. [2:12-15]
5. [2:17-28]
6. [2:26-28]
7. [2:28]
8. [3:15-22]
9. [3:23-26]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
            ExportDefault(
                ExportDefaultData {
                    default_loc: 8,
                    def: TyRef(
                        Unqualified(
                            LocalRef(
                                PackedRefLocal {
                                    ref_loc: 9,
                                    index: 1,
                                },
                            ),
                        ),
                    ),
                },
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "Baz",
                "default",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "Baz",
                def: Annot(
                    ComponentAnnot(
                        (
                            1,
                            ComponentSig {
                                params_loc: 2,
                                tparams: Mono,
                                params: [],
                                rest_param: None,
                                renders: Annot(
                                    ComponentMissingRenders(
                                        3,
                                    ),
                                ),
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                ),
            },
        ),
        Variable(
            DefVariable {
                id_loc: 4,
                name: "Bar",
                def: Annot(
                    ComponentAnnot(
                        (
                            5,
                            ComponentSig {
                                params_loc: 6,
                                tparams: Mono,
                                params: [],
                                rest_param: None,
                                renders: Annot(
                                    ComponentMissingRenders(
                                        7,
                                    ),
                                ),
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn function_const_type_param() {
    let input = r#"
            export function foo<const X>(x: X): X { return x; };
        "#;
    let expected_output = r#"
Locs:
0. [1:7-37]
1. [1:16-19]
2. [1:19-28]
3. [1:26-27]
4. [1:32-33]
5. [1:36-37]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "foo",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        FunBinding(
            DefFunBinding {
                id_loc: 1,
                name: "foo",
                async_: false,
                generator: false,
                fn_loc: 0,
                def: FunSig {
                    tparams: Poly(
                        (
                            2,
                            [
                                TParam {
                                    name_loc: 3,
                                    name: "X",
                                    polarity: None,
                                    bound: None,
                                    default: None,
                                    is_const: true,
                                },
                            ],
                        ),
                    ),
                    params: [
                        FunParam {
                            name: Some(
                                "x",
                            ),
                            t: Annot(
                                Bound(
                                    AnnotBound {
                                        ref_loc: 4,
                                        name: "X",
                                    },
                                ),
                            ),
                        },
                    ],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Bound(
                            AnnotBound {
                                ref_loc: 5,
                                name: "X",
                            },
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
                statics: {},
                namespace_types: {},
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn union_annot() {
    let input = r#"
            declare export var a: string | number | null;
        "#;
    let expected_output = r#"
Locs:
0. [1:19-20]
1. [1:22-44]
2. [1:22-28]
3. [1:31-37]
4. [1:40-44]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [],
        exports: [
            ExportBinding(
                0,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [],
            type_stars: [],
            export_keys: [
                "a",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        Variable(
            DefVariable {
                id_loc: 0,
                name: "a",
                def: Annot(
                    Union(
                        AnnotUnion {
                            loc: 1,
                            t0: Annot(
                                String(
                                    2,
                                ),
                            ),
                            t1: Annot(
                                Number(
                                    3,
                                ),
                            ),
                            ts: [
                                Annot(
                                    Null(
                                        4,
                                    ),
                                ),
                            ],
                        },
                    ),
                ),
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn render_types() {
    let input = r#"
            export type X = renders number;
    export type Y = renders number | string;
    export type Z = renders (number | string);
        "#;
    let expected_output = r#"
Locs:
0. [1:20-21]
1. [1:24-38]
2. [1:32-38]
3. [2:12-13]
4. [2:16-39]
5. [2:16-30]
6. [2:24-30]
7. [2:33-39]
8. [3:12-13]
9. [3:16-41]
10. [3:25-40]
11. [3:25-31]
12. [3:34-40]
Type Sig:
Module {
    module_kind: CJSModule {
        type_exports: [
            ExportTypeBinding(
                0,
            ),
            ExportTypeBinding(
                1,
            ),
            ExportTypeBinding(
                2,
            ),
        ],
        exports: None,
        info: CJSModuleInfo {
            type_export_keys: [
                "X",
                "Y",
                "Z",
            ],
            type_stars: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "X",
                tparams: Mono,
                body: Annot(
                    Renders(
                        AnnotRenders {
                            loc: 1,
                            arg: Annot(
                                Number(
                                    2,
                                ),
                            ),
                            variant: Normal,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 3,
                custom_error_loc_opt: None,
                name: "Y",
                tparams: Mono,
                body: Annot(
                    Union(
                        AnnotUnion {
                            loc: 4,
                            t0: Annot(
                                Renders(
                                    AnnotRenders {
                                        loc: 5,
                                        arg: Annot(
                                            Number(
                                                6,
                                            ),
                                        ),
                                        variant: Normal,
                                    },
                                ),
                            ),
                            t1: Annot(
                                String(
                                    7,
                                ),
                            ),
                            ts: [],
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 8,
                custom_error_loc_opt: None,
                name: "Z",
                tparams: Mono,
                body: Annot(
                    Renders(
                        AnnotRenders {
                            loc: 9,
                            arg: Annot(
                                Union(
                                    AnnotUnion {
                                        loc: 10,
                                        t0: Annot(
                                            Number(
                                                11,
                                            ),
                                        ),
                                        t1: Annot(
                                            String(
                                                12,
                                            ),
                                        ),
                                        ts: [],
                                    },
                                ),
                            ),
                            variant: Normal,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn render_maybe_types() {
    let input = r#"
            export type X = renders? number;
    export type Y = renders? number | string;
    export type Z = renders? (number | string);
    export type C = component() renders? number;
    declare export component Foo() renders? number;
    export component Bar() renders? number { return null }
        "#;
    let expected_output = r#"
Locs:
0. [1:20-21]
1. [1:24-39]
2. [1:33-39]
3. [2:12-13]
4. [2:16-40]
5. [2:16-31]
6. [2:25-31]
7. [2:34-40]
8. [3:12-13]
9. [3:16-42]
10. [3:26-41]
11. [3:26-32]
12. [3:35-41]
13. [4:12-13]
14. [4:16-43]
15. [4:25-27]
16. [4:28-43]
17. [4:37-43]
18. [5:15-47]
19. [5:25-28]
20. [5:28-30]
21. [5:31-46]
22. [5:40-46]
23. [6:7-38]
24. [6:17-20]
25. [6:20-22]
26. [6:23-38]
27. [6:32-38]
Type Sig:
Module {
    module_kind: ESModule {
        type_exports: [
            ExportTypeBinding(
                3,
            ),
            ExportTypeBinding(
                0,
            ),
            ExportTypeBinding(
                1,
            ),
            ExportTypeBinding(
                2,
            ),
        ],
        exports: [
            ExportBinding(
                5,
            ),
            ExportBinding(
                4,
            ),
        ],
        ts_pending: [],
        info: ESModuleInfo {
            type_export_keys: [
                "C",
                "X",
                "Y",
                "Z",
            ],
            type_stars: [],
            export_keys: [
                "Bar",
                "Foo",
            ],
            stars: [],
            ts_pending_keys: [],
            strict: true,
            platform_availability_set: None,
            strictness_kind: Flow,
        },
    },
    module_refs: [],
    local_defs: [
        TypeAlias(
            DefTypeAlias {
                id_loc: 0,
                custom_error_loc_opt: None,
                name: "X",
                tparams: Mono,
                body: Annot(
                    Renders(
                        AnnotRenders {
                            loc: 1,
                            arg: Annot(
                                Number(
                                    2,
                                ),
                            ),
                            variant: Maybe,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 3,
                custom_error_loc_opt: None,
                name: "Y",
                tparams: Mono,
                body: Annot(
                    Union(
                        AnnotUnion {
                            loc: 4,
                            t0: Annot(
                                Renders(
                                    AnnotRenders {
                                        loc: 5,
                                        arg: Annot(
                                            Number(
                                                6,
                                            ),
                                        ),
                                        variant: Maybe,
                                    },
                                ),
                            ),
                            t1: Annot(
                                String(
                                    7,
                                ),
                            ),
                            ts: [],
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 8,
                custom_error_loc_opt: None,
                name: "Z",
                tparams: Mono,
                body: Annot(
                    Renders(
                        AnnotRenders {
                            loc: 9,
                            arg: Annot(
                                Union(
                                    AnnotUnion {
                                        loc: 10,
                                        t0: Annot(
                                            Number(
                                                11,
                                            ),
                                        ),
                                        t1: Annot(
                                            String(
                                                12,
                                            ),
                                        ),
                                        ts: [],
                                    },
                                ),
                            ),
                            variant: Maybe,
                        },
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        TypeAlias(
            DefTypeAlias {
                id_loc: 13,
                custom_error_loc_opt: None,
                name: "C",
                tparams: Mono,
                body: Annot(
                    ComponentAnnot(
                        (
                            14,
                            ComponentSig {
                                params_loc: 15,
                                tparams: Mono,
                                params: [],
                                rest_param: None,
                                renders: Annot(
                                    Renders(
                                        AnnotRenders {
                                            loc: 16,
                                            arg: Annot(
                                                Number(
                                                    17,
                                                ),
                                            ),
                                            variant: Maybe,
                                        },
                                    ),
                                ),
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                ),
                strictness_kind: Flow,
            },
        ),
        ComponentBinding(
            DefComponentBinding {
                id_loc: 19,
                name: "Foo",
                fn_loc: 18,
                def: ComponentSig {
                    params_loc: 20,
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    renders: Annot(
                        Renders(
                            AnnotRenders {
                                loc: 21,
                                arg: Annot(
                                    Number(
                                        22,
                                    ),
                                ),
                                variant: Maybe,
                            },
                        ),
                    ),
                    strictness_kind: Flow,
                },
            },
        ),
        ComponentBinding(
            DefComponentBinding {
                id_loc: 24,
                name: "Bar",
                fn_loc: 23,
                def: ComponentSig {
                    params_loc: 25,
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    renders: Annot(
                        Renders(
                            AnnotRenders {
                                loc: 26,
                                arg: Annot(
                                    Number(
                                        27,
                                    ),
                                ),
                                variant: Maybe,
                            },
                        ),
                    ),
                    strictness_kind: Flow,
                },
            },
        ),
    ],
    dirty_local_defs: [],
    remote_refs: [],
    pattern_defs: [],
    dirty_pattern_defs: [],
    patterns: [],
}
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&print_sig(input)))
}

#[test]
fn builtins() {
    let input = r#"
        declare var x: T;
        type T = string;
    "#;
    let expected_output = r#"
Locs:
0. [1:12-13]
1. [1:15-16]
2. [2:5-6]
3. [2:9-15]
4. [0:0]
Local defs:
0. Variable(
    DefVariable {
        id_loc: 0,
        name: "x",
        def: TyRef(
            Unqualified(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 1,
                        index: 1,
                    },
                ),
            ),
        ),
    },
)
1. TypeAlias(
    DefTypeAlias {
        id_loc: 2,
        custom_error_loc_opt: None,
        name: "T",
        tparams: Mono,
        body: Annot(
            String(
                3,
            ),
        ),
        strictness_kind: Flow,
    },
)
2. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 4,
        name: "globalThis",
        values: {
            "globalThis": (
                4,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 4,
                            index: 2,
                        },
                    ),
                ),
            ),
            "x": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
        },
        types: {
            "T": (
                2,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 2,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin global value x
Builtin global type T
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtins_ignore_name_def_for_use_special_cased_names() {
    let input = r#"
        type T1 = Array<string>;
        type T2 = $ReadOnly<{foo: bar}>;
        declare class Array {}
        type $ReadOnly = number;
    "#;
    let expected_output = r#"
Locs:
0. [1:5-7]
1. [1:10-23]
2. [1:16-22]
3. [2:5-7]
4. [2:10-31]
5. [2:20-30]
6. [2:21-24]
7. [2:26-29]
8. [3:14-19]
9. [4:5-14]
10. [4:17-23]
11. [0:0]
Local defs:
0. TypeAlias(
    DefTypeAlias {
        id_loc: 0,
        custom_error_loc_opt: None,
        name: "T1",
        tparams: Mono,
        body: Annot(
            Array(
                (
                    1,
                    Annot(
                        String(
                            2,
                        ),
                    ),
                    Flow,
                ),
            ),
        ),
        strictness_kind: Flow,
    },
)
1. TypeAlias(
    DefTypeAlias {
        id_loc: 3,
        custom_error_loc_opt: None,
        name: "T2",
        tparams: Mono,
        body: Annot(
            ReadOnly(
                (
                    4,
                    Annot(
                        ObjAnnot(
                            AnnotObjAnnot {
                                loc: 5,
                                obj_kind: ExactObj,
                                props: {
                                    "foo": ObjAnnotField(
                                        (
                                            6,
                                            TyRef(
                                                Unqualified(
                                                    BuiltinRef(
                                                        PackedRefBuiltin {
                                                            ref_loc: 7,
                                                            type_ref: true,
                                                            name: "bar",
                                                        },
                                                    ),
                                                ),
                                            ),
                                            Neutral,
                                        ),
                                    ),
                                },
                                computed_props: [],
                                proto: ObjAnnotImplicitProto,
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                ),
            ),
        ),
        strictness_kind: Flow,
    },
)
2. DeclareClassBinding(
    DefDeclareClassBinding {
        id_loc: 8,
        name: "Array",
        def: DeclareClassSig {
            tparams: Mono,
            extends: ClassImplicitExtends,
            mixins: [],
            implements: [],
            static_props: {},
            own_props: {},
            proto_props: {},
            computed_own_props: [],
            computed_proto_props: [],
            computed_static_props: [],
            static_calls: [],
            calls: [],
            constructs: [],
            dict: None,
            static_dict: None,
            abstract_: false,
            abstract_props: {},
            strictness_kind: Flow,
        },
        namespace_types: {},
    },
)
3. TypeAlias(
    DefTypeAlias {
        id_loc: 9,
        custom_error_loc_opt: None,
        name: "$ReadOnly",
        tparams: Mono,
        body: Annot(
            Number(
                10,
            ),
        ),
        strictness_kind: Flow,
    },
)
4. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 11,
        name: "globalThis",
        values: {
            "Array": (
                8,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 8,
                            index: 2,
                        },
                    ),
                ),
            ),
            "globalThis": (
                11,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 11,
                            index: 4,
                        },
                    ),
                ),
            ),
        },
        types: {
            "$ReadOnly": (
                9,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 9,
                            index: 3,
                        },
                    ),
                ),
            ),
            "T1": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
            "T2": (
                3,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 3,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
Builtin global value Array
Builtin global value globalThis
Builtin global type $ReadOnly
Builtin global type T1
Builtin global type T2
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_cjs_module() {
    let input = r#"
        type T = string;
        declare module foo {
          declare module.exports: T;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:5-6]
1. [1:9-15]
2. [2:15-18]
3. [3:26-27]
4. [0:0]
Local defs:
0. TypeAlias(
    DefTypeAlias {
        id_loc: 0,
        custom_error_loc_opt: None,
        name: "T",
        tparams: Mono,
        body: Annot(
            String(
                1,
            ),
        ),
        strictness_kind: Flow,
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 4,
        name: "globalThis",
        values: {
            "globalThis": (
                4,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 4,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {
            "T": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin global type T
Builtin module foo:
Loc: 2
CJSModule {
    type_exports: [],
    exports: Some(
        TyRef(
            Unqualified(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 3,
                        index: 0,
                    },
                ),
            ),
        ),
    ),
    info: CJSModuleInfo {
        type_export_keys: [],
        type_stars: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: Flow,
    },
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_cjs_ignore_later() {
    let input = r#"
        type T = string;
        declare module foo {
          declare module.exports: string;
        }
        declare module foo {
          declare module.exports: number;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:5-6]
1. [1:9-15]
2. [2:15-18]
3. [3:26-32]
4. [5:15-18]
5. [0:0]
Local defs:
0. TypeAlias(
    DefTypeAlias {
        id_loc: 0,
        custom_error_loc_opt: None,
        name: "T",
        tparams: Mono,
        body: Annot(
            String(
                1,
            ),
        ),
        strictness_kind: Flow,
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 5,
        name: "globalThis",
        values: {
            "globalThis": (
                5,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 5,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {
            "T": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin global type T
Builtin module foo:
Loc: 2
CJSModule {
    type_exports: [],
    exports: Some(
        Annot(
            String(
                3,
            ),
        ),
    ),
    info: CJSModuleInfo {
        type_export_keys: [],
        type_stars: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: Flow,
    },
}
Errors:
BindingValidationError(ModuleOverride { name: "foo", override_binding_loc: 2, existing_binding_loc: 4 })
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_cjs_module_auto_export_type() {
    let input = r#"
        declare module foo {
          declare type T1 = number;
          export type T2 = string;
          declare module.exports: string;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:15-18]
1. [2:15-17]
2. [2:20-26]
3. [3:14-16]
4. [3:19-25]
5. [4:26-32]
6. [0:0]
Local defs:
0. TypeAlias(
    DefTypeAlias {
        id_loc: 1,
        custom_error_loc_opt: None,
        name: "T1",
        tparams: Mono,
        body: Annot(
            Number(
                2,
            ),
        ),
        strictness_kind: Flow,
    },
)
1. TypeAlias(
    DefTypeAlias {
        id_loc: 3,
        custom_error_loc_opt: None,
        name: "T2",
        tparams: Mono,
        body: Annot(
            String(
                4,
            ),
        ),
        strictness_kind: Flow,
    },
)
2. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 6,
        name: "globalThis",
        values: {
            "globalThis": (
                6,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 6,
                            index: 2,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin module foo:
Loc: 0
CJSModule {
    type_exports: [
        ExportTypeBinding(
            0,
        ),
        ExportTypeBinding(
            1,
        ),
    ],
    exports: Some(
        Annot(
            String(
                5,
            ),
        ),
    ),
    info: CJSModuleInfo {
        type_export_keys: [
            "T1",
            "T2",
        ],
        type_stars: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: Flow,
    },
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_cjs_module_unused_type_exported() {
    let input = r#"
        declare module foo {
          declare export type T = number;
          declare module.exports: string;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:15-18]
1. [2:22-23]
2. [2:26-32]
3. [3:26-32]
4. [0:0]
Local defs:
0. TypeAlias(
    DefTypeAlias {
        id_loc: 1,
        custom_error_loc_opt: None,
        name: "T",
        tparams: Mono,
        body: Annot(
            Number(
                2,
            ),
        ),
        strictness_kind: Flow,
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 4,
        name: "globalThis",
        values: {
            "globalThis": (
                4,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 4,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin module foo:
Loc: 0
CJSModule {
    type_exports: [
        ExportTypeBinding(
            0,
        ),
    ],
    exports: Some(
        Annot(
            String(
                3,
            ),
        ),
    ),
    info: CJSModuleInfo {
        type_export_keys: [
            "T",
        ],
        type_stars: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: Flow,
    },
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_cjs_module_used_type() {
    let input = r#"
        declare module foo {
          declare type T = number;
          declare module.exports: T;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:15-18]
1. [2:15-16]
2. [2:19-25]
3. [3:26-27]
4. [0:0]
Local defs:
0. TypeAlias(
    DefTypeAlias {
        id_loc: 1,
        custom_error_loc_opt: None,
        name: "T",
        tparams: Mono,
        body: Annot(
            Number(
                2,
            ),
        ),
        strictness_kind: Flow,
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 4,
        name: "globalThis",
        values: {
            "globalThis": (
                4,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 4,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin module foo:
Loc: 0
CJSModule {
    type_exports: [
        ExportTypeBinding(
            0,
        ),
    ],
    exports: Some(
        TyRef(
            Unqualified(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 3,
                        index: 0,
                    },
                ),
            ),
        ),
    ),
    info: CJSModuleInfo {
        type_export_keys: [
            "T",
        ],
        type_stars: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: Flow,
    },
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_cjs_module_used_type_exported() {
    let input = r#"
        declare module foo {
          declare export type T = number;
          declare module.exports: T;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:15-18]
1. [2:22-23]
2. [2:26-32]
3. [3:26-27]
4. [0:0]
Local defs:
0. TypeAlias(
    DefTypeAlias {
        id_loc: 1,
        custom_error_loc_opt: None,
        name: "T",
        tparams: Mono,
        body: Annot(
            Number(
                2,
            ),
        ),
        strictness_kind: Flow,
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 4,
        name: "globalThis",
        values: {
            "globalThis": (
                4,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 4,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin module foo:
Loc: 0
CJSModule {
    type_exports: [
        ExportTypeBinding(
            0,
        ),
    ],
    exports: Some(
        TyRef(
            Unqualified(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 3,
                        index: 0,
                    },
                ),
            ),
        ),
    ),
    info: CJSModuleInfo {
        type_export_keys: [
            "T",
        ],
        type_stars: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: Flow,
    },
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_cjs_module_with_implicit_exports() {
    let input = r#"
        declare module foo {
          declare var x: string;
          declare const y: string;
          declare let z: string;
          const zRef = z;
          function empty() {}
          declare function f(): void;
          declare class Y {}
          declare component foo()
          declare enum A { B }
          declare type T = number;
          declare export type U = string;
          export const ignored = 3; // unsupported;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:15-18]
1. [2:14-15]
2. [2:17-23]
3. [3:16-17]
4. [3:19-25]
5. [4:14-15]
6. [4:17-23]
7. [7:19-20]
8. [7:20-28]
9. [7:24-28]
10. [8:16-17]
11. [9:2-25]
12. [9:20-23]
13. [9:23-25]
14. [9:25]
15. [10:15-16]
16. [10:19-20]
17. [11:15-16]
18. [11:19-25]
19. [12:22-23]
20. [12:26-32]
21. [0:0]
Local defs:
0. Variable(
    DefVariable {
        id_loc: 1,
        name: "x",
        def: Annot(
            String(
                2,
            ),
        ),
    },
)
1. Variable(
    DefVariable {
        id_loc: 3,
        name: "y",
        def: Annot(
            String(
                4,
            ),
        ),
    },
)
2. Variable(
    DefVariable {
        id_loc: 5,
        name: "z",
        def: Annot(
            String(
                6,
            ),
        ),
    },
)
3. DeclareFun(
    DefDeclareFun {
        id_loc: 7,
        name: "f",
        fn_loc: 8,
        def: FunSig {
            tparams: Mono,
            params: [],
            rest_param: None,
            this_param: None,
            return_: Annot(
                Void(
                    9,
                ),
            ),
            type_guard: None,
            effect_: ArbitraryEffect,
            strictness_kind: Flow,
        },
        statics: {},
        namespace_types: {},
        tail: [],
    },
)
4. DeclareClassBinding(
    DefDeclareClassBinding {
        id_loc: 10,
        name: "Y",
        def: DeclareClassSig {
            tparams: Mono,
            extends: ClassImplicitExtends,
            mixins: [],
            implements: [],
            static_props: {},
            own_props: {},
            proto_props: {},
            computed_own_props: [],
            computed_proto_props: [],
            computed_static_props: [],
            static_calls: [],
            calls: [],
            constructs: [],
            dict: None,
            static_dict: None,
            abstract_: false,
            abstract_props: {},
            strictness_kind: Flow,
        },
        namespace_types: {},
    },
)
5. ComponentBinding(
    DefComponentBinding {
        id_loc: 12,
        name: "foo",
        fn_loc: 11,
        def: ComponentSig {
            params_loc: 13,
            tparams: Mono,
            params: [],
            rest_param: None,
            renders: Annot(
                ComponentMissingRenders(
                    14,
                ),
            ),
            strictness_kind: Flow,
        },
    },
)
6. EnumBinding(
    DefEnumBinding {
        id_loc: 15,
        name: "A",
        rep: Some(
            StringRep {
                truthy: true,
            },
        ),
        members: {
            "B": 16,
        },
        member_values: None,
        has_unknown_members: false,
        strictness_kind: Flow,
    },
)
7. TypeAlias(
    DefTypeAlias {
        id_loc: 17,
        custom_error_loc_opt: None,
        name: "T",
        tparams: Mono,
        body: Annot(
            Number(
                18,
            ),
        ),
        strictness_kind: Flow,
    },
)
8. TypeAlias(
    DefTypeAlias {
        id_loc: 19,
        custom_error_loc_opt: None,
        name: "U",
        tparams: Mono,
        body: Annot(
            String(
                20,
            ),
        ),
        strictness_kind: Flow,
    },
)
9. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 21,
        name: "globalThis",
        values: {
            "globalThis": (
                21,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 21,
                            index: 9,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin module foo:
Loc: 0
CJSModule {
    type_exports: [
        ExportTypeBinding(
            7,
        ),
        ExportTypeBinding(
            8,
        ),
    ],
    exports: Some(
        Value(
            DeclareModuleImplicitlyExportedObject(
                ValueDeclareModuleImplicitlyExportedObject {
                    loc: 0,
                    module_name: Userland(
                        "foo",
                    ),
                    props: {
                        "A": ObjValueField(
                            (
                                0,
                                Ref(
                                    LocalRef(
                                        PackedRefLocal {
                                            ref_loc: 0,
                                            index: 6,
                                        },
                                    ),
                                ),
                                Positive,
                            ),
                        ),
                        "Y": ObjValueField(
                            (
                                0,
                                Ref(
                                    LocalRef(
                                        PackedRefLocal {
                                            ref_loc: 0,
                                            index: 4,
                                        },
                                    ),
                                ),
                                Positive,
                            ),
                        ),
                        "f": ObjValueField(
                            (
                                0,
                                Ref(
                                    LocalRef(
                                        PackedRefLocal {
                                            ref_loc: 0,
                                            index: 3,
                                        },
                                    ),
                                ),
                                Positive,
                            ),
                        ),
                        "foo": ObjValueField(
                            (
                                0,
                                Ref(
                                    LocalRef(
                                        PackedRefLocal {
                                            ref_loc: 0,
                                            index: 5,
                                        },
                                    ),
                                ),
                                Positive,
                            ),
                        ),
                        "x": ObjValueField(
                            (
                                0,
                                Ref(
                                    LocalRef(
                                        PackedRefLocal {
                                            ref_loc: 0,
                                            index: 0,
                                        },
                                    ),
                                ),
                                Positive,
                            ),
                        ),
                        "y": ObjValueField(
                            (
                                0,
                                Ref(
                                    LocalRef(
                                        PackedRefLocal {
                                            ref_loc: 0,
                                            index: 1,
                                        },
                                    ),
                                ),
                                Positive,
                            ),
                        ),
                        "z": ObjValueField(
                            (
                                0,
                                Ref(
                                    LocalRef(
                                        PackedRefLocal {
                                            ref_loc: 0,
                                            index: 2,
                                        },
                                    ),
                                ),
                                Positive,
                            ),
                        ),
                    },
                    strictness_kind: Flow,
                },
            ),
        ),
    ),
    info: CJSModuleInfo {
        type_export_keys: [
            "T",
            "U",
        ],
        type_stars: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: Flow,
    },
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_es_module_default() {
    let input = r#"
        declare module foo {
          declare export default string;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:15-18]
1. [2:17-24]
2. [2:25-31]
3. [0:0]
Local defs:
0. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 3,
        name: "globalThis",
        values: {
            "globalThis": (
                3,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 3,
                            index: 0,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin module foo:
Loc: 0
ESModule {
    type_exports: [],
    exports: [
        ExportDefault(
            ExportDefaultData {
                default_loc: 1,
                def: Annot(
                    String(
                        2,
                    ),
                ),
            },
        ),
    ],
    ts_pending: [],
    info: ESModuleInfo {
        type_export_keys: [],
        type_stars: [],
        export_keys: [
            "default",
        ],
        stars: [],
        ts_pending_keys: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: Flow,
    },
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_module_import_typeof() {
    let input = r#"
        declare module foo {
          declare export var x: string;
        }
        declare module bar {
          import typeof {x} from 'foo';
          declare export var y: x;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:15-18]
1. [2:21-22]
2. [2:24-30]
3. [4:15-18]
4. [5:17-18]
5. [6:21-22]
6. [6:24-25]
7. [0:0]
Module refs:
0. Userland("foo")
Local defs:
0. Variable(
    DefVariable {
        id_loc: 1,
        name: "x",
        def: Annot(
            String(
                2,
            ),
        ),
    },
)
1. Variable(
    DefVariable {
        id_loc: 5,
        name: "y",
        def: TyRef(
            Unqualified(
                RemoteRef(
                    PackedRefRemote {
                        ref_loc: 6,
                        index: 0,
                    },
                ),
            ),
        ),
    },
)
2. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 7,
        name: "globalThis",
        values: {
            "globalThis": (
                7,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 7,
                            index: 2,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)
Remote refs:
0. ImportTypeof { id_loc: 4, name: "x", index: 0, remote: "x", strictness_kind: Flow }
Builtin global value globalThis
Builtin module bar:
Loc: 3
ESModule {
    type_exports: [],
    exports: [
        ExportBinding(
            1,
        ),
    ],
    ts_pending: [],
    info: ESModuleInfo {
        type_export_keys: [],
        type_stars: [],
        export_keys: [
            "y",
        ],
        stars: [],
        ts_pending_keys: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: Flow,
    },
}
Builtin module foo:
Loc: 0
ESModule {
    type_exports: [],
    exports: [
        ExportBinding(
            0,
        ),
    ],
    ts_pending: [],
    info: ESModuleInfo {
        type_export_keys: [],
        type_stars: [],
        export_keys: [
            "x",
        ],
        stars: [],
        ts_pending_keys: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: Flow,
    },
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_interface_merge_props() {
    let input = r#"
        interface Foo {
          a: string;
        }
        interface Foo {
          b: number;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:10-13]
1. [2:2-3]
2. [2:5-11]
3. [5:2-3]
4. [5:5-11]
5. [0:0]
Local defs:
0. Interface(
    DefInterface {
        id_loc: 0,
        name: "Foo",
        tparams: Mono,
        def: InterfaceSig {
            extends: [],
            props: {
                "a": InterfaceField(
                    (
                        Some(
                            1,
                        ),
                        Annot(
                            String(
                                2,
                            ),
                        ),
                        Neutral,
                    ),
                ),
                "b": InterfaceField(
                    (
                        Some(
                            3,
                        ),
                        Annot(
                            Number(
                                4,
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            computed_props: [],
            calls: [],
            constructs: [],
            dict: None,
            abstract_: false,
            strictness_kind: Flow,
        },
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 5,
        name: "globalThis",
        values: {
            "globalThis": (
                5,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 5,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {
            "Foo": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin global type Foo
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_interface_merge_methods_overload() {
    let input = r#"
        interface Foo {
          bar(): string;
        }
        interface Foo {
          bar(): number;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:10-13]
1. [2:2-15]
2. [2:2-5]
3. [2:9-15]
4. [5:2-15]
5. [5:2-5]
6. [5:9-15]
7. [0:0]
Local defs:
0. Interface(
    DefInterface {
        id_loc: 0,
        name: "Foo",
        tparams: Mono,
        def: InterfaceSig {
            extends: [],
            props: {
                "bar": InterfaceMethod(
                    [
                        (
                            5,
                            4,
                            FunSig {
                                tparams: Mono,
                                params: [],
                                rest_param: None,
                                this_param: None,
                                return_: Annot(
                                    Number(
                                        6,
                                    ),
                                ),
                                type_guard: None,
                                effect_: ArbitraryEffect,
                                strictness_kind: Flow,
                            },
                        ),
                        (
                            2,
                            1,
                            FunSig {
                                tparams: Mono,
                                params: [],
                                rest_param: None,
                                this_param: None,
                                return_: Annot(
                                    String(
                                        3,
                                    ),
                                ),
                                type_guard: None,
                                effect_: ArbitraryEffect,
                                strictness_kind: Flow,
                            },
                        ),
                    ],
                ),
            },
            computed_props: [],
            calls: [],
            constructs: [],
            dict: None,
            abstract_: false,
            strictness_kind: Flow,
        },
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 7,
        name: "globalThis",
        values: {
            "globalThis": (
                7,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 7,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {
            "Foo": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin global type Foo
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_interface_merge_extends() {
    let input = r#"
        interface Base1 { x: string; }
        interface Base2 { y: number; }
        interface Foo extends Base1 {
          a: string;
        }
        interface Foo extends Base2 {
          b: number;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:10-15]
1. [1:18-19]
2. [1:21-27]
3. [2:10-15]
4. [2:18-19]
5. [2:21-27]
6. [3:10-13]
7. [3:22-27]
8. [4:2-3]
9. [4:5-11]
10. [6:22-27]
11. [7:2-3]
12. [7:5-11]
13. [0:0]
Local defs:
0. Interface(
    DefInterface {
        id_loc: 0,
        name: "Base1",
        tparams: Mono,
        def: InterfaceSig {
            extends: [],
            props: {
                "x": InterfaceField(
                    (
                        Some(
                            1,
                        ),
                        Annot(
                            String(
                                2,
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            computed_props: [],
            calls: [],
            constructs: [],
            dict: None,
            abstract_: false,
            strictness_kind: Flow,
        },
    },
)
1. Interface(
    DefInterface {
        id_loc: 3,
        name: "Base2",
        tparams: Mono,
        def: InterfaceSig {
            extends: [],
            props: {
                "y": InterfaceField(
                    (
                        Some(
                            4,
                        ),
                        Annot(
                            Number(
                                5,
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            computed_props: [],
            calls: [],
            constructs: [],
            dict: None,
            abstract_: false,
            strictness_kind: Flow,
        },
    },
)
2. Interface(
    DefInterface {
        id_loc: 6,
        name: "Foo",
        tparams: Mono,
        def: InterfaceSig {
            extends: [
                TyRef(
                    Unqualified(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 7,
                                index: 0,
                            },
                        ),
                    ),
                ),
                TyRef(
                    Unqualified(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 10,
                                index: 1,
                            },
                        ),
                    ),
                ),
            ],
            props: {
                "a": InterfaceField(
                    (
                        Some(
                            8,
                        ),
                        Annot(
                            String(
                                9,
                            ),
                        ),
                        Neutral,
                    ),
                ),
                "b": InterfaceField(
                    (
                        Some(
                            11,
                        ),
                        Annot(
                            Number(
                                12,
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            computed_props: [],
            calls: [],
            constructs: [],
            dict: None,
            abstract_: false,
            strictness_kind: Flow,
        },
    },
)
3. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 13,
        name: "globalThis",
        values: {
            "globalThis": (
                13,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 13,
                            index: 3,
                        },
                    ),
                ),
            ),
        },
        types: {
            "Base1": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
            "Base2": (
                3,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 3,
                            index: 1,
                        },
                    ),
                ),
            ),
            "Foo": (
                6,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 6,
                            index: 2,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin global type Base1
Builtin global type Base2
Builtin global type Foo
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_interface_merge_calls() {
    let input = r#"
        interface Foo {
          (): string;
        }
        interface Foo {
          (): number;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:10-13]
1. [2:2-12]
2. [2:6-12]
3. [5:2-12]
4. [5:6-12]
5. [0:0]
Local defs:
0. Interface(
    DefInterface {
        id_loc: 0,
        name: "Foo",
        tparams: Mono,
        def: InterfaceSig {
            extends: [],
            props: {},
            computed_props: [],
            calls: [
                Annot(
                    FunAnnot(
                        (
                            1,
                            FunSig {
                                tparams: Mono,
                                params: [],
                                rest_param: None,
                                this_param: None,
                                return_: Annot(
                                    String(
                                        2,
                                    ),
                                ),
                                type_guard: None,
                                effect_: ArbitraryEffect,
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                ),
                Annot(
                    FunAnnot(
                        (
                            3,
                            FunSig {
                                tparams: Mono,
                                params: [],
                                rest_param: None,
                                this_param: None,
                                return_: Annot(
                                    Number(
                                        4,
                                    ),
                                ),
                                type_guard: None,
                                effect_: ArbitraryEffect,
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                ),
            ],
            constructs: [],
            dict: None,
            abstract_: false,
            strictness_kind: Flow,
        },
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 5,
        name: "globalThis",
        values: {
            "globalThis": (
                5,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 5,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {
            "Foo": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin global type Foo
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_interface_merge_three_way() {
    let input = r#"
        interface Foo {
          a: string;
        }
        interface Foo {
          b: number;
        }
        interface Foo {
          c: boolean;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:10-13]
1. [2:2-3]
2. [2:5-11]
3. [5:2-3]
4. [5:5-11]
5. [8:2-3]
6. [8:5-12]
7. [0:0]
Local defs:
0. Interface(
    DefInterface {
        id_loc: 0,
        name: "Foo",
        tparams: Mono,
        def: InterfaceSig {
            extends: [],
            props: {
                "a": InterfaceField(
                    (
                        Some(
                            1,
                        ),
                        Annot(
                            String(
                                2,
                            ),
                        ),
                        Neutral,
                    ),
                ),
                "b": InterfaceField(
                    (
                        Some(
                            3,
                        ),
                        Annot(
                            Number(
                                4,
                            ),
                        ),
                        Neutral,
                    ),
                ),
                "c": InterfaceField(
                    (
                        Some(
                            5,
                        ),
                        Annot(
                            Boolean(
                                6,
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            computed_props: [],
            calls: [],
            constructs: [],
            dict: None,
            abstract_: false,
            strictness_kind: Flow,
        },
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 7,
        name: "globalThis",
        values: {
            "globalThis": (
                7,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 7,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {
            "Foo": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin global type Foo
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_interface_merge_prop_conflict() {
    let input = r#"
        interface Foo {
          a: string;
        }
        interface Foo {
          a: number;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:10-13]
1. [2:2-3]
2. [2:5-11]
3. [0:0]
Local defs:
0. Interface(
    DefInterface {
        id_loc: 0,
        name: "Foo",
        tparams: Mono,
        def: InterfaceSig {
            extends: [],
            props: {
                "a": InterfaceField(
                    (
                        Some(
                            1,
                        ),
                        Annot(
                            String(
                                2,
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            computed_props: [],
            calls: [],
            constructs: [],
            dict: None,
            abstract_: false,
            strictness_kind: Flow,
        },
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 3,
        name: "globalThis",
        values: {
            "globalThis": (
                3,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 3,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {
            "Foo": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin global type Foo
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_interface_merge_tparam_defaults() {
    let input = r#"
        interface Foo<T, U> {
          a: T;
        }
        interface Foo<X, Y = X> {
          b: Y;
        }
    "#;
    let output = print_builtins(vec![input]);
    let expected_output = r#"
Locs:
0. [1:10-13]
1. [1:13-19]
2. [1:14-15]
3. [1:17-18]
4. [2:2-3]
5. [2:5-6]
6. [4:21-22]
7. [5:2-3]
8. [5:5-6]
9. [0:0]
Local defs:
0. Interface(
    DefInterface {
        id_loc: 0,
        name: "Foo",
        tparams: Poly(
            (
                1,
                [
                    TParam {
                        name_loc: 2,
                        name: "T",
                        polarity: None,
                        bound: None,
                        default: None,
                        is_const: false,
                    },
                    TParam {
                        name_loc: 3,
                        name: "U",
                        polarity: None,
                        bound: None,
                        default: Some(
                            Annot(
                                Bound(
                                    AnnotBound {
                                        ref_loc: 6,
                                        name: "T",
                                    },
                                ),
                            ),
                        ),
                        is_const: false,
                    },
                ],
            ),
        ),
        def: InterfaceSig {
            extends: [],
            props: {
                "a": InterfaceField(
                    (
                        Some(
                            4,
                        ),
                        Annot(
                            Bound(
                                AnnotBound {
                                    ref_loc: 5,
                                    name: "T",
                                },
                            ),
                        ),
                        Neutral,
                    ),
                ),
                "b": InterfaceField(
                    (
                        Some(
                            7,
                        ),
                        Annot(
                            Bound(
                                AnnotBound {
                                    ref_loc: 8,
                                    name: "U",
                                },
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            computed_props: [],
            calls: [],
            constructs: [],
            dict: None,
            abstract_: false,
            strictness_kind: Flow,
        },
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 9,
        name: "globalThis",
        values: {
            "globalThis": (
                9,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 9,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {
            "Foo": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin global type Foo
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&output));
}

#[test]
fn builtin_interface_merge_repeated_tparam_defaults() {
    let input = r#"
        interface Foo<T = string> {
          a: T;
        }
        interface Foo<U = number> {
          b: U;
        }
    "#;
    let output = print_builtins(vec![input]);
    let expected_output = r#"
Locs:
0. [1:10-13]
1. [1:13-25]
2. [1:14-15]
3. [1:18-24]
4. [2:2-3]
5. [2:5-6]
6. [5:2-3]
7. [5:5-6]
8. [0:0]
Local defs:
0. Interface(
    DefInterface {
        id_loc: 0,
        name: "Foo",
        tparams: Poly(
            (
                1,
                [
                    TParam {
                        name_loc: 2,
                        name: "T",
                        polarity: None,
                        bound: None,
                        default: Some(
                            Annot(
                                String(
                                    3,
                                ),
                            ),
                        ),
                        is_const: false,
                    },
                ],
            ),
        ),
        def: InterfaceSig {
            extends: [],
            props: {
                "a": InterfaceField(
                    (
                        Some(
                            4,
                        ),
                        Annot(
                            Bound(
                                AnnotBound {
                                    ref_loc: 5,
                                    name: "T",
                                },
                            ),
                        ),
                        Neutral,
                    ),
                ),
                "b": InterfaceField(
                    (
                        Some(
                            6,
                        ),
                        Annot(
                            Bound(
                                AnnotBound {
                                    ref_loc: 7,
                                    name: "T",
                                },
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            computed_props: [],
            calls: [],
            constructs: [],
            dict: None,
            abstract_: false,
            strictness_kind: Flow,
        },
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 8,
        name: "globalThis",
        values: {
            "globalThis": (
                8,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 8,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {
            "Foo": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin global type Foo
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&output));
}

#[test]
fn builtin_declare_class_interface_merge_tparam_default() {
    let input = r#"
        declare class C<T> {
          x: T;
        }
        interface C<U = string> {
          y: U;
        }
    "#;
    let output = print_builtins(vec![input]);
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [1:15-18]
2. [1:16-17]
3. [2:2-3]
4. [2:5-6]
5. [4:16-22]
6. [5:2-3]
7. [5:5-6]
8. [0:0]
Local defs:
0. DeclareClassBinding(
    DefDeclareClassBinding {
        id_loc: 0,
        name: "C",
        def: DeclareClassSig {
            tparams: Poly(
                (
                    1,
                    [
                        TParam {
                            name_loc: 2,
                            name: "T",
                            polarity: None,
                            bound: None,
                            default: Some(
                                Annot(
                                    String(
                                        5,
                                    ),
                                ),
                            ),
                            is_const: false,
                        },
                    ],
                ),
            ),
            extends: ClassImplicitExtends,
            mixins: [],
            implements: [],
            static_props: {},
            own_props: {
                "x": InterfaceField(
                    (
                        Some(
                            3,
                        ),
                        Annot(
                            Bound(
                                AnnotBound {
                                    ref_loc: 4,
                                    name: "T",
                                },
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            proto_props: {
                "y": InterfaceField(
                    (
                        Some(
                            6,
                        ),
                        Annot(
                            Bound(
                                AnnotBound {
                                    ref_loc: 7,
                                    name: "T",
                                },
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            computed_own_props: [],
            computed_proto_props: [],
            computed_static_props: [],
            static_calls: [],
            calls: [],
            constructs: [],
            dict: None,
            static_dict: None,
            abstract_: false,
            abstract_props: {},
            strictness_kind: Flow,
        },
        namespace_types: {},
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 8,
        name: "globalThis",
        values: {
            "C": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
            "globalThis": (
                8,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 8,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)
Builtin global value C
Builtin global value globalThis
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&output));
}

#[test]
fn builtin_namespace_merge_tparam_defaults() {
    let input = r#"
        declare namespace N {
          interface Foo<T> { a: T; }
          declare class C<T> { x: T; }
        }
        declare namespace N {
          interface Foo<U = string> { b: U; }
          interface C<U = string> { y: U; }
        }
    "#;
    let output = print_builtins(vec![input]);
    let expected_output = r#"
Locs:
0. [1:18-19]
1. [2:12-15]
2. [2:15-18]
3. [2:16-17]
4. [2:21-22]
5. [2:24-25]
6. [3:16-17]
7. [3:17-20]
8. [3:18-19]
9. [3:23-24]
10. [3:26-27]
11. [6:20-26]
12. [6:30-31]
13. [6:33-34]
14. [7:18-24]
15. [7:28-29]
16. [7:31-32]
17. [0:0]
Local defs:
0. Interface(
    DefInterface {
        id_loc: 1,
        name: "Foo",
        tparams: Poly(
            (
                2,
                [
                    TParam {
                        name_loc: 3,
                        name: "T",
                        polarity: None,
                        bound: None,
                        default: Some(
                            Annot(
                                String(
                                    11,
                                ),
                            ),
                        ),
                        is_const: false,
                    },
                ],
            ),
        ),
        def: InterfaceSig {
            extends: [],
            props: {
                "a": InterfaceField(
                    (
                        Some(
                            4,
                        ),
                        Annot(
                            Bound(
                                AnnotBound {
                                    ref_loc: 5,
                                    name: "T",
                                },
                            ),
                        ),
                        Neutral,
                    ),
                ),
                "b": InterfaceField(
                    (
                        Some(
                            12,
                        ),
                        Annot(
                            Bound(
                                AnnotBound {
                                    ref_loc: 13,
                                    name: "T",
                                },
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            computed_props: [],
            calls: [],
            constructs: [],
            dict: None,
            abstract_: false,
            strictness_kind: Flow,
        },
    },
)
1. DeclareClassBinding(
    DefDeclareClassBinding {
        id_loc: 6,
        name: "C",
        def: DeclareClassSig {
            tparams: Poly(
                (
                    7,
                    [
                        TParam {
                            name_loc: 8,
                            name: "T",
                            polarity: None,
                            bound: None,
                            default: Some(
                                Annot(
                                    String(
                                        14,
                                    ),
                                ),
                            ),
                            is_const: false,
                        },
                    ],
                ),
            ),
            extends: ClassImplicitExtends,
            mixins: [],
            implements: [],
            static_props: {},
            own_props: {
                "x": InterfaceField(
                    (
                        Some(
                            9,
                        ),
                        Annot(
                            Bound(
                                AnnotBound {
                                    ref_loc: 10,
                                    name: "T",
                                },
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            proto_props: {
                "y": InterfaceField(
                    (
                        Some(
                            15,
                        ),
                        Annot(
                            Bound(
                                AnnotBound {
                                    ref_loc: 16,
                                    name: "T",
                                },
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            computed_own_props: [],
            computed_proto_props: [],
            computed_static_props: [],
            static_calls: [],
            calls: [],
            constructs: [],
            dict: None,
            static_dict: None,
            abstract_: false,
            abstract_props: {},
            strictness_kind: Flow,
        },
        namespace_types: {},
    },
)
2. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 0,
        name: "N",
        values: {
            "C": (
                6,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 6,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {
            "Foo": (
                1,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 1,
                            index: 0,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
3. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 17,
        name: "globalThis",
        values: {
            "N": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 2,
                        },
                    ),
                ),
            ),
            "globalThis": (
                17,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 17,
                            index: 3,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)
Builtin global value N
Builtin global value globalThis
"#;
    assert_eq!(dedent_trim(expected_output), dedent_trim(&output));
}

#[test]
fn builtin_interface_merge_tparam_mismatch() {
    let input = r#"
        interface Foo<T> {
          a: T;
        }
        interface Foo<T, U> {
          b: U;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:10-13]
1. [1:13-16]
2. [1:14-15]
3. [2:2-3]
4. [2:5-6]
5. [4:10-13]
6. [0:0]
Local defs:
0. Interface(
    DefInterface {
        id_loc: 0,
        name: "Foo",
        tparams: Poly(
            (
                1,
                [
                    TParam {
                        name_loc: 2,
                        name: "T",
                        polarity: None,
                        bound: None,
                        default: None,
                        is_const: false,
                    },
                ],
            ),
        ),
        def: InterfaceSig {
            extends: [],
            props: {
                "a": InterfaceField(
                    (
                        Some(
                            3,
                        ),
                        Annot(
                            Bound(
                                AnnotBound {
                                    ref_loc: 4,
                                    name: "T",
                                },
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            computed_props: [],
            calls: [],
            constructs: [],
            dict: None,
            abstract_: false,
            strictness_kind: Flow,
        },
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 6,
        name: "globalThis",
        values: {
            "globalThis": (
                6,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 6,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {
            "Foo": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin global type Foo
Errors:
BindingValidationError(InterfaceMergeTparamMismatch { name: "Foo", current_binding_loc: 5, existing_binding_loc: 0 })
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_interface_merge_with_type_alias() {
    let input = r#"
        type Foo = string;
        interface Foo {
          a: number;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:5-8]
1. [1:11-17]
2. [2:10-13]
3. [0:0]
Local defs:
0. TypeAlias(
    DefTypeAlias {
        id_loc: 0,
        custom_error_loc_opt: None,
        name: "Foo",
        tparams: Mono,
        body: Annot(
            String(
                1,
            ),
        ),
        strictness_kind: Flow,
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 3,
        name: "globalThis",
        values: {
            "globalThis": (
                3,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 3,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {
            "Foo": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin global type Foo
Errors:
BindingValidationError(NameOverride { name: "Foo", override_binding_loc: 0, existing_binding_loc: 2 })
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_declare_class_interface_merging_basic() {
    let input = r#"
        declare class C { x: number; }
        interface C { y: string; }
    "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [1:18-19]
2. [1:21-27]
3. [2:14-15]
4. [2:17-23]
5. [0:0]

Local defs:
0. DeclareClassBinding(
    DefDeclareClassBinding {
        id_loc: 0,
        name: "C",
        def: DeclareClassSig {
            tparams: Mono,
            extends: ClassImplicitExtends,
            mixins: [],
            implements: [],
            static_props: {},
            own_props: {
                "x": InterfaceField(
                    (
                        Some(
                            1,
                        ),
                        Annot(
                            Number(
                                2,
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            proto_props: {
                "y": InterfaceField(
                    (
                        Some(
                            3,
                        ),
                        Annot(
                            String(
                                4,
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            computed_own_props: [],
            computed_proto_props: [],
            computed_static_props: [],
            static_calls: [],
            calls: [],
            constructs: [],
            dict: None,
            static_dict: None,
            abstract_: false,
            abstract_props: {},
            strictness_kind: Flow,
        },
        namespace_types: {},
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 5,
        name: "globalThis",
        values: {
            "C": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
            "globalThis": (
                5,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 5,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)

Builtin global value C

Builtin global value globalThis
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_declare_class_interface_merging_new() {
    let input = r#"
        declare class C {}
        interface C { new(s: string): C; }
    "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [2:14-31]
2. [2:21-27]
3. [2:30-31]
4. [0:0]
Local defs:
0. DeclareClassBinding(
    DefDeclareClassBinding {
        id_loc: 0,
        name: "C",
        def: DeclareClassSig {
            tparams: Mono,
            extends: ClassImplicitExtends,
            mixins: [],
            implements: [],
            static_props: {},
            own_props: {},
            proto_props: {},
            computed_own_props: [],
            computed_proto_props: [],
            computed_static_props: [],
            static_calls: [],
            calls: [],
            constructs: [
                Annot(
                    FunAnnot(
                        (
                            1,
                            FunSig {
                                tparams: Mono,
                                params: [
                                    FunParam {
                                        name: Some(
                                            "s",
                                        ),
                                        t: Annot(
                                            String(
                                                2,
                                            ),
                                        ),
                                    },
                                ],
                                rest_param: None,
                                this_param: None,
                                return_: TyRef(
                                    Unqualified(
                                        LocalRef(
                                            PackedRefLocal {
                                                ref_loc: 3,
                                                index: 0,
                                            },
                                        ),
                                    ),
                                ),
                                type_guard: None,
                                effect_: ArbitraryEffect,
                                strictness_kind: Flow,
                            },
                        ),
                    ),
                ),
            ],
            dict: None,
            static_dict: None,
            abstract_: false,
            abstract_props: {},
            strictness_kind: Flow,
        },
        namespace_types: {},
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 4,
        name: "globalThis",
        values: {
            "C": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
            "globalThis": (
                4,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 4,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)
Builtin global value C
Builtin global value globalThis
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_declare_class_interface_merging_reverse() {
    let input = r#"
        interface C { y: string; }
        declare class C { x: number; }
    "#;
    let expected_output = r#"
Locs:
0. [1:14-15]
1. [1:17-23]
2. [2:14-15]
3. [2:18-19]
4. [2:21-27]
5. [0:0]

Local defs:
0. DeclareClassBinding(
    DefDeclareClassBinding {
        id_loc: 2,
        name: "C",
        def: DeclareClassSig {
            tparams: Mono,
            extends: ClassImplicitExtends,
            mixins: [],
            implements: [],
            static_props: {},
            own_props: {
                "x": InterfaceField(
                    (
                        Some(
                            3,
                        ),
                        Annot(
                            Number(
                                4,
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            proto_props: {
                "y": InterfaceField(
                    (
                        Some(
                            0,
                        ),
                        Annot(
                            String(
                                1,
                            ),
                        ),
                        Neutral,
                    ),
                ),
            },
            computed_own_props: [],
            computed_proto_props: [],
            computed_static_props: [],
            static_calls: [],
            calls: [],
            constructs: [],
            dict: None,
            static_dict: None,
            abstract_: false,
            abstract_props: {},
            strictness_kind: Flow,
        },
        namespace_types: {},
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 5,
        name: "globalThis",
        values: {
            "C": (
                2,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 2,
                            index: 0,
                        },
                    ),
                ),
            ),
            "globalThis": (
                5,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 5,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)

Builtin global value C

Builtin global value globalThis

Builtin global type C
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_toplevel_import_equals() {
    // TypeScript `import X = require(...)` at the top level of a libdef must be
    // ignored (not bound into Global), otherwise pack_builtin crashes with
    // `unexpected remote builtin`. Same shape as `builtin_toplevel_import` but
    // exercises the ImportEqualsDeclaration parser path.
    let input = r#"
        declare module foo {
          declare export var x: string;
        }
        import F = require('foo');
        declare module bar {
          declare export var y: F;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:15-18]
1. [2:21-22]
2. [2:24-30]
3. [5:15-18]
4. [6:21-22]
5. [6:24-25]
6. [0:0]
Local defs:
0. Variable(
    DefVariable {
        id_loc: 1,
        name: "x",
        def: Annot(
            String(
                2,
            ),
        ),
    },
)
1. Variable(
    DefVariable {
        id_loc: 4,
        name: "y",
        def: TyRef(
            Unqualified(
                BuiltinRef(
                    PackedRefBuiltin {
                        ref_loc: 5,
                        type_ref: true,
                        name: "F",
                    },
                ),
            ),
        ),
    },
)
2. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 6,
        name: "globalThis",
        values: {
            "globalThis": (
                6,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 6,
                            index: 2,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin module bar:
Loc: 3
ESModule {
    type_exports: [],
    exports: [
        ExportBinding(
            1,
        ),
    ],
    ts_pending: [],
    info: ESModuleInfo {
        type_export_keys: [],
        type_stars: [],
        export_keys: [
            "y",
        ],
        stars: [],
        ts_pending_keys: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: Flow,
    },
}
Builtin module foo:
Loc: 0
ESModule {
    type_exports: [],
    exports: [
        ExportBinding(
            0,
        ),
    ],
    ts_pending: [],
    info: ESModuleInfo {
        type_export_keys: [],
        type_stars: [],
        export_keys: [
            "x",
        ],
        stars: [],
        ts_pending_keys: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: Flow,
    },
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_toplevel_import() {
    let input = r#"
        declare module foo {
          declare export var x: string;
        }
        import typeof {x} from 'foo';
        declare module bar {
          declare export var y: x;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:15-18]
1. [2:21-22]
2. [2:24-30]
3. [5:15-18]
4. [6:21-22]
5. [6:24-25]
6. [0:0]
Local defs:
0. Variable(
    DefVariable {
        id_loc: 1,
        name: "x",
        def: Annot(
            String(
                2,
            ),
        ),
    },
)
1. Variable(
    DefVariable {
        id_loc: 4,
        name: "y",
        def: TyRef(
            Unqualified(
                BuiltinRef(
                    PackedRefBuiltin {
                        ref_loc: 5,
                        type_ref: true,
                        name: "x",
                    },
                ),
            ),
        ),
    },
)
2. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 6,
        name: "globalThis",
        values: {
            "globalThis": (
                6,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 6,
                            index: 2,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin module bar:
Loc: 3
ESModule {
    type_exports: [],
    exports: [
        ExportBinding(
            1,
        ),
    ],
    ts_pending: [],
    info: ESModuleInfo {
        type_export_keys: [],
        type_stars: [],
        export_keys: [
            "y",
        ],
        stars: [],
        ts_pending_keys: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: Flow,
    },
}
Builtin module foo:
Loc: 0
ESModule {
    type_exports: [],
    exports: [
        ExportBinding(
            0,
        ),
    ],
    ts_pending: [],
    info: ESModuleInfo {
        type_export_keys: [],
        type_stars: [],
        export_keys: [
            "x",
        ],
        stars: [],
        ts_pending_keys: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: Flow,
    },
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_module_export_specifiers() {
    let input = r#"
        declare module "foo" {
          declare var x : string;
          declare var y : string;
          declare export {x, y};
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:15-20]
1. [2:14-15]
2. [2:18-24]
3. [3:14-15]
4. [3:18-24]
5. [4:18-19]
6. [4:21-22]
7. [0:0]
Local defs:
0. Variable(
    DefVariable {
        id_loc: 1,
        name: "x",
        def: Annot(
            String(
                2,
            ),
        ),
    },
)
1. Variable(
    DefVariable {
        id_loc: 3,
        name: "y",
        def: Annot(
            String(
                4,
            ),
        ),
    },
)
2. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 7,
        name: "globalThis",
        values: {
            "globalThis": (
                7,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 7,
                            index: 2,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin module foo:
Loc: 0
ESModule {
    type_exports: [],
    exports: [
        ExportRef(
            LocalRef(
                PackedRefLocal {
                    ref_loc: 5,
                    index: 0,
                },
            ),
        ),
        ExportRef(
            LocalRef(
                PackedRefLocal {
                    ref_loc: 6,
                    index: 1,
                },
            ),
        ),
    ],
    ts_pending: [],
    info: ESModuleInfo {
        type_export_keys: [],
        type_stars: [],
        export_keys: [
            "x",
            "y",
        ],
        stars: [],
        ts_pending_keys: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: Flow,
    },
}
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_declare_namespace() {
    let input = r#"
        declare namespace ns {
          declare export const bar1: number;
          declare const bar2: boolean;
          declare var bar3: boolean;
          declare function f(): string;
          declare function f(): number;
          declare type Baz = string;
          export type Boz = string;
          enum B {
            C,
            D,
          }
          if (true) {} // unsupported
          export const foo = ''; // unsupported
          export default foo; // unsupported
          declare module.exports: {foo: string}; // unsupported
          import React from 'react'; // unsupported
        }
        declare global {
          declare const fromGlobal: number;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:18-20]
1. [2:23-27]
2. [2:29-35]
3. [3:16-20]
4. [3:22-29]
5. [4:14-18]
6. [4:20-27]
7. [5:19-20]
8. [5:20-30]
9. [5:24-30]
10. [6:19-20]
11. [6:20-30]
12. [6:24-30]
13. [7:15-18]
14. [7:21-27]
15. [8:14-17]
16. [8:20-26]
17. [9:7-8]
18. [10:4-5]
19. [11:4-5]
20. [0:0]
Local defs:
0. Variable(
    DefVariable {
        id_loc: 1,
        name: "bar1",
        def: Annot(
            Number(
                2,
            ),
        ),
    },
)
1. Variable(
    DefVariable {
        id_loc: 3,
        name: "bar2",
        def: Annot(
            Boolean(
                4,
            ),
        ),
    },
)
2. Variable(
    DefVariable {
        id_loc: 5,
        name: "bar3",
        def: Annot(
            Boolean(
                6,
            ),
        ),
    },
)
3. DeclareFun(
    DefDeclareFun {
        id_loc: 7,
        name: "f",
        fn_loc: 8,
        def: FunSig {
            tparams: Mono,
            params: [],
            rest_param: None,
            this_param: None,
            return_: Annot(
                String(
                    9,
                ),
            ),
            type_guard: None,
            effect_: ArbitraryEffect,
            strictness_kind: Flow,
        },
        statics: {},
        namespace_types: {},
        tail: [
            (
                10,
                11,
                FunSig {
                    tparams: Mono,
                    params: [],
                    rest_param: None,
                    this_param: None,
                    return_: Annot(
                        Number(
                            12,
                        ),
                    ),
                    type_guard: None,
                    effect_: ArbitraryEffect,
                    strictness_kind: Flow,
                },
            ),
        ],
    },
)
4. TypeAlias(
    DefTypeAlias {
        id_loc: 13,
        custom_error_loc_opt: None,
        name: "Baz",
        tparams: Mono,
        body: Annot(
            String(
                14,
            ),
        ),
        strictness_kind: Flow,
    },
)
5. TypeAlias(
    DefTypeAlias {
        id_loc: 15,
        custom_error_loc_opt: None,
        name: "Boz",
        tparams: Mono,
        body: Annot(
            String(
                16,
            ),
        ),
        strictness_kind: Flow,
    },
)
6. EnumBinding(
    DefEnumBinding {
        id_loc: 17,
        name: "B",
        rep: Some(
            StringRep {
                truthy: true,
            },
        ),
        members: {
            "C": 18,
            "D": 19,
        },
        member_values: None,
        has_unknown_members: false,
        strictness_kind: Flow,
    },
)
7. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 0,
        name: "ns",
        values: {
            "B": (
                17,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 17,
                            index: 6,
                        },
                    ),
                ),
            ),
            "bar1": (
                1,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 1,
                            index: 0,
                        },
                    ),
                ),
            ),
            "bar2": (
                3,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 3,
                            index: 1,
                        },
                    ),
                ),
            ),
            "bar3": (
                5,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 5,
                            index: 2,
                        },
                    ),
                ),
            ),
            "f": (
                7,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 7,
                            index: 3,
                        },
                    ),
                ),
            ),
        },
        types: {
            "Baz": (
                13,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 13,
                            index: 4,
                        },
                    ),
                ),
            ),
            "Boz": (
                15,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 15,
                            index: 5,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
8. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 20,
        name: "globalThis",
        values: {
            "globalThis": (
                20,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 20,
                            index: 8,
                        },
                    ),
                ),
            ),
            "ns": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 7,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin global value ns
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_declare_global_scoping() {
    let builtins = r#"
        type ExistingGlobal = string;
        declare const existingGlobalValue: number;
    "#;
    let augmentation = r#"
        export type ModuleType = number;
        export declare const moduleValue: number;
        declare global {
          type GlobalReference = ExistingGlobal;
          declare const globalValueReference: typeof existingGlobalValue;
          type ModuleReference = ModuleType;
          declare const moduleValueReference: typeof moduleValue;
          type LocalType = boolean;
          type LocalReference = LocalType;
          declare const localValue: string;
          declare const localValueReference: typeof localValue;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:5-19]
1. [1:22-28]
2. [2:14-33]
3. [2:35-41]
4. [1:12-22]
5. [1:25-31]
6. [2:21-32]
7. [2:34-40]
8. [4:7-22]
9. [4:25-39]
10. [5:16-36]
11. [5:38-64]
12. [5:45-64]
13. [6:7-22]
14. [6:25-35]
15. [7:16-36]
16. [7:38-56]
17. [7:45-56]
18. [8:7-16]
19. [8:19-26]
20. [9:7-21]
21. [9:24-33]
22. [10:16-26]
23. [10:28-34]
24. [11:16-35]
25. [11:37-54]
26. [11:44-54]
27. [0:0]
Local defs:
0. TypeAlias(
    DefTypeAlias {
        id_loc: 0,
        custom_error_loc_opt: None,
        name: "ExistingGlobal",
        tparams: Mono,
        body: Annot(
            String(
                1,
            ),
        ),
        strictness_kind: Flow,
    },
)
1. Variable(
    DefVariable {
        id_loc: 2,
        name: "existingGlobalValue",
        def: Annot(
            Number(
                3,
            ),
        ),
    },
)
2. TypeAlias(
    DefTypeAlias {
        id_loc: 4,
        custom_error_loc_opt: None,
        name: "ModuleType",
        tparams: Mono,
        body: Annot(
            Number(
                5,
            ),
        ),
        strictness_kind: TypeScriptLoose,
    },
)
3. Variable(
    DefVariable {
        id_loc: 6,
        name: "moduleValue",
        def: Annot(
            Number(
                7,
            ),
        ),
    },
)
4. TypeAlias(
    DefTypeAlias {
        id_loc: 8,
        custom_error_loc_opt: None,
        name: "GlobalReference",
        tparams: Mono,
        body: TyRef(
            Unqualified(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 9,
                        index: 0,
                    },
                ),
            ),
        ),
        strictness_kind: TypeScriptLoose,
    },
)
5. Variable(
    DefVariable {
        id_loc: 10,
        name: "globalValueReference",
        def: Annot(
            Typeof(
                AnnotTypeof {
                    loc: 11,
                    qname: [
                        "existingGlobalValue",
                    ],
                    t: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 12,
                                index: 1,
                            },
                        ),
                    ),
                    targs: None,
                },
            ),
        ),
    },
)
6. TypeAlias(
    DefTypeAlias {
        id_loc: 13,
        custom_error_loc_opt: None,
        name: "ModuleReference",
        tparams: Mono,
        body: TyRef(
            Unqualified(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 14,
                        index: 2,
                    },
                ),
            ),
        ),
        strictness_kind: TypeScriptLoose,
    },
)
7. Variable(
    DefVariable {
        id_loc: 15,
        name: "moduleValueReference",
        def: Annot(
            Typeof(
                AnnotTypeof {
                    loc: 16,
                    qname: [
                        "moduleValue",
                    ],
                    t: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 17,
                                index: 3,
                            },
                        ),
                    ),
                    targs: None,
                },
            ),
        ),
    },
)
8. TypeAlias(
    DefTypeAlias {
        id_loc: 18,
        custom_error_loc_opt: None,
        name: "LocalType",
        tparams: Mono,
        body: Annot(
            Boolean(
                19,
            ),
        ),
        strictness_kind: TypeScriptLoose,
    },
)
9. TypeAlias(
    DefTypeAlias {
        id_loc: 20,
        custom_error_loc_opt: None,
        name: "LocalReference",
        tparams: Mono,
        body: TyRef(
            Unqualified(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 21,
                        index: 8,
                    },
                ),
            ),
        ),
        strictness_kind: TypeScriptLoose,
    },
)
10. Variable(
    DefVariable {
        id_loc: 22,
        name: "localValue",
        def: Annot(
            String(
                23,
            ),
        ),
    },
)
11. Variable(
    DefVariable {
        id_loc: 24,
        name: "localValueReference",
        def: Annot(
            Typeof(
                AnnotTypeof {
                    loc: 25,
                    qname: [
                        "localValue",
                    ],
                    t: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 26,
                                index: 10,
                            },
                        ),
                    ),
                    targs: None,
                },
            ),
        ),
    },
)
12. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 27,
        name: "globalThis",
        values: {
            "existingGlobalValue": (
                2,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 2,
                            index: 1,
                        },
                    ),
                ),
            ),
            "globalThis": (
                27,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 27,
                            index: 12,
                        },
                    ),
                ),
            ),
            "globalValueReference": (
                10,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 10,
                            index: 5,
                        },
                    ),
                ),
            ),
            "localValue": (
                22,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 22,
                            index: 10,
                        },
                    ),
                ),
            ),
            "localValueReference": (
                24,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 24,
                            index: 11,
                        },
                    ),
                ),
            ),
            "moduleValueReference": (
                15,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 15,
                            index: 7,
                        },
                    ),
                ),
            ),
        },
        types: {
            "ExistingGlobal": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
            "GlobalReference": (
                8,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 8,
                            index: 4,
                        },
                    ),
                ),
            ),
            "LocalReference": (
                20,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 20,
                            index: 9,
                        },
                    ),
                ),
            ),
            "LocalType": (
                18,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 18,
                            index: 8,
                        },
                    ),
                ),
            ),
            "ModuleReference": (
                13,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 13,
                            index: 6,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: TypeScriptLoose,
    },
)
Builtin global value existingGlobalValue
Builtin global value globalThis
Builtin global value globalValueReference
Builtin global value localValue
Builtin global value localValueReference
Builtin global value moduleValueReference
Builtin global type ExistingGlobal
Builtin global type GlobalReference
Builtin global type LocalReference
Builtin global type LocalType
Builtin global type ModuleReference
    "#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins_with_global_augmentations(
            vec![builtins],
            vec![augmentation],
        ))
    )
}

#[test]
fn declare_namespace_declaration_merging() {
    let input = r#"
        declare namespace ns_v {
          declare const a: string;
        }
        declare namespace ns_v {
          declare const b: string;
        }
        declare namespace ns_t {
          type T1 = string;
        }
        declare namespace ns_t {
          type T2 = string;
        }
        declare namespace ns_v_and_then_t {
          declare const a: string;
        }
        declare namespace ns_t_and_then_v {
          type T1 = string;
        }
        declare namespace ns_v_and_then_t {
          type T1 = string;
        }
        declare namespace ns_t_and_then_v {
          declare const a: string;
        }

        declare const non_ns_value: string;
        type non_ns_type = string;
        // The following namespaces won't have any effect
        declare namespace non_ns_value {
          declare const b: string;
        }
        declare namespace non_ns_value {
          type T1 = string;
        }
        declare namespace non_ns_type {
          declare const b: string;
        }
        declare namespace non_ns_type {
          type T1 = string;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:18-22]
1. [2:16-17]
2. [2:19-25]
3. [5:16-17]
4. [5:19-25]
5. [7:18-22]
6. [8:7-9]
7. [8:12-18]
8. [11:7-9]
9. [11:12-18]
10. [13:18-33]
11. [14:16-17]
12. [14:19-25]
13. [16:18-33]
14. [17:7-9]
15. [17:12-18]
16. [20:7-9]
17. [20:12-18]
18. [23:16-17]
19. [23:19-25]
20. [25:14-26]
21. [25:28-34]
22. [26:5-16]
23. [26:19-25]
24. [0:0]
Local defs:
0. Variable(
    DefVariable {
        id_loc: 1,
        name: "a",
        def: Annot(
            String(
                2,
            ),
        ),
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 0,
        name: "ns_v",
        values: {
            "a": (
                1,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 1,
                            index: 0,
                        },
                    ),
                ),
            ),
            "b": (
                3,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 3,
                            index: 2,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)
2. Variable(
    DefVariable {
        id_loc: 3,
        name: "b",
        def: Annot(
            String(
                4,
            ),
        ),
    },
)
3. TypeAlias(
    DefTypeAlias {
        id_loc: 6,
        custom_error_loc_opt: None,
        name: "T1",
        tparams: Mono,
        body: Annot(
            String(
                7,
            ),
        ),
        strictness_kind: Flow,
    },
)
4. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 5,
        name: "ns_t",
        values: {},
        types: {
            "T1": (
                6,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 6,
                            index: 3,
                        },
                    ),
                ),
            ),
            "T2": (
                8,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 8,
                            index: 5,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
5. TypeAlias(
    DefTypeAlias {
        id_loc: 8,
        custom_error_loc_opt: None,
        name: "T2",
        tparams: Mono,
        body: Annot(
            String(
                9,
            ),
        ),
        strictness_kind: Flow,
    },
)
6. Variable(
    DefVariable {
        id_loc: 11,
        name: "a",
        def: Annot(
            String(
                12,
            ),
        ),
    },
)
7. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 10,
        name: "ns_v_and_then_t",
        values: {
            "a": (
                11,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 11,
                            index: 6,
                        },
                    ),
                ),
            ),
        },
        types: {
            "T1": (
                16,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 16,
                            index: 10,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
8. TypeAlias(
    DefTypeAlias {
        id_loc: 14,
        custom_error_loc_opt: None,
        name: "T1",
        tparams: Mono,
        body: Annot(
            String(
                15,
            ),
        ),
        strictness_kind: Flow,
    },
)
9. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 13,
        name: "ns_t_and_then_v",
        values: {
            "a": (
                18,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 18,
                            index: 11,
                        },
                    ),
                ),
            ),
        },
        types: {
            "T1": (
                14,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 14,
                            index: 8,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
10. TypeAlias(
    DefTypeAlias {
        id_loc: 16,
        custom_error_loc_opt: None,
        name: "T1",
        tparams: Mono,
        body: Annot(
            String(
                17,
            ),
        ),
        strictness_kind: Flow,
    },
)
11. Variable(
    DefVariable {
        id_loc: 18,
        name: "a",
        def: Annot(
            String(
                19,
            ),
        ),
    },
)
12. Variable(
    DefVariable {
        id_loc: 20,
        name: "non_ns_value",
        def: Annot(
            String(
                21,
            ),
        ),
    },
)
13. TypeAlias(
    DefTypeAlias {
        id_loc: 22,
        custom_error_loc_opt: None,
        name: "non_ns_type",
        tparams: Mono,
        body: Annot(
            String(
                23,
            ),
        ),
        strictness_kind: Flow,
    },
)
14. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 24,
        name: "globalThis",
        values: {
            "globalThis": (
                24,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 24,
                            index: 14,
                        },
                    ),
                ),
            ),
            "non_ns_value": (
                20,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 20,
                            index: 12,
                        },
                    ),
                ),
            ),
            "ns_t_and_then_v": (
                13,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 13,
                            index: 9,
                        },
                    ),
                ),
            ),
            "ns_v": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 1,
                        },
                    ),
                ),
            ),
            "ns_v_and_then_t": (
                10,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 10,
                            index: 7,
                        },
                    ),
                ),
            ),
        },
        types: {
            "non_ns_type": (
                22,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 22,
                            index: 13,
                        },
                    ),
                ),
            ),
            "ns_t": (
                5,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 5,
                            index: 4,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin global value non_ns_value
Builtin global value ns_t_and_then_v
Builtin global value ns_v
Builtin global value ns_v_and_then_t
Builtin global type non_ns_type
Builtin global type ns_t
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn declare_namespace_declaration_merging_member_references() {
    let input = r#"
        declare namespace ns {
          type TypeAlias = LaterType;
          type ValueAlias = typeof laterValue;
        }
        declare namespace ns {
          type LaterType = string;
          declare const laterValue: number;
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:18-20]
1. [2:7-16]
2. [2:19-28]
3. [3:7-17]
4. [3:20-37]
5. [3:27-37]
6. [6:7-16]
7. [6:19-25]
8. [7:16-26]
9. [7:28-34]
10. [0:0]
Local defs:
0. TypeAlias(
    DefTypeAlias {
        id_loc: 1,
        custom_error_loc_opt: None,
        name: "TypeAlias",
        tparams: Mono,
        body: TyRef(
            Unqualified(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 2,
                        index: 3,
                    },
                ),
            ),
        ),
        strictness_kind: Flow,
    },
)
1. TypeAlias(
    DefTypeAlias {
        id_loc: 3,
        custom_error_loc_opt: None,
        name: "ValueAlias",
        tparams: Mono,
        body: Annot(
            Typeof(
                AnnotTypeof {
                    loc: 4,
                    qname: [
                        "laterValue",
                    ],
                    t: Ref(
                        LocalRef(
                            PackedRefLocal {
                                ref_loc: 5,
                                index: 4,
                            },
                        ),
                    ),
                    targs: None,
                },
            ),
        ),
        strictness_kind: Flow,
    },
)
2. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 0,
        name: "ns",
        values: {
            "laterValue": (
                8,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 8,
                            index: 4,
                        },
                    ),
                ),
            ),
        },
        types: {
            "LaterType": (
                6,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 6,
                            index: 3,
                        },
                    ),
                ),
            ),
            "TypeAlias": (
                1,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 1,
                            index: 0,
                        },
                    ),
                ),
            ),
            "ValueAlias": (
                3,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 3,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
3. TypeAlias(
    DefTypeAlias {
        id_loc: 6,
        custom_error_loc_opt: None,
        name: "LaterType",
        tparams: Mono,
        body: Annot(
            String(
                7,
            ),
        ),
        strictness_kind: Flow,
    },
)
4. Variable(
    DefVariable {
        id_loc: 8,
        name: "laterValue",
        def: Annot(
            Number(
                9,
            ),
        ),
    },
)
5. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 10,
        name: "globalThis",
        values: {
            "globalThis": (
                10,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 10,
                            index: 5,
                        },
                    ),
                ),
            ),
            "ns": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 2,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin global value ns
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn declare_namespace_nested_declaration_merging_member_references() {
    let input = r#"
        declare namespace ns {
          declare namespace nested {
            type Alias = Later;
          }
        }
        declare namespace ns {
          declare namespace nested {
            type Later = symbol;
          }
        }
    "#;
    let expected_output = r#"
Locs:
0. [1:18-20]
1. [2:20-26]
2. [3:9-14]
3. [3:17-22]
4. [8:9-14]
5. [8:17-23]
6. [0:0]
Local defs:
0. TypeAlias(
    DefTypeAlias {
        id_loc: 2,
        custom_error_loc_opt: None,
        name: "Alias",
        tparams: Mono,
        body: TyRef(
            Unqualified(
                LocalRef(
                    PackedRefLocal {
                        ref_loc: 3,
                        index: 3,
                    },
                ),
            ),
        ),
        strictness_kind: Flow,
    },
)
1. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 1,
        name: "nested",
        values: {},
        types: {
            "Alias": (
                2,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 2,
                            index: 0,
                        },
                    ),
                ),
            ),
            "Later": (
                4,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 4,
                            index: 3,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
2. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 0,
        name: "ns",
        values: {},
        types: {
            "nested": (
                1,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 1,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
3. TypeAlias(
    DefTypeAlias {
        id_loc: 4,
        custom_error_loc_opt: None,
        name: "Later",
        tparams: Mono,
        body: Annot(
            Symbol(
                5,
            ),
        ),
        strictness_kind: Flow,
    },
)
4. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 6,
        name: "globalThis",
        values: {
            "globalThis": (
                6,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 6,
                            index: 4,
                        },
                    ),
                ),
            ),
        },
        types: {
            "ns": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 2,
                        },
                    ),
                ),
            ),
        },
        strictness_kind: Flow,
    },
)
Builtin global value globalThis
Builtin global type ns
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn declare_namespace_nested_declaration_merging_no_errors() {
    let first = r#"
        declare namespace ns {
          interface ValueAndType {
            a: string;
          }
          declare const ValueAndType: { prototype: ValueAndType };
          declare namespace nested {
            interface Inner {
              a: string;
            }
            declare const Inner: { prototype: Inner };
          }
        }
    "#;
    let second = r#"
        declare namespace ns {
          interface ValueAndType {
            b: number;
          }
          declare namespace nested {
            interface Inner {
              b: number;
            }
          }
        }
    "#;
    let output = print_builtins(vec![first, second]);
    assert!(
        !output.contains("\nErrors:\n"),
        "nested namespace declaration merging should not report errors:\n{}",
        output
    );
}

#[test]
fn builtin_pattern() {
    let input = r#"
        const o = { p: 0 };
        const {p} = o;
    "#;
    let expected_output = r#"
Locs:
0. [1:6-7]
1. [1:10-18]
2. [1:12-13]
3. [1:15-16]
4. [2:7-8]
5. [2:7-8]
6. [2:12-13]
7. [0:0]
Local defs:
0. Variable(
    DefVariable {
        id_loc: 0,
        name: "o",
        def: Value(
            ObjLit(
                ValueObjLit {
                    loc: 1,
                    frozen: false,
                    proto: None,
                    props: {
                        "p": ObjValueField(
                            (
                                2,
                                Value(
                                    NumberLit(
                                        (
                                            3,
                                            0.0,
                                            "0",
                                        ),
                                    ),
                                ),
                                Neutral,
                            ),
                        ),
                    },
                    strictness_kind: Flow,
                },
            ),
        ),
    },
)
1. Variable(
    DefVariable {
        id_loc: 5,
        name: "p",
        def: Pattern(
            1,
        ),
    },
)
2. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 7,
        name: "globalThis",
        values: {
            "globalThis": (
                7,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 7,
                            index: 2,
                        },
                    ),
                ),
            ),
            "o": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
            "p": (
                5,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 5,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: Flow,
    },
)
Pattern defs:
0. Ref(LocalRef(PackedRefLocal { ref_loc: 6, index: 0 }))
Patterns:
0. PDef(0)
1. PropP { id_loc: 4, name: "p", def: 0, strictness_kind: Flow }
Builtin global value globalThis
Builtin global value o
Builtin global value p
"#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins(vec![input]))
    )
}

#[test]
fn builtin_umd_global() {
    let augmentation = r#"
        export declare const x: number;
        export interface I {}
        export as namespace Umd;
    "#;
    let expected_output = r#"
Locs:
0. [1:21-22]
1. [1:24-30]
2. [2:17-18]
3. [3:20-23]
4. [0:0]
Local defs:
0. Variable(
    DefVariable {
        id_loc: 0,
        name: "x",
        def: Annot(
            Number(
                1,
            ),
        ),
    },
)
1. Interface(
    DefInterface {
        id_loc: 2,
        name: "I",
        tparams: Mono,
        def: InterfaceSig {
            extends: [],
            props: {},
            computed_props: [],
            calls: [],
            constructs: [],
            dict: None,
            abstract_: false,
            strictness_kind: TypeScriptLoose,
        },
    },
)
2. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 4,
        name: "globalThis",
        values: {
            "globalThis": (
                4,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 4,
                            index: 2,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: TypeScriptLoose,
    },
)
Builtin global value globalThis
Builtin UMD global Umd:
Loc: 3
ESModule {
    type_exports: [
        ExportTypeBinding(
            1,
        ),
    ],
    exports: [
        ExportBinding(
            0,
        ),
    ],
    ts_pending: [],
    info: ESModuleInfo {
        type_export_keys: [
            "I",
        ],
        type_stars: [],
        export_keys: [
            "x",
        ],
        stars: [],
        ts_pending_keys: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: TypeScriptLoose,
    },
}
    "#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins_with_global_augmentations(
            vec![],
            vec![augmentation],
        ))
    )
}

#[test]
fn builtin_umd_global_export_assignment() {
    let augmentation = r#"
        declare function m(): m.M;
        declare namespace m {
          interface M {}
        }
        export = m;
        export as namespace Umd;
    "#;
    let expected_output = r#"
Locs:
0. [1:17-18]
1. [1:18-25]
2. [1:22-25]
3. [1:22-23]
4. [1:24-25]
5. [3:12-13]
6. [5:9-10]
7. [6:20-23]
8. [0:0]
Local defs:
0. DeclareFun(
    DefDeclareFun {
        id_loc: 0,
        name: "m",
        fn_loc: 1,
        def: FunSig {
            tparams: Mono,
            params: [],
            rest_param: None,
            this_param: None,
            return_: TyRef(
                Qualified(
                    TyRefQualified {
                        loc: 2,
                        id_loc: 4,
                        name: "M",
                        qualification: Unqualified(
                            LocalRef(
                                PackedRefLocal {
                                    ref_loc: 3,
                                    index: 0,
                                },
                            ),
                        ),
                        strictness_kind: TypeScriptLoose,
                    },
                ),
            ),
            type_guard: None,
            effect_: ArbitraryEffect,
            strictness_kind: TypeScriptLoose,
        },
        statics: {},
        namespace_types: {
            "M": (
                5,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 5,
                            index: 1,
                        },
                    ),
                ),
            ),
        },
        tail: [],
    },
)
1. Interface(
    DefInterface {
        id_loc: 5,
        name: "M",
        tparams: Mono,
        def: InterfaceSig {
            extends: [],
            props: {},
            computed_props: [],
            calls: [],
            constructs: [],
            dict: None,
            abstract_: false,
            strictness_kind: TypeScriptLoose,
        },
    },
)
2. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 8,
        name: "globalThis",
        values: {
            "globalThis": (
                8,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 8,
                            index: 2,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: TypeScriptLoose,
    },
)
Builtin global value globalThis
Builtin UMD global Umd:
Loc: 7
CJSModule {
    type_exports: [],
    exports: Some(
        Ref(
            LocalRef(
                PackedRefLocal {
                    ref_loc: 6,
                    index: 0,
                },
            ),
        ),
    ),
    info: CJSModuleInfo {
        type_export_keys: [],
        type_stars: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: TypeScriptLoose,
    },
}
    "#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins_with_global_augmentations(
            vec![],
            vec![augmentation],
        ))
    )
}

#[test]
fn builtin_umd_global_skipped_with_imports() {
    let augmentation = r#"
        import type {T} from './other';
        export declare const x: T;
        export as namespace Umd;
    "#;
    let expected_output = r#"
Locs:
0. [0:0]
Local defs:
0. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 0,
        name: "globalThis",
        values: {
            "globalThis": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: TypeScriptLoose,
    },
)
Builtin global value globalThis
    "#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins_with_global_augmentations(
            vec![],
            vec![augmentation],
        ))
    )
}

#[test]
fn builtin_umd_global_first_in_wins() {
    let builtins = r#"
        declare var Taken: number;
    "#;
    let taken = r#"
        export declare const t: number;
        export as namespace Taken;
    "#;
    let first = r#"
        export declare const a: number;
        export as namespace Dup;
        export as namespace Dup;
    "#;
    let second = r#"
        export declare const b: number;
        export as namespace Dup;
    "#;
    let expected_output = r#"
Locs:
0. [1:12-17]
1. [1:19-25]
2. [1:21-22]
3. [1:24-30]
4. [2:20-23]
5. [0:0]
Local defs:
0. Variable(
    DefVariable {
        id_loc: 0,
        name: "Taken",
        def: Annot(
            Number(
                1,
            ),
        ),
    },
)
1. Variable(
    DefVariable {
        id_loc: 2,
        name: "a",
        def: Annot(
            Number(
                3,
            ),
        ),
    },
)
2. NamespaceBinding(
    DefNamespaceBinding {
        id_loc: 5,
        name: "globalThis",
        values: {
            "Taken": (
                0,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 0,
                            index: 0,
                        },
                    ),
                ),
            ),
            "globalThis": (
                5,
                Ref(
                    LocalRef(
                        PackedRefLocal {
                            ref_loc: 5,
                            index: 2,
                        },
                    ),
                ),
            ),
        },
        types: {},
        strictness_kind: TypeScriptLoose,
    },
)
Builtin global value Taken
Builtin global value globalThis
Builtin UMD global Dup:
Loc: 4
ESModule {
    type_exports: [],
    exports: [
        ExportBinding(
            1,
        ),
    ],
    ts_pending: [],
    info: ESModuleInfo {
        type_export_keys: [],
        type_stars: [],
        export_keys: [
            "a",
        ],
        stars: [],
        ts_pending_keys: [],
        strict: true,
        platform_availability_set: None,
        strictness_kind: TypeScriptLoose,
    },
}
    "#;
    assert_eq!(
        dedent_trim(expected_output),
        dedent_trim(&print_builtins_with_global_augmentations(
            vec![builtins],
            vec![taken, first, second],
        ))
    )
}
