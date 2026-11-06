/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use flow_aloc::ALoc;
    use flow_common::reason::Name;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_parser::loc::Loc;
    use flow_parser::loc_sig::LocSig;

    use crate::ty::Binder;
    use crate::ty::BinderKind;
    use crate::ty::Decl;
    use crate::ty::DeclTypeAliasDeclData;
    use crate::ty::Dict;
    use crate::ty::Elt;
    use crate::ty::FunEffect;
    use crate::ty::FunParam;
    use crate::ty::FunT;
    use crate::ty::GenKind;
    use crate::ty::NamedProp;
    use crate::ty::ObjKind;
    use crate::ty::ObjT;
    use crate::ty::Polarity;
    use crate::ty::Prop;
    use crate::ty::PropSource;
    use crate::ty::ReturnT;
    use crate::ty::TupleElement;
    use crate::ty::Ty;
    use crate::ty::TypeParam;
    use crate::ty_printer::PrinterOptions;
    use crate::ty_printer::TypeAtPosPrint;
    use crate::ty_printer::string_of_t;
    use crate::ty_printer::string_of_type_at_pos_result;
    use crate::ty_symbol::Provenance;
    use crate::ty_symbol::Symbol;

    fn test_options() -> PrinterOptions {
        PrinterOptions {
            prefer_single_quotes: false,
            size: 80,
        }
    }

    #[test]
    fn test_object_property_get() {
        let getter = Prop::NamedProp {
            name: Name::new("foo"),
            prop: NamedProp::Get(Arc::new(Ty::Str)),
            inherited: false,
            source: PropSource::Other,
            def_locs: vec![].into(),
        };
        let obj = Ty::<ALoc>::Obj(Box::new(ObjT {
            obj_kind: ObjKind::ExactObj,
            obj_def_loc: None,
            obj_props: vec![getter].into(),
        }));
        let opts = test_options();
        let result = string_of_t(&obj, &opts);
        assert_eq!(result, "{get foo(): string}");
    }

    #[test]
    fn test_object_property_set() {
        let setter = Prop::NamedProp {
            name: Name::new("foo"),
            prop: NamedProp::Set(Arc::new(Ty::Str)),
            inherited: false,
            source: PropSource::Other,
            def_locs: vec![].into(),
        };
        let obj = Ty::<ALoc>::Obj(Box::new(ObjT {
            obj_kind: ObjKind::ExactObj,
            obj_def_loc: None,
            obj_props: vec![setter].into(),
        }));
        let opts = test_options();
        let result = string_of_t(&obj, &opts);
        assert_eq!(result, "{set foo(string): void}");
    }

    #[test]
    fn test_empty_inexact_tuple() {
        let tup = Ty::<ALoc>::Tup {
            elements: vec![].into(),
            inexact: true,
        };
        let opts = test_options();
        let result = string_of_t(&tup, &opts);
        assert_eq!(result, "[...]");
    }

    #[test]
    fn test_inexact_tuple() {
        let elements = vec![TupleElement::TupleElement {
            name: None,
            t: Arc::new(Ty::Num),
            polarity: Polarity::Neutral,
            optional: false,
        }];
        let tup = Ty::<ALoc>::Tup {
            elements: elements.into(),
            inexact: true,
        };
        let opts = test_options();
        let result = string_of_t(&tup, &opts);
        assert_eq!(result, "[number, ...]");
    }

    #[test]
    fn test_top_type() {
        let opts = test_options();
        let result = string_of_t(&Ty::<ALoc>::Top, &opts);
        assert_eq!(result, "unknown");
    }

    #[test]
    fn test_truncated_type() {
        let opts = test_options();
        let result = string_of_t(&Ty::<ALoc>::Truncated, &opts);
        assert_eq!(result, "...");
    }

    #[test]
    fn test_variance_keywords() {
        let mk_field = |name: &str, polarity: Polarity, t: Ty<ALoc>| Prop::NamedProp {
            name: Name::new(name),
            prop: NamedProp::Field {
                t: Arc::new(t),
                polarity,
                optional: false,
            },
            inherited: false,
            source: PropSource::Other,
            def_locs: vec![].into(),
        };
        let obj = Ty::<ALoc>::Obj(Box::new(ObjT {
            obj_kind: ObjKind::IndexedObj(Dict {
                dict_polarity: Polarity::Positive,
                dict_name: Some(FlowSmolStr::new("key")),
                dict_key: Arc::new(Ty::Str),
                dict_value: Arc::new(Ty::Top),
            }),
            obj_def_loc: None,
            obj_props: vec![
                mk_field("ro", Polarity::Positive, Ty::Str),
                mk_field("wo", Polarity::Negative, Ty::Num),
            ]
            .into(),
        }));
        let opts = test_options();
        let result = string_of_t(&obj, &opts);
        assert_eq!(
            result,
            "{readonly [key: string]: unknown, readonly ro: string, writeonly wo: number}"
        );
    }

    #[test]
    fn test_type_param_bound_and_variance() {
        let func = Ty::<ALoc>::Fun(Box::new(FunT {
            fun_this_param: None,
            fun_params: vec![].into(),
            fun_rest_param: None,
            fun_return: ReturnT::ReturnType(Arc::new(Ty::Void)),
            fun_type_params: Some(
                vec![TypeParam {
                    tp_name: FlowSmolStr::new("T"),
                    tp_bound: Some(Arc::new(Ty::Top)),
                    tp_polarity: Polarity::Positive,
                    tp_default: None,
                    tp_const: false,
                }]
                .into(),
            ),
            fun_static: Arc::new(Ty::Top),
            fun_effect: FunEffect::Arbitrary,
        }));
        let opts = test_options();
        let result = string_of_t(&func, &opts);
        assert_eq!(result, "<out T extends unknown>() => void");
    }
    #[test]
    fn test_function_this_param() {
        let func = Ty::<ALoc>::Fun(Box::new(FunT {
            fun_this_param: Some(Arc::new(Ty::Str)),
            fun_params: vec![].into(),
            fun_rest_param: None,
            fun_return: ReturnT::ReturnType(Arc::new(Ty::Void)),
            fun_type_params: None,
            fun_static: Arc::new(Ty::Top),
            fun_effect: FunEffect::Arbitrary,
        }));
        let opts = test_options();
        let result = string_of_t(&func, &opts);
        assert_eq!(result, "(this: string) => void");
    }

    fn void_fun() -> Ty<ALoc> {
        Ty::Fun(Box::new(FunT {
            fun_this_param: None,
            fun_params: vec![].into(),
            fun_rest_param: None,
            fun_return: ReturnT::ReturnType(Arc::new(Ty::Void)),
            fun_type_params: None,
            fun_static: Arc::new(Ty::Top),
            fun_effect: FunEffect::Arbitrary,
        }))
    }

    #[test]
    fn test_maybe_function_omits_parens() {
        let maybe_fun = Ty::<ALoc>::Union(
            false,
            Arc::new(Ty::Null),
            Arc::new(Ty::Void),
            vec![Arc::new(void_fun())].into(),
        );
        let result = string_of_t(&maybe_fun, &test_options());
        assert_eq!(result, "?() => void");
    }

    #[test]
    fn test_maybe_union_with_function_keeps_parens() {
        let maybe_union = Ty::<ALoc>::Union(
            false,
            Arc::new(Ty::Null),
            Arc::new(Ty::Void),
            vec![Arc::new(void_fun()), Arc::new(Ty::Str)].into(),
        );
        let result = string_of_t(&maybe_union, &test_options());
        assert_eq!(result, "?(() => void) | string");
    }

    #[test]
    fn test_indexed_access_parenthesizes_object() {
        let index = |object: Ty<ALoc>| Ty::IndexedAccess {
            _object: Arc::new(object),
            index: Arc::new(Ty::StrLit(FlowSmolStr::new("a"))),
            optional: false,
        };
        let opts = test_options();

        let union = Ty::<ALoc>::Union(false, Arc::new(Ty::Num), Arc::new(Ty::Str), vec![].into());
        assert_eq!(
            string_of_t(&index(union), &opts),
            "(number | string)[\"a\"]"
        );
        assert_eq!(
            string_of_t(&index(void_fun()), &opts),
            "(() => void)[\"a\"]"
        );
        assert_eq!(string_of_t(&index(Ty::Str), &opts), "string[\"a\"]");
    }

    #[test]
    fn test_hook_binder_uses_hook_keyword() {
        let hook = Elt::Type(Arc::new(Ty::<ALoc>::Fun(Box::new(FunT {
            fun_this_param: None,
            fun_params: vec![(
                Some(FlowSmolStr::new("x")),
                Arc::new(Ty::Num),
                FunParam {
                    prm_optional: false,
                },
            )]
            .into(),
            fun_rest_param: None,
            fun_return: ReturnT::ReturnType(Arc::new(Ty::Void)),
            fun_type_params: None,
            fun_static: Arc::new(Ty::Top),
            fun_effect: FunEffect::Hook,
        }))));
        let binder = Binder {
            kind: BinderKind::Function,
            name: FlowSmolStr::new("useFoo"),
            owner: None,
            type_parameter_context: None,
            overloads: 0,
            targs: None,
        };

        let (type_str, _) = string_of_type_at_pos_result::<Loc>(
            TypeAtPosPrint {
                ty: &hook,
                refs: None,
                binder: Some(&binder),
                alias: None,
            },
            &|_| Loc::none(),
            &test_options(),
        );

        assert_eq!(type_str, "hook useFoo(x: number): void");
    }

    #[test]
    fn test_truncated_object_reports_omitted_properties() {
        let field = |name: &str| Prop::NamedProp {
            name: Name::new(name),
            prop: NamedProp::Field {
                t: Arc::new(Ty::Str),
                polarity: Polarity::Neutral,
                optional: false,
            },
            inherited: false,
            source: PropSource::Other,
            def_locs: vec![].into(),
        };
        let obj = Ty::<ALoc>::Obj(Box::new(ObjT {
            obj_kind: ObjKind::ExactObj,
            obj_def_loc: None,
            obj_props: vec![field("a"), field("b"), field("c"), field("d")].into(),
        }));
        let opts = PrinterOptions {
            size: 3,
            ..test_options()
        };

        let result = string_of_t(&obj, &opts);

        assert_eq!(result, "{a: string, b: string, ... 2 more properties ...}");
    }

    #[test]
    fn test_truncated_union_reports_omitted_members() {
        let union = Ty::<ALoc>::Union(
            false,
            Arc::new(Ty::Num),
            Arc::new(Ty::Str),
            vec![Arc::new(Ty::Bool)].into(),
        );
        let opts = PrinterOptions {
            size: 3,
            ..test_options()
        };

        let result = string_of_t(&union, &opts);

        assert_eq!(result, "number | string | ... 1 more union member ...");
    }

    #[test]
    fn test_multiline_union_aligns_members() {
        let union = Ty::<ALoc>::Union(
            false,
            Arc::new(Ty::StrLit(FlowSmolStr::new(
                "FIRST_UNION_MEMBER_WITH_A_LONG_NAME",
            ))),
            Arc::new(Ty::StrLit(FlowSmolStr::new(
                "SECOND_UNION_MEMBER_WITH_A_LONG_NAME",
            ))),
            vec![Arc::new(Ty::StrLit(FlowSmolStr::new(
                "THIRD_UNION_MEMBER_WITH_A_LONG_NAME",
            )))]
            .into(),
        );
        let type_alias = Elt::Decl(Decl::TypeAliasDecl(Box::new(DeclTypeAliasDeclData {
            import: false,
            name: Symbol {
                sym_provenance: Provenance::Local,
                sym_def_loc: ALoc::none(),
                sym_name: FlowSmolStr::new("LongUnion"),
                sym_anonymous: false,
            },
            tparams: None,
            type_: Some(Arc::new(union)),
            opaque: None,
        })));

        let (type_str, _) = string_of_type_at_pos_result(
            TypeAtPosPrint {
                ty: &type_alias,
                refs: None,
                binder: None,
                alias: None,
            },
            &|_| Loc::none(),
            &test_options(),
        );

        assert_eq!(
            type_str,
            "type LongUnion =\n  | \"FIRST_UNION_MEMBER_WITH_A_LONG_NAME\"\n  | \"SECOND_UNION_MEMBER_WITH_A_LONG_NAME\"\n  | \"THIRD_UNION_MEMBER_WITH_A_LONG_NAME\""
        );
    }

    #[test]
    fn test_truncated_type_omits_hidden_refs() {
        let symbol = |name: &str| Symbol {
            sym_provenance: Provenance::Local,
            sym_def_loc: ALoc::none(),
            sym_name: FlowSmolStr::new(name),
            sym_anonymous: false,
        };
        let visible = symbol("Visible");
        let hidden = symbol("Hidden");
        let field = |name: &str, symbol: Symbol<ALoc>| Prop::NamedProp {
            name: Name::new(name),
            prop: NamedProp::Field {
                t: Arc::new(Ty::Generic(Box::new((
                    symbol,
                    GenKind::TypeAliasKind,
                    None,
                )))),
                polarity: Polarity::Neutral,
                optional: false,
            },
            inherited: false,
            source: PropSource::Other,
            def_locs: vec![].into(),
        };
        let obj = Elt::Type(Arc::new(Ty::Obj(Box::new(ObjT {
            obj_kind: ObjKind::ExactObj,
            obj_def_loc: None,
            obj_props: vec![
                field("visible", visible.clone()),
                field("hidden", hidden.clone()),
            ]
            .into(),
        }))));
        let refs: BTreeSet<_> = [visible, hidden]
            .into_iter()
            .map(|symbol| symbol.map_locs(&|_| Loc::none()))
            .collect();
        let opts = PrinterOptions {
            size: 2,
            ..test_options()
        };

        let (type_str, refs) = string_of_type_at_pos_result(
            TypeAtPosPrint {
                ty: &obj,
                refs: Some(&refs),
                binder: None,
                alias: None,
            },
            &|_| Loc::none(),
            &opts,
        );

        assert_eq!(type_str, "{visible: Visible, ... 1 more property ...}");
        assert_eq!(refs, Some(vec![("Visible".to_string(), Loc::none())]));
    }
}
