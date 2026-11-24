/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dupe::Dupe;
    use flow_aloc::ALoc;
    use flow_common::reason::Name;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_parser::file_key::FileKey;
    use flow_parser::file_key::FileKeyInner;
    use flow_parser::loc::Loc;
    use flow_parser::loc::Position;
    use flow_parser::loc_sig::LocSig;

    use crate::ty::AnyKind;
    use crate::ty::BotKind;
    use crate::ty::GenKind;
    use crate::ty::NamedProp;
    use crate::ty::ObjKind;
    use crate::ty::ObjT;
    use crate::ty::Polarity;
    use crate::ty::Prop;
    use crate::ty::PropSource;
    use crate::ty::Ty;
    use crate::ty::UnsoundnessKind;
    use crate::ty_symbol::Provenance;
    use crate::ty_symbol::RemoteInfo;
    use crate::ty_symbol::Symbol;
    use crate::ty_utils::patch_up_react_symbol;
    use crate::ty_utils::patch_up_react_types;
    use crate::ty_utils::simplify_type;

    // Helper to create a field property
    fn make_field(name: &str, t: Ty<ALoc>, polarity: Polarity) -> Prop<ALoc> {
        Prop::NamedProp {
            name: Name::new(name),
            prop: NamedProp::Field {
                t: Arc::new(t),
                polarity,
                optional: false,
            },
            inherited: false,
            source: PropSource::Other,
            def_locs: Arc::from([] as [ALoc; 0]),
        }
    }

    // Helper to create an inexact object
    fn make_obj(props: Vec<Prop<ALoc>>) -> Ty<ALoc> {
        Ty::Obj(Box::new(ObjT {
            obj_kind: ObjKind::InexactObj,
            obj_def_loc: None,
            obj_props: Arc::from(props),
        }))
    }

    #[test]
    fn test_patch_up_react_symbol() {
        let position = Position { line: 1, column: 0 };
        let symbol = Symbol {
            sym_provenance: Provenance::Library(RemoteInfo { imported_as: None }),
            sym_def_loc: ALoc::of_loc(Loc {
                source: Some(FileKey::new(FileKeyInner::LibFile("react.js".to_string()))),
                start: position,
                end: position,
            }),
            sym_name: FlowSmolStr::new_inline("Portal"),
            sym_anonymous: false,
        };

        let patched = patch_up_react_symbol(&symbol).unwrap();
        assert_eq!(patched.sym_name, "React.Portal");

        let component_props_symbol = Symbol {
            sym_name: FlowSmolStr::new_inline("ComponentProps"),
            ..symbol
        };
        let patched = patch_up_react_symbol(&component_props_symbol).unwrap();
        assert_eq!(patched.sym_name, "React.ComponentProps");

        let internal_symbol = Symbol {
            sym_provenance: Provenance::Local,
            sym_def_loc: ALoc::none(),
            sym_name: FlowSmolStr::new_inline("React$RendersExactly"),
            sym_anonymous: false,
        };
        let patched = patch_up_react_symbol(&internal_symbol).unwrap();
        assert_eq!(patched.sym_name, "React.RendersExactly");

        let component_props = Arc::new(Ty::Generic(Box::new((
            component_props_symbol,
            GenKind::TypeAliasKind,
            Some(Arc::from([Arc::new(Ty::Num)])),
        ))));
        let patched = patch_up_react_types(component_props);
        assert!(matches!(
            patched.as_ref(),
            Ty::Generic(deref!((symbol, _, Some(args))))
                if symbol.sym_name == "React.ComponentProps"
                    && matches!(args.as_ref(), [type_] if matches!(type_.as_ref(), Ty::Num))
        ));
    }

    // {f: number} | {f: number}
    // ~>
    // {f: number}
    #[test]
    fn test_simplify_union_obj_neutral() {
        let prop = make_field("f", Ty::Num, Polarity::Neutral);
        let obj1 = make_obj(vec![prop.clone()]);
        let obj2 = make_obj(vec![prop]);

        let input = Ty::Union(
            false,
            Arc::new(obj1.clone()),
            Arc::new(obj2),
            Arc::from([] as [Arc<Ty<ALoc>>; 0]),
        );

        let output = simplify_type(true, Some(false), Arc::new(input));
        assert_eq!(*output, obj1);
    }

    // {+f: number} | {-f: number}
    // ~>
    // {+f: number} | {-f: number}
    #[test]
    fn test_simplify_union_obj_different_polarity() {
        let prop1 = make_field("f", Ty::Num, Polarity::Positive);
        let prop2 = make_field("f", Ty::Num, Polarity::Negative);
        let obj1 = make_obj(vec![prop1]);
        let obj2 = make_obj(vec![prop2]);

        let input = Ty::Union(
            false,
            Arc::new(obj1.clone()),
            Arc::new(obj2.clone()),
            Arc::from([] as [Arc<Ty<ALoc>>; 0]),
        );

        let output = simplify_type(true, Some(false), Arc::new(input.clone()));
        assert_eq!(*output, input);
    }

    // When merge_kinds is true, all kinds of `empty` are equivalent, even when
    // nested under a type constructor.
    //
    // {f: empty} | {f: empty'}
    // ~> (merge_kinds:true)
    // {f: empty'}
    #[test]
    fn test_simplify_union_obj_empty_insensitive() {
        let prop1 = make_field("f", Ty::Bot(BotKind::EmptyType), Polarity::Neutral);
        let prop2 = make_field("f", Ty::Bot(BotKind::EmptyType), Polarity::Neutral);
        let obj1 = make_obj(vec![prop1.clone()]);
        let obj2 = make_obj(vec![prop2]);

        let input = Ty::Union(
            false,
            Arc::new(obj1.clone()),
            Arc::new(obj2),
            Arc::from([] as [Arc<Ty<ALoc>>; 0]),
        );

        let output = simplify_type(true, Some(false), Arc::new(input));
        assert_eq!(*output, obj1);
    }

    // When merge_kinds is false, we preserve the different kinds of any.
    //
    // any | (any' & (any & any'))
    // ~>
    // any | (any' & (any & any'))
    #[test]
    fn test_merge_any_kinds_sensitive() {
        let any1 = Ty::Any(AnyKind::Unsound(UnsoundnessKind::BoundFunctionThis));
        let any2 = Ty::Any(AnyKind::Annotated(ALoc::none()));

        let inner_union = Ty::Union(
            false,
            Arc::new(any1.clone()),
            Arc::new(any2.clone()),
            Arc::from([] as [Arc<Ty<ALoc>>; 0]),
        );

        let intersection = Ty::Inter(
            Arc::new(any2.clone()),
            Arc::new(inner_union),
            Arc::from([] as [Arc<Ty<ALoc>>; 0]),
        );

        let input = Ty::Union(
            false,
            Arc::new(any1.clone()),
            Arc::new(intersection.clone()),
            Arc::from([] as [Arc<Ty<ALoc>>; 0]),
        );

        let output = simplify_type(false, Some(false), Arc::new(input.clone()));
        assert_eq!(*output, input);
    }

    // When merge_kinds is true, all kinds of any are considered equal and so
    // are merged when appearing in unions or intersections.
    //
    // any | (any' & (any & any'))
    // ~>
    // any
    //
    // The output could also be any'. The kind of the resulting any type when
    // merge_kinds is true, is not specified.
    #[test]
    fn test_merge_any_kinds_insensitive() {
        let any1 = Ty::Any(AnyKind::Unsound(UnsoundnessKind::BoundFunctionThis));
        let any2 = Ty::Any(AnyKind::Annotated(ALoc::none()));

        let inner_union = Ty::Union(
            false,
            Arc::new(any1.clone()),
            Arc::new(any2.clone()),
            Arc::from([] as [Arc<Ty<ALoc>>; 0]),
        );

        let intersection = Ty::Inter(
            Arc::new(any2.clone()),
            Arc::new(inner_union),
            Arc::from([] as [Arc<Ty<ALoc>>; 0]),
        );

        let input = Ty::Union(
            false,
            Arc::new(any1.clone()),
            Arc::new(intersection),
            Arc::from([] as [Arc<Ty<ALoc>>; 0]),
        );

        let output = simplify_type(true, Some(false), Arc::new(input));
        assert_eq!(*output, any2);
    }

    #[test]
    fn test_idempotence() {
        let t0 = Ty::Union(
            false,
            Arc::new(Ty::Any(AnyKind::Annotated(ALoc::none()))),
            Arc::new(Ty::Num),
            Arc::from(vec![Arc::new(Ty::NumLit("42".into()))]),
        );

        let simplified_once = simplify_type(false, Some(false), Arc::new(t0.clone()));
        let simplified_twice = simplify_type(false, Some(false), simplified_once.dupe());
        assert_eq!(simplified_once, simplified_twice);

        let sorted_once = simplify_type(false, Some(true), Arc::new(t0.clone()));
        let sorted_twice = simplify_type(false, Some(true), sorted_once.dupe());
        assert_eq!(sorted_once, sorted_twice);
    }

    #[test]
    fn test_sorting() {
        let t0 = Ty::Union(
            false,
            Arc::new(Ty::Any(AnyKind::Annotated(ALoc::none()))),
            Arc::new(Ty::Num),
            Arc::from(vec![Arc::new(Ty::NumLit("42".into()))]),
        );

        let t1 = Ty::Union(
            false,
            Arc::new(Ty::NumLit("1".into())),
            Arc::new(Ty::NumLit("2".into())),
            Arc::from(vec![Arc::new(Ty::NumLit("42".into()))]),
        );

        let t2 = Ty::Union(
            false,
            Arc::new(Ty::NumLit("2".into())),
            Arc::new(t0),
            Arc::from(vec![Arc::new(t1)]),
        );

        let sorted = simplify_type(false, Some(true), Arc::new(t2));

        if let Ty::Union(_, t0, t1, _rest) = sorted.as_ref() {
            assert!(matches!(t0.as_ref(), Ty::Any(_)));
            assert!(matches!(t1.as_ref(), Ty::NumLit(_)));
        } else {
            panic!("Expected Union type after sorting, got: {:?}", sorted);
        }
    }

    #[test]
    fn test_union_intersection_simplification() {
        let void_ty = Arc::new(Ty::Void);
        let any_ty = Arc::new(Ty::Any(AnyKind::Annotated(ALoc::none())));
        let num_lit = Arc::new(Ty::NumLit("1".into()));

        let inter1 = Ty::Inter(
            void_ty.dupe(),
            any_ty.dupe(),
            Arc::from(vec![num_lit.dupe()]),
        );

        let union1 = Ty::Union(
            false,
            void_ty.dupe(),
            Arc::new(inter1.clone()),
            Arc::from([] as [Arc<Ty<ALoc>>; 0]),
        );

        let inter2 = Ty::Inter(
            any_ty.dupe(),
            void_ty.dupe(),
            Arc::from(vec![num_lit.dupe()]),
        );

        let union2 = Ty::Union(
            false,
            Arc::new(inter2),
            void_ty.dupe(),
            Arc::from([] as [Arc<Ty<ALoc>>; 0]),
        );

        let input = Ty::Inter(
            Arc::new(union1),
            Arc::new(union2),
            Arc::from([] as [Arc<Ty<ALoc>>; 0]),
        );

        let output = simplify_type(false, Some(true), Arc::new(input));

        if let Ty::Union(_, t0, t1, _) = output.as_ref() {
            assert!(matches!(t0.as_ref(), Ty::Void));
            assert!(matches!(t1.as_ref(), Ty::Inter(..)));
        } else {
            panic!(
                "Expected Union type after simplification, got: {:?}",
                output
            );
        }
    }
}
