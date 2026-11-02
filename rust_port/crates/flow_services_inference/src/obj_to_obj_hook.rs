/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ops::Deref;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_parser::loc::Loc;
use flow_typing_flow_js::type_inference_hooks_js;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::properties;
use flow_typing_type::type_util;

thread_local! {
    static OBJ_TO_OBJ_MAP: RefCell<BTreeMap<ALoc, BTreeSet<properties::Id>>> = const { RefCell::new(BTreeMap::new()) };
}

// If the given type refers to an object literal, return the location of the object literal.
// Otherwise return None
fn get_object_literal_loc(ty: &Type) -> Option<ALoc> {
    match ty.deref() {
        TypeInner::DefT(_, def_t)
            if let DefTInner::ObjT(o) = def_t.deref()
                && o.literal.is_expression() =>
        {
            Some(type_util::def_loc_of_t(ty).dupe())
        }
        _ => None,
    }
}

fn obj_to_obj_hook_fn(obj1: &Type, obj2: &Type) {
    match get_object_literal_loc(obj1) {
        Some(obj1_aloc) => match (obj1.deref(), obj2.deref()) {
            (TypeInner::DefT(_, d1), TypeInner::DefT(_, d2))
                if let DefTInner::ObjT(_) = d1.deref()
                    && let DefTInner::ObjT(obj2_obj) = d2.deref() =>
            {
                let obj2_props_id = obj2_obj.props_tmap.dupe();
                OBJ_TO_OBJ_MAP.with(|m| {
                    m.borrow_mut()
                        .entry(obj1_aloc)
                        .or_default()
                        .insert(obj2_props_id);
                });
            }
            _ => {}
        },
        _ => {}
    }
}

pub fn with_obj_to_obj_hook<T>(
    enabled: bool,
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    f: impl FnOnce() -> T,
) -> (T, BTreeMap<Loc, BTreeSet<properties::Id>>) {
    OBJ_TO_OBJ_MAP.with(|m| m.borrow_mut().clear());
    if enabled {
        type_inference_hooks_js::set_obj_to_obj_hook(obj_to_obj_hook_fn);
    }
    let result = f();
    type_inference_hooks_js::reset_hooks();
    let aloc_map = OBJ_TO_OBJ_MAP.with(|m| std::mem::take(&mut *m.borrow_mut()));
    let loc_map = aloc_map.into_iter().fold(
        BTreeMap::new(),
        |mut acc: BTreeMap<Loc, BTreeSet<properties::Id>>, (aloc, ids)| {
            acc.entry(loc_of_aloc(&aloc)).or_default().extend(ids);
            acc
        },
    );
    (result, loc_map)
}
