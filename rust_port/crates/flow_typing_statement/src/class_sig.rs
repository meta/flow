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
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_common::files;
use flow_common::polarity::Polarity;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::subst_name::SubstName;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_env_builder::env_api::EnvKey;
use flow_env_builder::name_def_types::Def;
use flow_parser::loc_sig::LocSig;
use flow_typing_context::Context;
use flow_typing_errors::error_message::EAbstractClassData;
use flow_typing_errors::error_message::EOverrideData;
use flow_typing_errors::error_message::EPolarityMismatchData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::intermediate_error_types::AbstractErrorKind;
use flow_typing_errors::intermediate_error_types::NamedReferenceData;
use flow_typing_errors::intermediate_error_types::OverrideErrorKind;
use flow_typing_errors::intermediate_error_types::PropPolarityMismatchObject;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_common::obj_type;
use flow_typing_flow_js::flow_js;
use flow_typing_flow_js::flow_js::FlowJs;
use flow_typing_loc_env::func_class_sig_types;
use flow_typing_loc_env::func_class_sig_types::ConfigTypes;
use flow_typing_loc_env::func_class_sig_types::class as class_types;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::type_or_type_desc::TypeOrTypeDescT;
use flow_typing_type::type_::*;
use flow_typing_type::type_util;
use flow_typing_utils::abnormal::CheckExprError;
use flow_typing_utils::type_env;
use flow_typing_utils::typed_ast_utils;

use crate::func_sig;
use crate::type_annotation_cons_gen;

pub fn empty<C: ConfigTypes>(
    id: flow_aloc::ALocId,
    class_name: Option<FlowSmolStr>,
    class_loc: ALoc,
    reason: Reason,
    tparams: TypeParams,
    tparams_map: BTreeMap<SubstName, Type>,
    super_: class_types::Super,
) -> class_types::Class<C> {
    let empty_sig = |reason: Reason| -> class_types::Signature<C> {
        class_types::Signature {
            reason,
            fields: BTreeMap::new(),
            private_fields: BTreeMap::new(),
            proto_fields: BTreeMap::new(),
            methods: BTreeMap::new(),
            private_methods: BTreeMap::new(),
            getters: BTreeMap::new(),
            setters: BTreeMap::new(),
            calls: Vec::new(),
            constructs: Vec::new(),
            dict: None,
            abstract_members: std::collections::BTreeSet::new(),
            override_members: BTreeMap::new(),
        }
    };
    let constructor = Vec::new();
    let static_ = {
        let reason = reason.dupe().replace_desc(None);
        empty_sig(reason)
    };
    let instance = empty_sig(reason);
    class_types::Class {
        id,
        class_name,
        class_loc,
        tparams,
        tparams_map,
        super_,
        constructor,
        static_,
        instance,
        abstract_: false,
        is_declare: false,
    }
}

fn structural<C: ConfigTypes>(x: &class_types::Class<C>) -> bool {
    match &x.super_ {
        class_types::Super::Interface(_) => true,
        class_types::Super::Class(_) => false,
    }
}

fn get_inst_kind<C: ConfigTypes>(x: &class_types::Class<C>) -> InstanceKind {
    match &x.super_ {
        class_types::Super::Interface(iface) => InstanceKind::InterfaceKind {
            inline: iface.inline,
            reject_primitives: false,
        },
        class_types::Super::Class(_) => InstanceKind::ClassKind,
    }
}

fn map_sig<C: ConfigTypes>(
    static_: bool,
    f: impl FnOnce(&mut class_types::Signature<C>),
    s: &mut class_types::Class<C>,
) {
    if static_ {
        f(&mut s.static_);
    } else {
        f(&mut s.instance);
    }
}

fn with_sig<C: ConfigTypes, R>(
    static_: bool,
    f: impl FnOnce(&class_types::Signature<C>) -> R,
    s: &class_types::Class<C>,
) -> R {
    if static_ {
        f(&s.static_)
    } else {
        f(&s.instance)
    }
}

// Track [override]-marked members per-side. Unlike [abstract_members],
// this is tracked on BOTH instance and static sides — override checks
// fire independently per side (a static [override] can't satisfy an
// instance-only base member, and vice versa). The recorded [loc] is the
// member's def loc, used as the primary loc for the late-pass
// [EOverride] errors.
//
// Per-side dispatch happens at the call site via [map_sig], so [s] is
// already the per-side signature — no [static_] argument needed.
fn update_override_members<C: ConfigTypes>(
    override_: bool,
    name: &Name,
    loc: ALoc,
    s: &class_types::Signature<C>,
) -> BTreeMap<Name, ALoc> {
    if !override_ {
        s.override_members.clone()
    } else {
        let mut m = s.override_members.clone();
        m.insert(name.dupe(), loc);
        m
    }
}

// Private members (ES [#name]) live in a separate namespace from public
// members. Key [override_members] under the prefixed name (e.g. ["#foo"])
// so it can't collide with a public inherited [foo] when the not-inherited
// check looks it up. (The inherited walk reads only public
// own_props/proto_props, so a prefixed key correctly fails the membership
// check.)
fn private_member_key(name: &str) -> FlowSmolStr {
    FlowSmolStr::new(format!("#{}", name))
}

pub fn add_private_field<C: ConfigTypes>(
    override_: bool,
    name: FlowSmolStr,
    loc: ALoc,
    polarity: Polarity,
    field: class_types::Field<C>,
    static_: bool,
    x: &mut class_types::Class<C>,
) {
    map_sig(
        static_,
        |s| {
            let new_override_members = update_override_members(
                override_,
                &Name::new(private_member_key(&name)),
                loc.dupe(),
                s,
            );
            s.private_fields.insert(name, (Some(loc), polarity, field));
            s.override_members = new_override_members;
        },
        x,
    );
}

pub fn add_private_method<C: ConfigTypes>(
    static_: bool,
    override_: bool,
    name: FlowSmolStr,
    id_loc: ALoc,
    this_write_loc: Option<ALoc>,
    uses_this: bool,
    func_sig_: func_class_sig_types::func::Func<C>,
    set_asts: class_types::SetAsts<C>,
    set_type: class_types::SetType,
    x: &mut class_types::Class<C>,
) {
    let func_info = class_types::FuncInfo {
        id_loc: Some(id_loc.dupe()),
        this_write_loc,
        uses_this,
        func_sig: func_sig_,
        set_asts,
        set_type,
    };
    map_sig(
        static_,
        |s| {
            let new_override_members = update_override_members(
                override_,
                &Name::new(private_member_key(&name)),
                id_loc,
                s,
            );
            s.private_methods.insert(name, func_info);
            s.override_members = new_override_members;
        },
        x,
    );
}

pub fn public_fields_of_signature<C: ConfigTypes>(
    static_: bool,
    s: &class_types::Class<C>,
) -> &BTreeMap<Name, class_types::FieldPrime<C>> {
    if static_ {
        &s.static_.fields
    } else {
        &s.instance.fields
    }
}

pub fn private_fields_of_signature<C: ConfigTypes>(
    static_: bool,
    s: &class_types::Class<C>,
) -> &BTreeMap<FlowSmolStr, class_types::FieldPrime<C>> {
    if static_ {
        &s.static_.private_fields
    } else {
        &s.instance.private_fields
    }
}

pub fn add_constructor<C: ConfigTypes>(
    id_loc: Option<ALoc>,
    func_sig_: func_class_sig_types::func::Func<C>,
    set_asts: Option<class_types::SetAsts<C>>,
    set_type: Option<class_types::SetType>,
    s: &mut class_types::Class<C>,
) {
    let set_asts = set_asts.unwrap_or_else(|| Rc::new(|_| {}));
    let set_type = set_type.unwrap_or_else(|| Rc::new(|_| {}));
    s.constructor = vec![class_types::FuncInfo {
        id_loc,
        this_write_loc: None,
        uses_this: false,
        func_sig: func_sig::to_ctor_sig(func_sig_),
        set_asts,
        set_type,
    }];
}

pub fn add_default_constructor<C: ConfigTypes>(reason: Reason, s: &mut class_types::Class<C>) {
    let func_sig_ = func_sig::default_constructor(reason);
    add_constructor(None, func_sig_, None, None, s);
}

pub fn append_constructor<C: ConfigTypes>(
    id_loc: Option<ALoc>,
    func_sig_: func_class_sig_types::func::Func<C>,
    set_asts: Option<class_types::SetAsts<C>>,
    set_type: Option<class_types::SetType>,
    s: &mut class_types::Class<C>,
) {
    let set_asts = set_asts.unwrap_or_else(|| Rc::new(|_| {}));
    let set_type = set_type.unwrap_or_else(|| Rc::new(|_| {}));
    s.constructor.push(class_types::FuncInfo {
        id_loc,
        this_write_loc: None,
        uses_this: false,
        func_sig: func_sig::to_ctor_sig(func_sig_),
        set_asts,
        set_type,
    });
}

pub fn append_constructor_replacing_default<C: ConfigTypes>(
    id_loc: Option<ALoc>,
    func_sig_: func_class_sig_types::func::Func<C>,
    set_asts: Option<class_types::SetAsts<C>>,
    set_type: Option<class_types::SetType>,
    s: &mut class_types::Class<C>,
) {
    match s.constructor.as_slice() {
        [class_types::FuncInfo { id_loc: None, .. }] => {
            add_constructor(id_loc, func_sig_, set_asts, set_type, s)
        }
        _ => append_constructor(id_loc, func_sig_, set_asts, set_type, s),
    }
}

// Track abstractness only on the instance side — [inst_abstract_props]
// reads only [s.instance.abstract_members]; static-side writes are dead.
// Add-only: a name in [abstract_members] means "some declared shape of
// this name is abstract." Removing when [abstract_] is false would be wrong
// for accessor pairs — an abstract [get x] followed by a concrete [set x]
// shares the name [x], and a remove would silently clear the getter
// obligation. The in-class shadow case the prior implementation tried to
// handle (`abstract x; x = 0;` inside one class) is parser-blocked as a
// duplicate member.
//
// Name-only granularity matches TS: [abstract get x] + [abstract set x]
// collapse to a single obligation "x", and a subclass that implements
// either half satisfies it. (TS2515 fires only when a subclass omits
// both halves.)
fn update_abstract_members<C: ConfigTypes>(
    abstract_: bool,
    static_: bool,
    name: &Name,
    s: &class_types::Signature<C>,
) -> BTreeSet<Name> {
    if static_ || !abstract_ {
        s.abstract_members.clone()
    } else {
        let mut m = s.abstract_members.clone();
        m.insert(name.dupe());
        m
    }
}

// [override_loc]: [Some loc] means the member is [override]-marked with
// its def loc; [None] means it isn't. Folding the bool and the loc into
// a single param prevents the [override = true, override_loc = None]
// inconsistency where the override flag would be silently dropped.
fn add_field_inner<C: ConfigTypes>(
    abstract_: bool,
    static_: bool,
    override_loc: Option<ALoc>,
    name: Name,
    fld: class_types::FieldPrime<C>,
    x: &mut class_types::Class<C>,
) {
    let flat = static_ || structural(x);
    map_sig(
        static_,
        |s| {
            s.fields.insert(name.dupe(), fld);
            if flat {
                s.proto_fields.remove(&name);
                s.methods.remove(&name);
                s.getters.remove(&name);
                s.setters.remove(&name);
            }
            let new_abstract_members = update_abstract_members(abstract_, static_, &name, s);
            s.abstract_members = new_abstract_members;
            if let Some(loc) = override_loc {
                s.override_members.insert(name.dupe(), loc);
            }
        },
        x,
    );
}

pub fn add_field<C: ConfigTypes>(
    static_: bool,
    abstract_: bool,
    override_: bool,
    name: Name,
    loc: ALoc,
    polarity: Polarity,
    field: class_types::Field<C>,
    x: &mut class_types::Class<C>,
) {
    let override_loc = if override_ { Some(loc.dupe()) } else { None };
    add_field_inner(
        abstract_,
        static_,
        override_loc,
        name,
        (Some(loc), polarity, field),
        x,
    );
}

pub fn add_indexer<C: ConfigTypes>(static_: bool, dict: DictType, x: &mut class_types::Class<C>) {
    map_sig(
        static_,
        |s| {
            s.dict = Some(dict);
        },
        x,
    );
}

pub fn has_indexer<C: ConfigTypes>(static_: bool, x: &class_types::Class<C>) -> bool {
    with_sig(static_, |s| s.dict.is_some(), x)
}

pub fn add_name_field<C: ConfigTypes>(x: &mut class_types::Class<C>) {
    let r = x.instance.reason.dupe().replace_desc(None);
    let t = str_module_t::why(r);
    add_field_inner(
        false,
        true,
        None,
        Name::new(FlowSmolStr::new("name")),
        (None, Polarity::Neutral, class_types::Field::Annot(t)),
        x,
    );
}

pub fn add_proto_field<C: ConfigTypes>(
    abstract_: bool,
    override_: bool,
    name: Name,
    loc: ALoc,
    polarity: Polarity,
    field: class_types::Field<C>,
    x: &mut class_types::Class<C>,
) {
    map_sig(
        false,
        |s| {
            s.proto_fields
                .insert(name.dupe(), (Some(loc.dupe()), polarity, field));
            s.methods.remove(&name);
            s.getters.remove(&name);
            s.setters.remove(&name);
            let new_abstract_members = update_abstract_members(abstract_, false, &name, s);
            s.abstract_members = new_abstract_members;
            let new_override_members = update_override_members(override_, &name, loc, s);
            s.override_members = new_override_members;
        },
        x,
    );
}

pub fn add_method<C: ConfigTypes>(
    static_: bool,
    abstract_: bool,
    override_: bool,
    name: Name,
    id_loc: ALoc,
    this_write_loc: Option<ALoc>,
    uses_this: bool,
    func_sig_: func_class_sig_types::func::Func<C>,
    set_asts: Option<class_types::SetAsts<C>>,
    set_type: Option<class_types::SetType>,
    x: &mut class_types::Class<C>,
) {
    let flat = static_ || structural(x);
    let set_asts = set_asts.unwrap_or_else(|| Rc::new(|_| {}));
    let set_type = set_type.unwrap_or_else(|| Rc::new(|_| {}));
    let func_info = class_types::FuncInfo {
        id_loc: Some(id_loc.dupe()),
        this_write_loc,
        uses_this,
        func_sig: func_sig_,
        set_asts,
        set_type,
    };
    map_sig(
        static_,
        |s| {
            if flat {
                s.fields.remove(&name);
            }
            s.proto_fields.remove(&name);
            s.methods.insert(name.dupe(), vec1::vec1![func_info]);
            s.getters.remove(&name);
            s.setters.remove(&name);
            let new_abstract_members = update_abstract_members(abstract_, static_, &name, s);
            s.abstract_members = new_abstract_members;
            let new_override_members = update_override_members(override_, &name, id_loc, s);
            s.override_members = new_override_members;
        },
        x,
    );
}

/// Appending a method builds a list of function signatures. This implements the  
/// bahvior of interfaces and declared classes, which interpret duplicate  
/// definitions as branches of a single overloaded method.  
pub fn append_method<C: ConfigTypes>(
    static_: bool,
    abstract_: bool,
    override_: bool,
    name: Name,
    id_loc: ALoc,
    this_write_loc: Option<ALoc>,
    uses_this: bool,
    func_sig_: func_class_sig_types::func::Func<C>,
    set_asts: Option<class_types::SetAsts<C>>,
    set_type: Option<class_types::SetType>,
    x: &mut class_types::Class<C>,
) {
    let flat = static_ || structural(x);
    let set_asts = set_asts.unwrap_or_else(|| Rc::new(|_| {}));
    let set_type = set_type.unwrap_or_else(|| Rc::new(|_| {}));
    let func_info = class_types::FuncInfo {
        id_loc: Some(id_loc.dupe()),
        this_write_loc,
        uses_this,
        func_sig: func_sig_,
        set_asts,
        set_type,
    };
    map_sig(
        static_,
        |s| {
            if flat {
                s.fields.remove(&name);
            }
            s.proto_fields.remove(&name);
            match s.methods.get_mut(&name) {
                Some(fsigs) => {
                    fsigs.push(func_info);
                }
                None => {
                    s.methods.insert(name.dupe(), vec1::vec1![func_info]);
                }
            }
            s.getters.remove(&name);
            s.setters.remove(&name);
            let new_abstract_members = update_abstract_members(abstract_, static_, &name, s);
            s.abstract_members = new_abstract_members;
            let new_override_members = update_override_members(override_, &name, id_loc, s);
            s.override_members = new_override_members;
        },
        x,
    );
}

pub fn append_call<C: ConfigTypes>(static_: bool, t: Type, x: &mut class_types::Class<C>) {
    map_sig(
        static_,
        |s| {
            s.calls.push(t);
        },
        x,
    );
}

// Construct signatures only exist on the instance side of an interface.
// A static `new(): T` on a declared class is an ordinary static method and
// goes through [append_method] with [static_] set instead.
pub fn append_construct<C: ConfigTypes>(t: Type, x: &mut class_types::Class<C>) {
    map_sig(
        false,
        |s| {
            s.constructs.push(t);
        },
        x,
    );
}

pub fn add_getter<C: ConfigTypes>(
    static_: bool,
    abstract_: bool,
    override_: bool,
    name: Name,
    id_loc: ALoc,
    this_write_loc: Option<ALoc>,
    func_sig_: func_class_sig_types::func::Func<C>,
    set_asts: Option<class_types::SetAsts<C>>,
    set_type: Option<class_types::SetType>,
    x: &mut class_types::Class<C>,
) {
    let flat = static_ || structural(x);
    let set_asts = set_asts.unwrap_or_else(|| Rc::new(|_| {}));
    let set_type = set_type.unwrap_or_else(|| Rc::new(|_| {}));
    let func_info = class_types::FuncInfo {
        id_loc: Some(id_loc.dupe()),
        this_write_loc,
        uses_this: false,
        func_sig: func_sig_,
        set_asts,
        set_type,
    };
    map_sig(
        static_,
        |s| {
            if flat {
                s.fields.remove(&name);
            }
            s.proto_fields.remove(&name);
            s.methods.remove(&name);
            s.getters.insert(name.dupe(), func_info);
            let new_abstract_members = update_abstract_members(abstract_, static_, &name, s);
            s.abstract_members = new_abstract_members;
            let new_override_members = update_override_members(override_, &name, id_loc, s);
            s.override_members = new_override_members;
        },
        x,
    );
}

pub fn add_setter<C: ConfigTypes>(
    static_: bool,
    abstract_: bool,
    override_: bool,
    name: Name,
    id_loc: ALoc,
    this_write_loc: Option<ALoc>,
    func_sig_: func_class_sig_types::func::Func<C>,
    set_asts: Option<class_types::SetAsts<C>>,
    set_type: Option<class_types::SetType>,
    x: &mut class_types::Class<C>,
) {
    let flat = static_ || structural(x);
    let set_asts = set_asts.unwrap_or_else(|| Rc::new(|_| {}));
    let set_type = set_type.unwrap_or_else(|| Rc::new(|_| {}));
    let func_info = class_types::FuncInfo {
        id_loc: Some(id_loc.dupe()),
        this_write_loc,
        uses_this: false,
        func_sig: func_sig_,
        set_asts,
        set_type,
    };
    map_sig(
        static_,
        |s| {
            if flat {
                s.fields.remove(&name);
            }
            s.proto_fields.remove(&name);
            s.methods.remove(&name);
            s.setters.insert(name.dupe(), func_info);
            let new_abstract_members = update_abstract_members(abstract_, static_, &name, s);
            s.abstract_members = new_abstract_members;
            let new_override_members = update_override_members(override_, &name, id_loc, s);
            s.override_members = new_override_members;
        },
        x,
    );
}

pub fn mem_constructor<C: ConfigTypes>(x: &class_types::Class<C>) -> bool {
    !x.constructor.is_empty()
}

fn iter_methods_with_name<C: ConfigTypes>(
    f: &mut impl FnMut(&str, &class_types::FuncInfo<C>) -> Result<(), CheckExprError>,
    s: &class_types::Signature<C>,
) -> Result<(), CheckExprError> {
    for (name, func_infos) in &s.methods {
        for func_info in func_infos {
            f(&name.display_smol_str(), func_info)?;
        }
    }
    for (name, func_info) in &s.private_methods {
        f(name.as_str(), func_info)?;
    }
    for (name, func_info) in &s.getters {
        f(&name.display_smol_str(), func_info)?;
    }
    for (name, func_info) in &s.setters {
        f(&name.display_smol_str(), func_info)?;
    }
    Ok(())
}

fn iter_methods<C: ConfigTypes>(
    f: &mut impl FnMut(&class_types::FuncInfo<C>) -> Result<(), CheckExprError>,
    s: &class_types::Signature<C>,
) -> Result<(), CheckExprError> {
    iter_methods_with_name(&mut |_, info| f(info), s)
}

fn this_tparam<C: ConfigTypes>(x: &class_types::Class<C>) -> Option<&TypeParam> {
    match &x.super_ {
        class_types::Super::Class(class_super) => Some(&class_super.this_tparam),
        class_types::Super::Interface(iface) => iface.this_tparam.as_ref(),
    }
}

fn this_t<C: ConfigTypes>(x: &class_types::Class<C>) -> Option<&Type> {
    match &x.super_ {
        class_types::Super::Class(class_super) => Some(&class_super.this_t),
        class_types::Super::Interface(iface) => iface.this_t.as_ref(),
    }
}

fn this_or_mixed<C: ConfigTypes>(loc: ALoc, x: &class_types::Class<C>) -> Type {
    this_t(x)
        .map(|t| t.dupe())
        .unwrap_or_else(|| dummy_this(loc))
}

fn tparams_with_this(tparams: &TypeParams, this_tp: TypeParam) -> TypeParams {
    // Use the loc for the original tparams,
    // or just the loc for the this type if there are no tparams
    let loc = match tparams {
        Some((loc, _)) => loc.dupe(),
        None => this_tp.reason.loc().dupe(),
    };
    // Add the type of `this` to the end of the list of type
    // parameters. Remember, order is important, since we don't have recursive
    // bounds (aka F-bounds): the bound of This refers to all the other type
    // parameters!
    let mut tparams_lst: Vec<TypeParam> = match tparams {
        Some((_, nel)) => nel.iter().cloned().collect(),
        None => Vec::new(),
    };
    tparams_lst.push(this_tp);
    let tparams_nel = vec1::Vec1::try_from_vec(tparams_lst).expect("tparams_lst is non-empty");
    Some((loc, tparams_nel))
}

fn to_field<C: ConfigTypes>(entry: &class_types::FieldPrime<C>) -> Property {
    let (key_loc, polarity, field) = entry;
    let type_ = match field {
        class_types::Field::Annot(t) => t.dupe(),
        class_types::Field::Infer(fsig, _) => func_sig::gettertype(fsig),
    };
    Property::new(PropertyInner::Field(Box::new(FieldData {
        preferred_def_locs: None,
        key_loc: key_loc.dupe(),
        type_,
        polarity: *polarity,
    })))
}

fn to_method<'a, C: crate::func_params_intf::Config>(
    cx: &mut Context<'a>,
    this_default: &Type,
    info: &class_types::FuncInfo<C>,
) -> Property {
    Property::new(PropertyInner::Method {
        key_loc: info.id_loc.dupe(),
        type_: func_sig::methodtype(
            cx,
            info.this_write_loc.dupe(),
            this_default.dupe(),
            &info.func_sig,
        ),
    })
}

/// Bridges the two key types used by class signature maps (`Name` for public
/// members, `FlowSmolStr` for private members) so `fields_to_prop_map` can build
/// a `Name`-keyed `PropertiesMap` from either.
pub(crate) trait AsPropName {
    fn as_prop_name(&self) -> Name;
}

impl AsPropName for Name {
    fn as_prop_name(&self) -> Name {
        self.dupe()
    }
}

impl AsPropName for FlowSmolStr {
    fn as_prop_name(&self) -> Name {
        Name::new(self.dupe())
    }
}

pub(crate) fn fields_to_prop_map<'a, C: ConfigTypes, K: AsPropName>(
    cx: &mut Context<'a>,
    fields: &BTreeMap<K, class_types::FieldPrime<C>>,
) -> properties::Id {
    let pmap: properties::PropertiesMap = fields
        .iter()
        .map(|(name, entry)| (name.as_prop_name(), to_field(entry)))
        .collect();
    cx.generate_property_map(pmap)
}

fn methods_to_prop_map<'a, C: crate::func_params_intf::Config>(
    cx: &mut Context<'a>,
    this_default: &Type,
    static_this: Option<&Type>,
    methods: &BTreeMap<FlowSmolStr, class_types::FuncInfo<C>>,
) -> properties::Id {
    let pmap: properties::PropertiesMap = methods
        .iter()
        .map(|(name, info)| {
            let this_default = if info.uses_this {
                static_this.unwrap_or(this_default)
            } else {
                this_default
            };
            (Name::new(name.dupe()), to_method(cx, this_default, info))
        })
        .collect();
    cx.generate_property_map(pmap)
}

fn elements<'a, C: crate::func_params_intf::Config>(
    cx: &mut Context<'a>,
    this: Type,
    static_: bool,
    constructor: Option<(Option<ALoc>, Type)>,
    s: &class_types::Signature<C>,
) -> (
    FlowOrdSet<Name>,
    BTreeMap<Name, Property>,
    BTreeMap<Name, Property>,
    Option<Type>,
    Option<Type>,
) {
    let this_default = |x: &func_class_sig_types::func::Func<C>, uses_this: bool| -> Type {
        if static_ && !uses_this {
            mixed_t::make(x.reason.dupe())
        } else {
            this.dupe()
        }
    };
    // If this is an overloaded method, create an intersection, attributed
    // to the first declared function signature. If there is a single
    // function signature for this method, simply return the method type.
    let mut methods: BTreeMap<Name, (Option<ALoc>, Type)> = BTreeMap::new();
    for (name, xs) in &s.methods {
        let uses_this = xs.iter().any(|fi| fi.uses_this);
        let ms: Vec<(Option<ALoc>, Type, &class_types::SetType)> = xs
            .iter()
            .map(|fi| {
                let t = func_sig::methodtype(
                    cx,
                    fi.this_write_loc.dupe(),
                    this_default(&fi.func_sig, uses_this),
                    &fi.func_sig,
                );
                (fi.id_loc.dupe(), t, &fi.set_type)
            })
            .collect();
        // Keep track of these before intersections are merged, to enable
        // type information on every member of the intersection.
        for (loc, t, set_type) in &ms {
            if loc.is_some() {
                (set_type)(t.dupe());
            }
        }
        let result = match ms.as_slice() {
            [(loc, t, _)] => (loc.dupe(), t.dupe()),
            [(loc0, t0, _), (_, t1, _), rest @ ..] => {
                let ts: Vec<Type> = rest.iter().map(|(_, t, _)| t.dupe()).collect();
                (
                    loc0.dupe(),
                    Type::new(TypeInner::IntersectionT(
                        type_util::reason_of_t(t0).dupe(),
                        inter_rep::make(t0.dupe(), t1.dupe(), ts.into()),
                    )),
                )
            }
            [] => unreachable!("Nel is non-empty"),
        };
        methods.insert(name.dupe(), result);
    }
    let private_this_default = |fi: &class_types::FuncInfo<C>| -> Type {
        if static_ && fi.uses_this {
            this.dupe()
        } else {
            any_t::make(
                AnySource::Unsound(UnsoundnessKind::BoundFunctionThis),
                fi.func_sig.reason.dupe(),
            )
        }
    };
    for func_info in s.private_methods.values() {
        if func_info.id_loc.is_some() {
            let t = func_sig::methodtype(
                cx,
                func_info.this_write_loc.dupe(),
                private_this_default(func_info),
                &func_info.func_sig,
            );
            (func_info.set_type)(t);
        }
    }
    // Re-add the constructor as a method.
    if let Some(t) = constructor {
        methods.insert(Name::new(FlowSmolStr::new("constructor")), t);
    }
    // If there is a both a getter and a setter, then flow the setter type to
    // the getter. Otherwise just use the getter type or the setter type
    let getters: BTreeMap<Name, (Option<ALoc>, Type, &class_types::SetType)> = s
        .getters
        .iter()
        .map(|(name, fi)| {
            (
                name.dupe(),
                (
                    fi.id_loc.dupe(),
                    func_sig::gettertype(&fi.func_sig),
                    &fi.set_type,
                ),
            )
        })
        .collect();
    let setters: BTreeMap<Name, (Option<ALoc>, Type, &class_types::SetType)> = s
        .setters
        .iter()
        .map(|(name, fi)| {
            (
                name.dupe(),
                (
                    fi.id_loc.dupe(),
                    func_sig::settertype(&fi.func_sig),
                    &fi.set_type,
                ),
            )
        })
        .collect();
    // Register getters and setters with the typed AST
    for (loc, t, set_type) in getters.values() {
        if loc.is_some() {
            (set_type)(t.dupe());
        }
    }
    for (loc, t, set_type) in setters.values() {
        if loc.is_some() {
            (set_type)(t.dupe());
        }
    }
    let mut getters_and_setters: BTreeMap<Name, Property> = BTreeMap::new();
    let all_accessor_keys: BTreeSet<Name> = getters.keys().chain(setters.keys()).duped().collect();
    for name in all_accessor_keys {
        let getter = getters.get(&name);
        let setter = setters.get(&name);
        let prop = match (getter, setter) {
            (Some((get_key_loc, get_type, _)), Some((set_key_loc, set_type, _))) => {
                Property::new(PropertyInner::GetSet(Box::new(GetSetData {
                    get_key_loc: get_key_loc.dupe(),
                    get_type: get_type.dupe(),
                    set_key_loc: set_key_loc.dupe(),
                    set_type: set_type.dupe(),
                })))
            }
            (Some((key_loc, type_, _)), None) => Property::new(PropertyInner::Get {
                key_loc: key_loc.dupe(),
                type_: type_.dupe(),
            }),
            (None, Some((key_loc, type_, _))) => Property::new(PropertyInner::Set {
                key_loc: key_loc.dupe(),
                type_: type_.dupe(),
            }),
            (None, None) => continue,
        };
        getters_and_setters.insert(name, prop);
    }
    let fields: BTreeMap<Name, Property> = s
        .fields
        .iter()
        .map(|(name, entry)| (name.dupe(), to_field(entry)))
        .collect();
    let mut methods_props: BTreeMap<Name, Property> = methods
        .into_iter()
        .map(|(name, (key_loc, type_))| {
            (
                name,
                Property::new(PropertyInner::Method { key_loc, type_ }),
            )
        })
        .collect();
    // Treat proto fields as methods, as they are on the proto object
    for (name, fld) in &s.proto_fields {
        methods_props.insert(name.dupe(), to_field(fld));
    }
    // Treat getters and setters as methods
    for (name, prop) in getters_and_setters {
        methods_props.insert(name, prop);
    }
    let call = {
        let mut iter = s.calls.iter().duped();
        if let Some(t0) = iter.next() {
            if let Some(t1) = iter.next() {
                let ts: Vec<Type> = iter.collect();
                Some(Type::new(TypeInner::IntersectionT(
                    type_util::reason_of_t(&t0).dupe(),
                    inter_rep::make(t0, t1, ts.into()),
                )))
            } else {
                Some(t0)
            }
        } else {
            None
        }
    };
    let construct = {
        let mut iter = s.constructs.iter().duped();
        if let Some(t0) = iter.next() {
            if let Some(t1) = iter.next() {
                let ts: Vec<Type> = iter.collect();
                Some(Type::new(TypeInner::IntersectionT(
                    type_util::reason_of_t(&t0).dupe(),
                    inter_rep::make(t0, t1, ts.into()),
                )))
            } else {
                Some(t0)
            }
        } else {
            None
        }
    };
    // Only un-initialized fields require annotations, so determine now
    // (syntactically) which fields have initializers
    let mut initialized_fields = FlowOrdSet::new();
    for (x, (_, _, field)) in &s.fields {
        match field {
            class_types::Field::Annot(_) => {}
            class_types::Field::Infer(_, _) => {
                initialized_fields.insert(x.dupe());
            }
        }
    }
    (initialized_fields, fields, methods_props, call, construct)
}

fn specialize<'a>(cx: &mut Context<'a>, use_op: UseOp, targs: Option<Vec<Type>>, c: Type) -> Type {
    let reason = type_util::reason_of_t(&c).dupe();
    type_annotation_cons_gen::specialize(cx, c, use_op, reason.dupe(), reason, targs)
}

fn statictype<'a, C: crate::func_params_intf::Config>(
    cx: &mut Context<'a>,
    static_proto: Type,
    x: &class_types::Class<C>,
) -> (FlowOrdSet<Name>, ObjType) {
    let s = &x.static_;
    let loc = x.static_.reason.loc().dupe();
    let this = type_util::class_type(this_or_mixed(loc, x), None);
    let (inited_fields, fields, methods, call, _construct) = elements(cx, this, true, None, s);
    let props: properties::PropertiesMap = {
        let mut seen = std::collections::BTreeSet::new();
        fields
            .iter()
            .chain(methods.iter())
            .map(|(name, prop)| {
                let name_key = name.dupe();
                if !seen.insert(name_key.dupe()) {
                    panic!(
                        "static fields and methods must be disjoint: {}",
                        flow_common::reason::dump_reason(None, &s.reason)
                    );
                }
                (name_key, prop.dupe())
            })
            .collect()
    };
    // Statics are not exact, because we allow width subtyping between them.
    // Specifically, given class A and class B extends A, Class<B> <: Class<A>.
    let static_ = obj_type::mk_with_proto(
        cx,
        s.reason.dupe(),
        ObjKind::Inexact,
        None,
        call,
        Some(props),
        None,
        static_proto,
        false,
    );
    match static_.deref() {
        TypeInner::DefT(_, def_t) => match def_t.deref() {
            DefTInner::ObjT(o) => (inited_fields, o.as_ref().clone()),
            _ => panic!("statics must be an ObjT"),
        },
        _ => panic!("statics must be an ObjT"),
    }
}

fn insttype<'a, C: crate::func_params_intf::Config>(
    cx: &mut Context<'a>,
    initialized_static_fields: FlowOrdSet<Name>,
    inst_kind: Option<InstanceKind>,
    s: &class_types::Class<C>,
) -> InstType {
    let constructor = {
        // Constructors do not bind `this`
        let ts: Vec<(Option<ALoc>, Type)> = s
            .constructor
            .iter()
            .map(|fi| {
                let t = func_sig::methodtype(
                    cx,
                    None,
                    dummy_this(s.instance.reason.loc().dupe()),
                    &fi.func_sig,
                );
                if fi.id_loc.is_some() {
                    (fi.set_type)(t.dupe());
                }
                (fi.id_loc.dupe(), t)
            })
            .collect();
        match ts.as_slice() {
            [] => None,
            [(loc, t)] => Some((loc.dupe(), t.dupe())),
            [(loc0, t0), (_, t1), rest @ ..] => {
                let ts: Vec<Type> = rest.iter().map(|(_, t)| t.dupe()).collect();
                Some((
                    loc0.dupe(),
                    Type::new(TypeInner::IntersectionT(
                        type_util::reason_of_t(t0).dupe(),
                        inter_rep::make(t0.dupe(), t1.dupe(), ts.into()),
                    )),
                ))
            }
        }
    };
    let type_args: Vec<(SubstName, Reason, Type, Polarity)> = match &s.tparams {
        None => Vec::new(),
        Some((_, nel)) => nel
            .iter()
            .map(|tp| {
                let t = s
                    .tparams_map
                    .get(&tp.name)
                    .expect("tparam not found in tparams_map")
                    .dupe();
                (tp.name.dupe(), tp.reason.dupe(), t, tp.polarity)
            })
            .collect(),
    };
    let loc = s.instance.reason.loc().dupe();
    let (initialized_fields, fields, methods, call, construct) = elements(
        cx,
        this_or_mixed(loc.dupe(), s),
        false,
        constructor,
        &s.instance,
    );
    let private_this_type = |sig_: &class_types::Signature<C>| -> Type {
        any_t::make(
            AnySource::Unsound(UnsoundnessKind::BoundFunctionThis),
            sig_.reason.dupe(),
        )
    };
    let private_static_this = type_util::class_type(this_or_mixed(loc, s), None);
    let own_pmap: properties::PropertiesMap = fields
        .iter()
        .map(|(name, prop)| (name.dupe(), prop.dupe()))
        .collect();
    let proto_pmap: properties::PropertiesMap = methods
        .iter()
        .map(|(name, prop)| (name.dupe(), prop.dupe()))
        .collect();
    let strictness_kind = cx.type_strictness_kind();
    InstType::new(InstTypeInner {
        class_id: s.id.dupe(),
        inst_react_dro: None,
        class_name: s.class_name.dupe(),
        type_args: type_args.into(),
        own_props: cx.generate_property_map(own_pmap),
        proto_props: cx.generate_property_map(proto_pmap),
        inst_call_t: call.map(|t| cx.make_call_prop(t)),
        inst_construct_t: construct.map(|t| cx.make_call_prop(t)),
        initialized_fields,
        initialized_static_fields,
        inst_kind: inst_kind.unwrap_or_else(|| get_inst_kind(s)),
        inst_dict: s.instance.dict.clone(),
        class_private_fields: fields_to_prop_map(cx, &s.instance.private_fields),
        class_private_static_fields: fields_to_prop_map(cx, &s.static_.private_fields),
        class_private_methods: methods_to_prop_map(
            cx,
            &private_this_type(&s.instance),
            None,
            &s.instance.private_methods,
        ),
        class_private_static_methods: methods_to_prop_map(
            cx,
            &private_this_type(&s.static_),
            Some(&private_static_this),
            &s.static_.private_methods,
        ),
        inst_abstract: s.abstract_,
        inst_abstract_props: s.instance.abstract_members.iter().duped().collect(),
        strictness_kind,
    })
}

pub fn mk_this<'a>(self_: Type, cx: &mut Context<'a>, reason: Reason) -> (TypeParam, Type) {
    let this_reason = reason.replace_desc(VirtualReasonDesc::RThisType);
    let this_tp = TypeParam::new(TypeParamInner {
        name: SubstName::name(FlowSmolStr::new("this")),
        reason: this_reason,
        bound: self_,
        polarity: Polarity::Positive,
        default: None,
        is_this: true,
        is_const: false,
    });
    let generic = flow_js_utils::generic_of_tparam(cx, |_, x: &Type| x.dupe(), &this_tp);
    (this_tp, generic)
}

fn supertype<'a, C: ConfigTypes>(cx: &mut Context<'a>, x: &class_types::Class<C>) -> (Type, Type) {
    let super_reason = x.instance.reason.dupe().replace_desc(None);
    let static_reason = x.static_.reason.dupe();
    match &x.super_ {
        class_types::Super::Interface(iface) => {
            let extends: Vec<Type> = iface
                .extends
                .iter()
                .zip(iface.extends_binding_kinds.iter())
                .map(|((annot_loc, c, targs_opt), binding_kind)| {
                    match (&iface.this_t, binding_kind, targs_opt) {
                        (Some(this_t), class_types::ClassLikeBindingKind::ClassLikeMono, _) => {
                            type_util::this_typeapp(
                                c.dupe(),
                                this_t.dupe(),
                                targs_opt.clone(),
                                Some(annot_loc.dupe()),
                            )
                        }
                        (
                            Some(this_t),
                            class_types::ClassLikeBindingKind::ClassLikePoly,
                            Some(_),
                        ) => type_util::this_typeapp(
                            c.dupe(),
                            this_t.dupe(),
                            targs_opt.clone(),
                            Some(annot_loc.dupe()),
                        ),
                        (Some(_), class_types::ClassLikeBindingKind::ClassLikePoly, None) => {
                            let reason = type_util::reason_of_t(c)
                                .dupe()
                                .reposition(annot_loc.dupe())
                                .annotate(annot_loc.dupe());
                            type_annotation_cons_gen::mk_instance(cx, reason, c.dupe(), None, None)
                        }
                        (
                            Some(this_t),
                            class_types::ClassLikeBindingKind::ClassLikeRawMono(raw_c),
                            _,
                        ) => type_util::this_typeapp(
                            raw_c.dupe(),
                            this_t.dupe(),
                            targs_opt.clone(),
                            Some(annot_loc.dupe()),
                        ),
                        (
                            Some(this_t),
                            class_types::ClassLikeBindingKind::ClassLikeRawPoly(raw_c),
                            Some(_),
                        ) => type_util::this_typeapp(
                            raw_c.dupe(),
                            this_t.dupe(),
                            targs_opt.clone(),
                            Some(annot_loc.dupe()),
                        ),
                        (
                            Some(_),
                            class_types::ClassLikeBindingKind::ClassLikeRawPoly(raw_c),
                            None,
                        ) => {
                            let reason = type_util::reason_of_t(raw_c)
                                .dupe()
                                .reposition(annot_loc.dupe())
                                .annotate(annot_loc.dupe());
                            type_annotation_cons_gen::mk_instance(
                                cx,
                                reason,
                                raw_c.dupe(),
                                None,
                                None,
                            )
                        }
                        (_, _, None) => {
                            let reason = type_util::reason_of_t(c)
                                .dupe()
                                .reposition(annot_loc.dupe())
                                .annotate(annot_loc.dupe());
                            type_annotation_cons_gen::mk_instance(cx, reason, c.dupe(), None, None)
                        }
                        (_, _, Some(targs)) => type_util::typeapp_annot(
                            false,
                            false,
                            annot_loc.dupe(),
                            c.dupe(),
                            targs.clone(),
                        ),
                    }
                })
                .collect();
            let extends = if iface.function_like {
                let mut e = extends;
                e.push(Type::new(TypeInner::FunProtoT(super_reason.dupe())));
                e
            } else {
                extends
            };
            // Interfaces support multiple inheritance, which is modelled as an
            // intersection of super types. TODO: Instead of using an intersection for
            // this, we should resolve the extends and build a flattened type, and just
            // use FunProtoT/ObjProtoT as the prototype
            let super_ = match extends.len() {
                0 => Type::new(TypeInner::ObjProtoT(super_reason.dupe())),
                1 => extends.into_iter().next().unwrap(),
                _ => {
                    let mut iter = extends.into_iter();
                    let t0 = iter.next().unwrap();
                    let t1 = iter.next().unwrap();
                    let ts: Vec<Type> = iter.collect();
                    Type::new(TypeInner::IntersectionT(
                        super_reason.dupe(),
                        inter_rep::make(t0, t1, ts.into()),
                    ))
                }
            };
            let static_proto = Type::new(TypeInner::NullProtoT(static_reason));
            (super_, static_proto)
        }
        class_types::Super::Class(class_super) => {
            let (extends_t, static_proto) = match &class_super.extends {
                class_types::Extends::Explicit((annot_loc, c, targs)) => {
                    // Eagerly specialize when there are no targs
                    let c = if targs.is_none() {
                        let use_op = UseOp::Op(Arc::new(RootUseOp::ClassExtendsCheck(Box::new(
                            ClassExtendsCheckData {
                                def_loc: x.instance.reason.loc.dupe(),
                                def: TypeOrTypeDescT::Type(class_super.this_tparam.bound.dupe()),
                                extends_loc: type_util::loc_of_t(c).dupe(),
                                extends: TypeOrTypeDescT::Type(c.dupe()),
                            },
                        ))));
                        specialize(cx, use_op, targs.clone(), c.dupe())
                    } else {
                        c.dupe()
                    };
                    let t = type_util::this_typeapp(
                        c.dupe(),
                        class_super.this_t.dupe(),
                        targs.clone(),
                        Some(annot_loc.dupe()),
                    );
                    // class B extends A {}; B.__proto__ === A. This must stay
                    // eager: `t` mentions the derived class's `this`, and
                    // substitution does not descend into a tvar, so deferring
                    // it strands `this` as `empty` for every inherited static.
                    // A base that is a value with a construct signature instead
                    // hands its statics over on the instance itself, via
                    // [construct_base_instance].
                    let static_proto = type_util::class_type(t.dupe(), Some(annot_loc.dupe()));
                    (t, static_proto)
                }
                class_types::Extends::Implicit { null } => {
                    let t = if *null {
                        Type::new(TypeInner::NullProtoT(super_reason.dupe()))
                    } else {
                        Type::new(TypeInner::ObjProtoT(super_reason.dupe()))
                    };
                    // class A {}; A.__proto__ === Function.prototype
                    let static_proto = Type::new(TypeInner::FunProtoT(static_reason));
                    (t, static_proto)
                }
            };
            let mixins: Vec<Type> = class_super
                .mixins
                .iter()
                .map(|(annot_loc, c, targs)| {
                    // Eagerly specialize when there are no targs
                    let c = if targs.is_none() {
                        specialize(cx, unknown_use(), targs.clone(), c.dupe())
                    } else {
                        c.dupe()
                    };
                    type_util::this_typeapp(
                        c,
                        class_super.this_t.dupe(),
                        targs.clone(),
                        Some(annot_loc.dupe()),
                    )
                })
                .collect();
            let mut all: Vec<Type> = mixins;
            all.push(extends_t);
            let super_ = match all.len() {
                0 => panic!("impossible"),
                1 => all.into_iter().next().unwrap(),
                _ => {
                    let mut iter = all.into_iter();
                    let t0 = iter.next().unwrap();
                    let t1 = iter.next().unwrap();
                    let ts: Vec<Type> = iter.collect();
                    Type::new(TypeInner::IntersectionT(
                        super_reason,
                        inter_rep::make(t0, t1, ts.into()),
                    ))
                }
            };
            (super_, static_proto)
        }
    }
}

fn this_instance_type<'a, C: crate::func_params_intf::Config>(
    cx: &mut Context<'a>,
    inst_kind: Option<InstanceKind>,
    x: &class_types::Class<C>,
) -> (Reason, InstanceT) {
    let sreason = x.static_.reason.dupe();
    let reason = x.instance.reason.dupe();
    let (super_, static_proto) = supertype(cx, x);
    let implements: Vec<Type> = match &x.super_ {
        class_types::Super::Interface(_) => Vec::new(),
        class_types::Super::Class(class_super) => class_super
            .implements
            .iter()
            .map(|(annot_loc, c, targs_opt)| match targs_opt {
                None => {
                    let reason = type_util::reason_of_t(c)
                        .dupe()
                        .reposition(annot_loc.dupe())
                        .annotate(annot_loc.dupe());
                    type_annotation_cons_gen::mk_instance(cx, reason, c.dupe(), None, None)
                }
                Some(targs) => type_util::typeapp_annot(
                    false,
                    false,
                    annot_loc.dupe(),
                    c.dupe(),
                    targs.clone(),
                ),
            })
            .collect(),
    };
    let (initialized_static_fields, static_objtype) = statictype(cx, static_proto, x);
    let inst = insttype(cx, initialized_static_fields, inst_kind, x);
    let static_ = Type::new(TypeInner::DefT(
        sreason,
        DefT::new(DefTInner::ObjT(Rc::new(static_objtype))),
    ));
    (
        reason,
        InstanceT::new(InstanceTInner {
            inst,
            static_,
            super_,
            implements: implements.into(),
        }),
    )
}

pub fn thistype<'a, C: crate::func_params_intf::Config>(
    cx: &mut Context<'a>,
    x: &class_types::Class<C>,
) -> Type {
    let (reason, instance_t) = this_instance_type(cx, None, x);
    match &x.super_ {
        class_types::Super::Interface(iface) if iface.this_tparam.is_some() => {
            let this_name = SubstName::name(FlowSmolStr::new_inline("this"));
            if iface.inline {
                flow_js_utils::fix_this_instance(
                    cx,
                    reason.dupe(),
                    reason,
                    &instance_t,
                    false,
                    this_name,
                )
            } else {
                Type::new(TypeInner::ThisInstanceT(Box::new(ThisInstanceTData {
                    reason,
                    instance: instance_t,
                    is_this: false,
                    subst_name: this_name,
                })))
            }
        }
        _ => Type::new(TypeInner::DefT(
            reason,
            DefT::new(DefTInner::InstanceT(Rc::new(instance_t))),
        )),
    }
}

fn check_methods<'a, C: crate::func_params_intf::Config>(
    cx: &mut Context<'a>,
    def_reason: Reason,
    x: &class_types::Class<C>,
    self_: &Type,
) {
    let skip = files::has_ts_ext(cx.file()) || cx.is_global_lib_context();
    let mut check_method =
        |msig: &func_class_sig_types::func::Func<C>, static_: bool, name: &str, id_loc: ALoc| {
            if let Some(this_param) = func_sig::this_param(&msig.fparams) {
                if skip {
                    return;
                }
                let self_val = if static_ {
                    type_util::class_type(self_.dupe(), None)
                } else {
                    self_.dupe()
                };
                let use_op = UseOp::Op(Arc::new(RootUseOp::ClassMethodDefinition(Box::new(
                    ClassMethodDefinitionData {
                        def_loc: id_loc,
                        method_name: FlowSmolStr::new(name),
                        name: def_reason.dupe(),
                    },
                ))));
                cx.add_post_inference_subtyping_check(self_val, use_op, this_param);
            }
        };
    with_sig(
        true,
        |s| {
            iter_methods_with_name(
                &mut |name: &str, fi: &class_types::FuncInfo<C>| {
                    if let Some(id_loc) = &fi.id_loc {
                        check_method(&fi.func_sig, true, name, id_loc.dupe());
                    }
                    Ok(())
                },
                s,
            )
            .unwrap();
        },
        x,
    );
    with_sig(
        false,
        |s| {
            iter_methods_with_name(
                &mut |name: &str, fi: &class_types::FuncInfo<C>| {
                    if let Some(id_loc) = &fi.id_loc {
                        check_method(&fi.func_sig, false, name, id_loc.dupe());
                    }
                    Ok(())
                },
                s,
            )
            .unwrap();
        },
        x,
    );
}

fn check_implements<'a, C: crate::func_params_intf::Config>(
    cx: &mut Context<'a>,
    def_reason: Reason,
    x: &class_types::Class<C>,
) {
    match &x.super_ {
        class_types::Super::Interface(_) => {}
        class_types::Super::Class(class_super) => {
            let this = thistype(cx, x);
            let reason = x.instance.reason.dupe();
            for ((annot_loc, c, targs_opt), binding_kind) in class_super
                .implements
                .iter()
                .zip(class_super.implements_binding_kinds.iter())
            {
                let i = match (binding_kind, targs_opt) {
                    (class_types::ClassLikeBindingKind::ClassLikeMono, _) => {
                        type_util::this_typeapp(
                            c.dupe(),
                            class_super.this_t.dupe(),
                            targs_opt.clone(),
                            Some(annot_loc.dupe()),
                        )
                    }
                    (class_types::ClassLikeBindingKind::ClassLikePoly, Some(_)) => {
                        type_util::this_typeapp(
                            c.dupe(),
                            class_super.this_t.dupe(),
                            targs_opt.clone(),
                            Some(annot_loc.dupe()),
                        )
                    }
                    (class_types::ClassLikeBindingKind::ClassLikePoly, None) => {
                        let r = type_util::reason_of_t(c)
                            .dupe()
                            .reposition(annot_loc.dupe())
                            .annotate(annot_loc.dupe());
                        type_annotation_cons_gen::mk_instance(cx, r, c.dupe(), None, None)
                    }
                    (class_types::ClassLikeBindingKind::ClassLikeRawMono(raw_c), _) => {
                        type_util::this_typeapp(
                            raw_c.dupe(),
                            class_super.this_t.dupe(),
                            targs_opt.clone(),
                            Some(annot_loc.dupe()),
                        )
                    }
                    (class_types::ClassLikeBindingKind::ClassLikeRawPoly(raw_c), Some(_)) => {
                        type_util::this_typeapp(
                            raw_c.dupe(),
                            class_super.this_t.dupe(),
                            targs_opt.clone(),
                            Some(annot_loc.dupe()),
                        )
                    }
                    (class_types::ClassLikeBindingKind::ClassLikeRawPoly(raw_c), None) => {
                        let r = type_util::reason_of_t(raw_c)
                            .dupe()
                            .reposition(annot_loc.dupe())
                            .annotate(annot_loc.dupe());
                        type_annotation_cons_gen::mk_instance(cx, r, raw_c.dupe(), None, None)
                    }
                    (
                        class_types::ClassLikeBindingKind::BindingOther
                        | class_types::ClassLikeBindingKind::BindingUnknown,
                        None,
                    ) => {
                        let r = type_util::reason_of_t(c)
                            .dupe()
                            .reposition(annot_loc.dupe())
                            .annotate(annot_loc.dupe());
                        type_annotation_cons_gen::mk_instance(cx, r, c.dupe(), None, None)
                    }
                    (
                        class_types::ClassLikeBindingKind::BindingOther
                        | class_types::ClassLikeBindingKind::BindingUnknown,
                        Some(targs),
                    ) => type_util::typeapp_annot(
                        false,
                        false,
                        annot_loc.dupe(),
                        c.dupe(),
                        targs.clone(),
                    ),
                };
                let use_op = UseOp::Op(Arc::new(RootUseOp::ClassImplementsCheck(Box::new(
                    ClassImplementsCheckData {
                        def: def_reason.dupe(),
                        name: reason.dupe(),
                        implements_loc: type_util::ref_loc_of_t(&i).dupe(),
                        implements: TypeOrTypeDescT::Type(i.dupe()),
                    },
                ))));
                cx.add_post_inference_validation_flow(
                    i,
                    UseT::new(UseTInner::ImplementsT(use_op, this.dupe())),
                );
            }
        }
    }
}

fn check_super<'a, C: crate::func_params_intf::Config>(
    cx: &mut Context<'a>,
    def_reason: Reason,
    x: &class_types::Class<C>,
    self_: &Type,
) {
    let reason = x.instance.reason.dupe();
    // NOTE: SuperT ignores the constructor anyway, so we don't pass it here.
    // Call properties are also ignored, so we ignore that result.
    // The this parameter of a class method is irrelvant for class subtyping, since
    // dynamic dispatch enforces that the method is called on the right subclass
    // at runtime even if the static type is a supertype.
    let inst_loc = reason.loc().dupe();
    let (_, own, proto, _call, _construct) = elements(
        cx,
        this_or_mixed(inst_loc.dupe(), x),
        false,
        None,
        &x.instance,
    );
    let static_ = {
        let this = type_util::class_type(this_or_mixed(inst_loc, x), None);
        // NOTE: The own, proto maps are disjoint by construction.
        let (_, own, proto, _call, _construct) = elements(cx, this, true, None, &x.static_);
        let mut merged = own;
        for (k, v) in proto {
            merged.insert(k, v);
        }
        merged
    };
    let derived_t = match &x.super_ {
        class_types::Super::Class(class_super) => class_super.this_tparam.bound.dupe(),
        class_types::Super::Interface(_) => self_.dupe(),
    };
    let reference = match &x.class_name {
        Some(name) => DerivedReference::Named(name.dupe()),
        None => DerivedReference::Unnamed(derived_t.dupe()),
    };
    let polarity_object = match &reference {
        DerivedReference::Named(name) => PropPolarityMismatchObject::Class(NamedReferenceData {
            loc: reason.loc().dupe(),
            name: name.dupe(),
        }),
        DerivedReference::Unnamed(t) => PropPolarityMismatchObject::Type(t),
    };
    for (x_name, p1) in &own {
        if let Some(p2) = proto.get(x_name) {
            let prop = x_name.dupe();
            let use_op = UseOp::Op(Arc::new(RootUseOp::ClassOwnProtoCheck(Box::new(
                ClassOwnProtoCheckData {
                    prop: prop.dupe(),
                    own_loc: property::first_loc(p1),
                    proto_loc: property::first_loc(p2),
                },
            ))));
            let propref = type_util::mk_named_prop(reason.dupe(), false, prop);
            let pt1 = property::property_type(p1);
            let pt2 = property::property_type(p2);
            flow_js::FlowJs::flow_p(
                cx,
                use_op,
                polarity_object.dupe(),
                polarity_object.dupe(),
                &propref,
                &pt1,
                &pt2,
            )
            .expect("flow_p should not fail outside speculation");
        }
    }
    let (super_, _) = supertype(cx, x);
    let use_op = UseOp::Op(Arc::new(RootUseOp::ClassExtendsCheck(Box::new(
        ClassExtendsCheckData {
            def_loc: def_reason.loc().dupe(),
            def: TypeOrTypeDescT::Type(derived_t.dupe()),
            extends_loc: type_util::loc_of_t(&super_).dupe(),
            extends: TypeOrTypeDescT::Type(super_.dupe()),
        },
    ))));
    let mut own_name_map: FlowOrdMap<Name, Property> = FlowOrdMap::new();
    for (k, v) in &own {
        own_name_map.insert(k.dupe(), v.dupe());
    }
    let mut proto_name_map: FlowOrdMap<Name, Property> = FlowOrdMap::new();
    for (k, v) in &proto {
        proto_name_map.insert(k.dupe(), v.dupe());
    }
    let mut static_name_map: FlowOrdMap<Name, Property> = FlowOrdMap::new();
    for (k, v) in &static_ {
        static_name_map.insert(k.dupe(), v.dupe());
    }
    cx.add_post_inference_validation_flow(
        super_,
        UseT::new(UseTInner::SuperT(Box::new(SuperTData {
            use_op,
            reason,
            derived_type: DerivedType {
                own: own_name_map,
                proto: proto_name_map,
                static_: static_name_map,
                // Overrides in library definitions are checked by TypeScript's rules: an
                // inherited property may be redeclared with a subtype.
                strictness_kind: if cx.is_global_lib_context() {
                    TypeStrictnessKind::TypeScriptLoose
                } else {
                    cx.type_strictness_kind()
                },
                reference,
                instance: derived_t,
            },
        }))),
    );
}

// Walk super chain leaf→root collecting both:
// - [obligations]: names declared abstract by some ancestor
// - [satisfied]: names concretely implemented by some intermediate
//   ancestor (shallower than the abstract declaration)
// A name is only an open obligation if it's in [obligations] and
// NOT in [satisfied] and NOT in [own_concrete_names]. Per-name
// [first_loc] tracking uses the SHALLOWEST abstract decl so the
// error points at the closest ancestor that left it open.
fn post_inference_check_abstract_obligations<'cx>(
    cx: &mut Context<'cx>,
    super_: &Type,
    class_name: &Option<FlowSmolStr>,
    class_loc: &ALoc,
    def_loc: &ALoc,
    own_concrete_names: &BTreeSet<Name>,
) -> Result<(), flow_utils_concurrency::job_error::JobError> {
    let seen: RefCell<BTreeSet<properties::Id>> = RefCell::new(BTreeSet::new());
    let obligations: RefCell<BTreeMap<Name, ALoc>> = RefCell::new(BTreeMap::new());
    let satisfied: RefCell<BTreeSet<Name>> = RefCell::new(BTreeSet::new());
    let add_obligation = |name: Name, loc: ALoc| {
        obligations.borrow_mut().entry(name).or_insert(loc);
    };
    let process_inst = |cx: &Context<'cx>, inst: &InstType| -> bool {
        if seen.borrow().contains(&inst.own_props) {
            false
        } else {
            seen.borrow_mut().insert(inst.own_props.dupe());
            let own_props = cx.find_props(inst.own_props.dupe());
            let proto_props = cx.find_props(inst.proto_props.dupe());
            // Concrete impls on this ancestor satisfy any abstract
            // obligation from a deeper (further-from-leaf) ancestor.
            // We rely on leaf→root traversal: by the time we add a name
            // to [obligations], any concrete impl above us has already
            // been added to [satisfied].
            let add_concrete = |name: &Name, _: &Property| {
                if !inst.inst_abstract_props.contains(name) {
                    satisfied.borrow_mut().insert(name.dupe());
                }
            };
            for (name, p) in own_props.iter() {
                add_concrete(name, p);
            }
            for (name, p) in proto_props.iter() {
                add_concrete(name, p);
            }
            for name in inst.inst_abstract_props.iter() {
                if !satisfied.borrow().contains(name) {
                    let prop_loc = match own_props.get(name) {
                        Some(p) => property::first_loc(p),
                        None => match proto_props.get(name) {
                            Some(p) => property::first_loc(p),
                            None => None,
                        },
                    };
                    let loc = prop_loc.unwrap_or_else(|| def_loc.dupe());
                    add_obligation(name.dupe(), loc);
                }
            }
            true
        }
    };
    fn walk<'cx>(
        cx: &mut Context<'cx>,
        process_inst: &dyn Fn(&Context<'cx>, &InstType) -> bool,
        t: &Type,
    ) -> Result<(), FlowJsException> {
        let concretes = cx.with_suppressed_errors(|cx| {
            FlowJs::possible_concrete_types_for_inspection(cx, type_util::reason_of_t(t), t)
        })?;
        for c in &concretes {
            walk_one(cx, process_inst, c)?;
        }
        Ok(())
    }
    fn walk_one<'cx>(
        cx: &mut Context<'cx>,
        process_inst: &dyn Fn(&Context<'cx>, &InstType) -> bool,
        t: &Type,
    ) -> Result<(), FlowJsException> {
        match t.deref() {
            TypeInner::ThisInstanceT(box ThisInstanceTData { instance, .. })
                if process_inst(cx, &instance.inst) =>
            {
                walk(cx, process_inst, &instance.super_)?;
            }
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::InstanceT(inst_t) if process_inst(cx, &inst_t.inst) => {
                    walk(cx, process_inst, &inst_t.super_)?;
                }
                _ => {}
            },
            TypeInner::IntersectionT(_, rep) => {
                for m in rep.members_iter() {
                    walk_one(cx, process_inst, m)?;
                }
            }
            // Union supers are not a real Flow construct here — a class
            // can't [extends (A | B)] — and the previous implementation
            // walked branches with shared [satisfied] state, letting a
            // concrete impl in one branch silently discharge an abstract
            // obligation declared only in another. We don't model the
            // correct per-branch semantics; fall through and skip.
            // If union supers ever become reachable, implement per-branch
            // [satisfied] with [intersect across branches] semantics.
            _ => {}
        }
        Ok(())
    }
    let walk_result = {
        let pi: &dyn Fn(&Context<'cx>, &InstType) -> bool = &process_inst;
        walk(cx, pi, super_)
    };
    match walk_result {
        Ok(()) => {}
        Err(FlowJsException::WorkerCanceled(c)) => {
            return Err(flow_utils_concurrency::job_error::JobError::Canceled(c));
        }
        Err(FlowJsException::TimedOut(t)) => {
            return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
        }
        Err(_) => {}
    }
    // Emit one error per unimplemented obligation.
    for (name, member_def_loc) in obligations.borrow().iter() {
        if !own_concrete_names.contains(name) && !satisfied.borrow().contains(name) {
            flow_js_utils::add_output_non_speculating(
                cx,
                ErrorMessage::EAbstractClass(Box::new(EAbstractClassData {
                    kind: AbstractErrorKind::AbstractMemberNotImplemented {
                        class_name: class_name.clone(),
                        member_name: name.display_smol_str(),
                        member_def_loc: member_def_loc.dupe(),
                    },
                    loc: class_loc.dupe(),
                })),
            );
        }
    }
    Ok(())
}

// For each abstract member inherited via [super], verify that [x]
// implements it. Members [x] itself re-declared as abstract do not
// satisfy the obligation (those names live in [x.instance.abstract_members]).
// Skipped entirely when [x] is itself abstract — an abstract subclass may
// leave obligations open for its own concrete subclasses. Also skipped
// when the experimental flag is off: [inst_abstract_props] is guaranteed
// empty everywhere, so the walk would always find no obligations — pure
// wasted work on every non-abstract class in every Flow root.
fn check_abstract_obligations<'cx, C: crate::func_params_intf::Config>(
    cx: &mut Context<'cx>,
    def_reason: Reason,
    x: &class_types::Class<C>,
) {
    if !cx.tslib_syntax() || x.abstract_ {
        return;
    }
    // Set of names [x] re-declares as abstract; these do not implement
    // any inherited obligation.
    let own_abstract = x.instance.abstract_members.clone();
    // Names of [x]'s own concrete instance members (fields, methods,
    // getters, setters, proto fields). These satisfy obligations.
    let own_concrete_names: BTreeSet<Name> = {
        let s = &x.instance;
        let mut acc: BTreeSet<Name> = BTreeSet::new();
        acc.extend(s.fields.keys().duped());
        acc.extend(s.proto_fields.keys().duped());
        acc.extend(s.methods.keys().duped());
        acc.extend(s.getters.keys().duped());
        acc.extend(s.setters.keys().duped());
        let s = acc;
        s.difference(&own_abstract).cloned().collect()
    };
    let (super_, _) = supertype(cx, x);
    let class_name = x.class_name.clone();
    let class_loc = x.class_loc.dupe();
    let def_loc = def_reason.loc().dupe();
    cx.add_post_inference_validation_callback(Box::new(move |cx: &mut Context<'_>| {
        post_inference_check_abstract_obligations(
            cx,
            &super_,
            &class_name,
            &class_loc,
            &def_loc,
            &own_concrete_names,
        )
    }));
}

// Determine whether the resolved [super] type carries a real [extends]
// clause. The "no real super" forms — [null] / [ObjProtoT] / [FunProtoT]
// / [MixedT] — mean the class either has no [extends] at all or extends
// a sentinel placeholder. In any of those cases [override] on a member
// is nonsensical (no member to inherit from).
//
// Wrappers ([AnnotT], [OpaqueT], [UnionT], [TypeAppT], [EvalT], [OpenT]
// tvars, ...) are concretized first so that, e.g., [extends OpaqueAlias]
// where the alias resolves to [ObjProtoT] is correctly classified as
// "no real extends" instead of leaking into the not-inherited check's
// "not declared in the base class" branch.
fn super_has_real_extends<'cx>(cx: &mut Context<'cx>, t: &Type) -> bool {
    fn check(t: &Type) -> bool {
        match t.deref() {
            TypeInner::NullProtoT(_) | TypeInner::ObjProtoT(_) | TypeInner::FunProtoT(_) => false,
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::NullT | DefTInner::MixedT(_) => false,
                _ => true,
            },
            TypeInner::IntersectionT(_, rep) => rep.members_iter().any(check),
            _ => true,
        }
    }
    let concretes = cx
        .with_suppressed_errors(|cx| {
            FlowJs::possible_concrete_types_for_inspection(cx, type_util::reason_of_t(t), t)
        })
        .unwrap_or_default();
    concretes.iter().any(check)
}

// Per-side inherited-walk state. [locs] / [owners] are populated for
// both sides; [name_abstract] is meaningful only on the instance side
// (the static side leaves it empty since [abstract_members] is
// instance-only).
struct InheritedSideState {
    locs: BTreeMap<Name, ALoc>,
    owners: BTreeMap<Name, Option<FlowSmolStr>>,
    name_abstract: BTreeMap<Name, bool>,
}

fn mk_inherited_side_state() -> InheritedSideState {
    InheritedSideState {
        locs: BTreeMap::new(),
        owners: BTreeMap::new(),
        name_abstract: BTreeMap::new(),
    }
}

// Walk the super chain ONCE and populate BOTH instance- and static-side
// inherited tables in parallel. Each ancestor [InstanceT] contributes:
// - instance: [own_props] + [proto_props] (public namespace; private
//   members live in separate slots that are not crossed here);
// - static: the [static] slot, concretized through wrappers
//   ([AnnotT], [EvalT], [OpenT], [IntersectionT], ...) before reading
//   the prop map. Without that concretization, cross-module inheritance
//   (which commonly wraps the static slot in [AnnotT]) would silently
//   drop the ancestor's static members.
// First-write wins for both sides — closest ancestor's loc/owner/
// abstract bit is recorded. Returns the per-side state plus the
// immediate-super's class name (for [base_class_name] in errors).
fn collect_inherited_members_both_sides<'cx>(
    cx: &mut Context<'cx>,
    super_: &Type,
) -> Result<
    (InheritedSideState, InheritedSideState, Option<FlowSmolStr>),
    flow_utils_concurrency::job_error::JobError,
> {
    let seen: RefCell<BTreeSet<properties::Id>> = RefCell::new(BTreeSet::new());
    let instance_state: RefCell<InheritedSideState> = RefCell::new(mk_inherited_side_state());
    let static_state: RefCell<InheritedSideState> = RefCell::new(mk_inherited_side_state());
    let immediate_super_name: RefCell<Option<FlowSmolStr>> = RefCell::new(None);
    let first_inst: RefCell<bool> = RefCell::new(true);
    let add_member = |state: &RefCell<InheritedSideState>,
                      inst_name: &Option<FlowSmolStr>,
                      is_abstract: bool,
                      name: &Name,
                      loc_opt: Option<ALoc>| {
        let mut s = state.borrow_mut();
        if !s.locs.contains_key(name) {
            let loc = loc_opt.unwrap_or_else(ALoc::none);
            s.locs.insert(name.dupe(), loc);
            s.owners.insert(name.dupe(), inst_name.clone());
            s.name_abstract.insert(name.dupe(), is_abstract);
        }
    };
    let process_inst = |cx: &mut Context<'cx>, instance_t: &InstanceT| -> bool {
        let inst = &instance_t.inst;
        if seen.borrow().contains(&inst.own_props) {
            false
        } else {
            seen.borrow_mut().insert(inst.own_props.dupe());
            let inst_name = inst.class_name.dupe();
            if *first_inst.borrow() {
                *immediate_super_name.borrow_mut() = inst_name.clone();
                *first_inst.borrow_mut() = false;
            }
            let own_props = cx.find_props(inst.own_props.dupe());
            let proto_props = cx.find_props(inst.proto_props.dupe());
            let visit_instance = |props: &properties::PropertiesMap| {
                for (name, p) in props.iter() {
                    let is_abstract = inst.inst_abstract_props.contains(name);
                    add_member(
                        &instance_state,
                        &inst_name,
                        is_abstract,
                        name,
                        property::first_loc(p),
                    );
                }
            };
            visit_instance(&own_props);
            visit_instance(&proto_props);
            fn visit_static_concrete<'cx>(
                cx: &mut Context<'cx>,
                static_state: &RefCell<InheritedSideState>,
                add_member: &dyn Fn(
                    &RefCell<InheritedSideState>,
                    &Option<FlowSmolStr>,
                    bool,
                    &Name,
                    Option<ALoc>,
                ),
                inst_name: &Option<FlowSmolStr>,
                t: &Type,
            ) {
                match t.deref() {
                    TypeInner::DefT(_, def_t) => match def_t.deref() {
                        DefTInner::ObjT(obj) => {
                            let static_props = cx.find_props(obj.props_tmap.dupe());
                            for (name, p) in static_props.iter() {
                                // Static members can't be abstract in our model.
                                add_member(
                                    static_state,
                                    inst_name,
                                    false,
                                    name,
                                    property::first_loc(p),
                                );
                            }
                        }
                        _ => {}
                    },
                    TypeInner::IntersectionT(_, rep) => {
                        for m in rep.members_iter() {
                            visit_static_concrete(cx, static_state, add_member, inst_name, m);
                        }
                    }
                    _ => {}
                }
            }
            let static_concretes = cx
                .with_suppressed_errors(|cx| {
                    FlowJs::possible_concrete_types_for_inspection(
                        cx,
                        type_util::reason_of_t(&instance_t.static_),
                        &instance_t.static_,
                    )
                })
                .unwrap_or_default();
            for c in &static_concretes {
                visit_static_concrete(cx, &static_state, &add_member, &inst_name, c);
            }
            true
        }
    };
    fn walk<'cx>(
        cx: &mut Context<'cx>,
        process_inst: &dyn Fn(&mut Context<'cx>, &InstanceT) -> bool,
        t: &Type,
    ) -> Result<(), FlowJsException> {
        let concretes = cx.with_suppressed_errors(|cx| {
            FlowJs::possible_concrete_types_for_inspection(cx, type_util::reason_of_t(t), t)
        })?;
        for c in &concretes {
            walk_one(cx, process_inst, c)?;
        }
        Ok(())
    }
    fn walk_one<'cx>(
        cx: &mut Context<'cx>,
        process_inst: &dyn Fn(&mut Context<'cx>, &InstanceT) -> bool,
        t: &Type,
    ) -> Result<(), FlowJsException> {
        match t.deref() {
            TypeInner::ThisInstanceT(box ThisInstanceTData { instance, .. }) => {
                if process_inst(cx, instance) {
                    walk(cx, process_inst, &instance.super_)?;
                }
            }
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::InstanceT(inst_t) if process_inst(cx, inst_t) => {
                    walk(cx, process_inst, &inst_t.super_)?;
                }
                _ => {}
            },
            TypeInner::IntersectionT(_, rep) => {
                for m in rep.members_iter() {
                    walk_one(cx, process_inst, m)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let pi: &dyn Fn(&mut Context<'cx>, &InstanceT) -> bool = &process_inst;
    let walk_result = walk(cx, pi, super_);
    match walk_result {
        Ok(()) => {}
        Err(FlowJsException::WorkerCanceled(c)) => {
            return Err(flow_utils_concurrency::job_error::JobError::Canceled(c));
        }
        Err(FlowJsException::TimedOut(t)) => {
            return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
        }
        Err(_) => {}
    }
    Ok((
        instance_state.into_inner(),
        static_state.into_inner(),
        immediate_super_name.into_inner(),
    ))
}

// Run the three override-check sub-checks for one side (instance OR
// static):
// - no-extends check: every [override] member is rejected if the class
//   has no real super class to inherit from.
// - not-inherited check: every [override] member's name must exist
//   somewhere in the inherited set.
// - implicit-override check (NIO): every concrete own member that
//   shadows an inherited member must carry the [override] modifier.
//
// The no-extends check, when it fires, is mutually exclusive with the
// other two (they assume a real super chain to walk).
//
// Arguments:
// [own_concrete_names]: names of [x]'s own concrete same-side members.
// [override_members]: names of [x]'s same-side members declared [override],
//   mapped to their def locs.
// [super]: the same-side super type, used by the no-extends check.
// [inherited_state]: precomputed inherited members for this side (already
//   walked once for both sides; see [collect_inherited_members_both_sides]).
// [immediate_super_name]: name of the closest super class (for
//   [base_class_name] in [OverrideOfNonInheritedMember]).
// [skip_implicit_check]: when true, the implicit-override check is
//   skipped.
#[allow(clippy::too_many_arguments)]
fn check_override_side<'cx>(
    cx: &mut Context<'cx>,
    class_name: &Option<FlowSmolStr>,
    own_concrete_names: &BTreeMap<Name, Option<ALoc>>,
    override_members: &BTreeMap<Name, ALoc>,
    super_: &Type,
    inherited_state: &InheritedSideState,
    immediate_super_name: &Option<FlowSmolStr>,
    skip_implicit_check: bool,
) {
    let InheritedSideState {
        locs: inherited_locs,
        owners: inherited_owners,
        name_abstract,
    } = inherited_state;
    // No-extends check. If [super] has no real extends clause and there
    // are [override] members, emit [OverrideWithoutExtends] for each and
    // skip the other checks for this side (they assume a real super).
    if !super_has_real_extends(cx, super_) && !override_members.is_empty() {
        for (member_name, loc) in override_members.iter() {
            if member_name.matches_str("constructor") {
                continue;
            }
            flow_js_utils::add_output_non_speculating(
                cx,
                ErrorMessage::EOverride(Box::new(EOverrideData {
                    kind: OverrideErrorKind::OverrideWithoutExtends {
                        class_name: class_name.clone(),
                        member_name: member_name.display_smol_str(),
                    },
                    loc: loc.dupe(),
                })),
            );
        }
    } else {
        // Not-inherited check. Every [override] name must exist somewhere
        // in the inherited set.
        for (member_name, loc) in override_members.iter() {
            if member_name.matches_str("constructor") {
                continue;
            }
            if !inherited_locs.contains_key(member_name) {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EOverride(Box::new(EOverrideData {
                        kind: OverrideErrorKind::OverrideOfNonInheritedMember {
                            class_name: class_name.clone(),
                            base_class_name: immediate_super_name.clone(),
                            member_name: member_name.display_smol_str(),
                        },
                        loc: loc.dupe(),
                    })),
                );
            }
        }
        // Implicit-override check (NIO). Every concrete own same-side member
        // that shadows an inherited member must carry the [override]
        // modifier — unless the shallowest inherited decl is abstract from
        // x's perspective.
        if !skip_implicit_check {
            for (member_name, own_loc_opt) in own_concrete_names.iter() {
                if member_name.matches_str("constructor") {
                    continue;
                }
                if override_members.contains_key(member_name) {
                    continue;
                }
                match inherited_locs.get(member_name) {
                    None => {}
                    Some(inherited_def_loc) => {
                        let is_abstract = name_abstract.get(member_name).copied().unwrap_or(false);
                        if is_abstract {
                            continue;
                        }
                        // The implicit-override check fires at the subclass
                        // member that should have had [override]. If
                        // [own_loc_opt] is [None] the member was auto-injected
                        // by the class machinery (e.g. the synthetic static
                        // [name] field), not user-authored — there's nothing
                        // for the user to mark [override] on, so skip emission
                        // entirely. (A previous both-locs-have-no-source
                        // filter mis-fired when the inherited side had a real
                        // loc, e.g. [class MyErr extends Error] under NIO.)
                        match own_loc_opt {
                            None => {}
                            Some(loc) => {
                                // [inherited_owners] maps each name to an `Option` (the
                                // owner class's name, if it has one). [get] wraps that
                                // in another `Option`, so flatten it.
                                let base_class_name =
                                    inherited_owners.get(member_name).cloned().flatten();
                                flow_js_utils::add_output_non_speculating(
                                    cx,
                                    ErrorMessage::EOverride(Box::new(EOverrideData {
                                        kind: OverrideErrorKind::ImplicitOverrideMissingModifier {
                                            class_name: class_name.clone(),
                                            base_class_name,
                                            member_name: member_name.display_smol_str(),
                                            inherited_def_loc: inherited_def_loc.dupe(),
                                        },
                                        loc: loc.dupe(),
                                    })),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

// Verify [override] members and (under [no_implicit_override]) implicit
// overrides. Mirrors [check_abstract_obligations]'s shape: gated on
// [tslib_syntax], registered as a post-inference callback so super
// resolution has completed, and walks both instance AND static sides
// (unlike abstract, which is instance-only).
//
// Skipped entirely for structural sigs ([interface], inline
// [interface { ... }], inline construct sigs) — those forms have no
// [override] modifier syntax (rejected at type-annotation time), so
// [override_members] is always empty AND there's no way for a user to
// satisfy the implicit-override check.
fn check_override_obligations<'cx, C: crate::func_params_intf::Config>(
    cx: &mut Context<'cx>,
    _def_reason: Reason,
    x: &class_types::Class<C>,
) {
    if !cx.tslib_syntax() || structural(x) {
        return;
    }
    let instance_override = x.instance.override_members.clone();
    let static_override = x.static_.override_members.clone();
    // The implicit-override check is skipped if [no_implicit_override]
    // is off, OR if the class is ambient. Ambient has two prongs:
    // 1. file/scope ambient via [under_declaration_context];
    // 2. in-file [declare class] via the [is_declare] flag set in
    //    [mk_declare_class_sig] in type_annotation.rs.
    let skip_implicit_check =
        !cx.no_implicit_override() || cx.under_declaration_context() || x.is_declare;
    // Short-circuit only when there's nothing for any check to do: no
    // [override] modifiers anywhere AND the implicit-override check
    // is skipped.
    if instance_override.is_empty() && static_override.is_empty() && skip_implicit_check {
        return;
    }
    let class_name = x.class_name.clone();
    // The implicit-override check is the only consumer of
    // [own_concrete_names_*]; building them when
    // [skip_implicit_check] is set is pure waste (two map walks
    // per class allocating a fresh map that no one reads). Build
    // only when the implicit-override check will run.
    // [add_fields]/[add_funcs]/[add_methods] each insert a
    // `Name` -> `Option<ALoc>` entry; the loc is used as the primary
    // loc for the [ImplicitOverrideMissingModifier] error. Abstract
    // members are subtracted because the implicit-override check is
    // about CONCRETE overrides — an [abstract foo()] shadowing a
    // concrete [foo()] is the abstract-obligation check's domain,
    // not this one.
    let own_concrete_names_of = |s: &class_types::Signature<C>| -> BTreeMap<Name, Option<ALoc>> {
        if skip_implicit_check {
            BTreeMap::new()
        } else {
            // Keyed by `Name`, so a `unique symbol` member is compared
            // against the inherited set by its nominal identity rather than
            // a shared string spelling.
            let add_fields = |m: &BTreeMap<Name, class_types::FieldPrime<C>>,
                              acc: &mut BTreeMap<Name, Option<ALoc>>| {
                for (k, (loc, _, _)) in m.iter() {
                    acc.insert(k.dupe(), loc.clone());
                }
            };
            let add_funcs = |m: &BTreeMap<Name, class_types::FuncInfo<C>>,
                             acc: &mut BTreeMap<Name, Option<ALoc>>| {
                for (k, info) in m.iter() {
                    acc.insert(k.dupe(), info.id_loc.clone());
                }
            };
            let add_methods =
                |m: &BTreeMap<Name, vec1::Vec1<class_types::FuncInfo<C>>>,
                 acc: &mut BTreeMap<Name, Option<ALoc>>| {
                    // [methods] maps each name to a non-empty list of
                    // [FuncInfo]; take the first one's id_loc.
                    for (k, infos) in m.iter() {
                        let info = infos.first();
                        acc.insert(k.dupe(), info.id_loc.clone());
                    }
                };
            let mut acc: BTreeMap<Name, Option<ALoc>> = BTreeMap::new();
            add_fields(&s.fields, &mut acc);
            add_fields(&s.proto_fields, &mut acc);
            add_methods(&s.methods, &mut acc);
            add_funcs(&s.getters, &mut acc);
            add_funcs(&s.setters, &mut acc);
            acc.into_iter()
                .filter(|(k, _)| !s.abstract_members.contains(k))
                .collect()
        }
    };
    let own_concrete_instance = own_concrete_names_of(&x.instance);
    let own_concrete_static = own_concrete_names_of(&x.static_);
    let (super_, _static_proto) = supertype(cx, x);
    cx.add_post_inference_validation_callback(Box::new(move |cx: &mut Context<'_>| {
        // One super-chain walk feeds both sides — each ancestor's
        // static slot lives on the same [InstanceT] we're already
        // visiting. Halves the [possible_concrete_types_for_inspection]
        // work versus the prior two-walk shape.
        let (instance_inherited, static_inherited, immediate_super_name) =
            collect_inherited_members_both_sides(cx, &super_)?;
        check_override_side(
            cx,
            &class_name,
            &own_concrete_instance,
            &instance_override,
            &super_,
            &instance_inherited,
            &immediate_super_name,
            skip_implicit_check,
        );
        check_override_side(
            cx,
            &class_name,
            &own_concrete_static,
            &static_override,
            &super_,
            &static_inherited,
            &immediate_super_name,
            skip_implicit_check,
        );
        Ok(())
    }));
}

pub fn check_signature_compatibility<'a, C: crate::func_params_intf::Config>(
    cx: &mut Context<'a>,
    def_reason: Reason,
    x: &class_types::Class<C>,
) {
    let self_ = match &x.super_ {
        class_types::Super::Interface(_) => thistype(cx, x),
        class_types::Super::Class(class_super) => class_super.this_t.dupe(),
    };
    check_super(cx, def_reason.dupe(), x, &self_);
    check_implements(cx, def_reason.dupe(), x);
    check_methods(cx, def_reason.dupe(), x, &self_);
    check_abstract_obligations(cx, def_reason.dupe(), x);
    check_override_obligations(cx, def_reason, x);
}

fn merge_tparam_polarities(cx: &mut Context, name_loc: &ALoc, tparams: &TypeParams) -> TypeParams {
    let Some((loc, tparams)) = tparams else {
        return None;
    };
    let var_info = cx.environment().var_info.dupe();
    let name_defs = cx.environment().name_defs.dupe();
    let conflicts = var_info
        .interface_merge_conflicts
        .iter()
        .chain(var_info.declare_class_interface_merge_conflicts.iter())
        .find(|(canonical_loc, other_locs)| {
            *canonical_loc == name_loc || other_locs.contains(name_loc)
        });
    let mut merged = tparams.clone();
    if let Some((canonical_loc, other_locs)) = conflicts {
        let mut first_polarities = vec![None; merged.len()];
        for decl_loc in std::iter::once(canonical_loc).chain(other_locs) {
            let Some((def, _, _, _, _)) = name_defs.get(&EnvKey::ordinary(decl_loc.dupe())) else {
                continue;
            };
            let decl_tparams = match def {
                Def::Interface(_, decl) => decl.tparams.as_ref(),
                Def::DeclaredClass(decl) => decl.decl.tparams.as_ref(),
                _ => None,
            };
            if let Some(decl_tparams) = decl_tparams
                && decl_tparams.params.len() == merged.len()
            {
                for ((tparam, first_polarity), decl_tparam) in merged
                    .iter_mut()
                    .zip(&mut first_polarities)
                    .zip(decl_tparams.params.iter())
                {
                    let polarity = typed_ast_utils::polarity(decl_tparam.variance.as_ref());
                    if decl_tparam.variance.is_none() {
                        continue;
                    }
                    match first_polarity {
                        None => {
                            *first_polarity = Some((
                                decl_tparam.name.loc.dupe(),
                                decl_tparam.name.name.dupe(),
                                polarity,
                            ));
                        }
                        Some((first_loc, first_name, first))
                            if name_loc == canonical_loc && *first != polarity =>
                        {
                            flow_js_utils::add_output_non_speculating(
                                cx,
                                ErrorMessage::EPolarityMismatch(Box::new(EPolarityMismatchData {
                                    loc: decl_tparam.name.loc.dupe(),
                                    type_param_loc: first_loc.dupe(),
                                    name: first_name.dupe(),
                                    expected_polarity: *first,
                                    actual_polarity: polarity,
                                })),
                            );
                        }
                        Some(_) => {}
                    }
                    if tparam.polarity == Polarity::Neutral {
                        let mut merged_tparam = (**tparam).clone();
                        merged_tparam.polarity = polarity;
                        *tparam = TypeParam::new(merged_tparam);
                    }
                }
            }
        }
    }
    Some((loc.dupe(), merged))
}

// TODO: Ideally we should check polarity for all class types, but this flag is
// flipped off for interface/declare class currently.
pub fn classtype<'a, C: crate::func_params_intf::Config>(
    cx: &mut Context<'a>,
    check_polarity_flag: bool,
    inst_kind: InstanceKind,
    x: &class_types::Class<C>,
) -> (
    Type,
    Type,
    (
        flow_typing_type::type_::properties::Id,
        flow_typing_type::type_::properties::Id,
    ),
) {
    let merged_tparams = merge_tparam_polarities(cx, x.instance.reason.loc(), &x.tparams);
    let (this_reason, mut this_instance_t) = this_instance_type(cx, Some(inst_kind), x);
    if merged_tparams != x.tparams
        && let Some((_, tparams)) = &merged_tparams
    {
        let mut instance = (*this_instance_t).clone();
        let mut inst = (*this_instance_t.inst).clone();
        let type_args = Rc::make_mut(&mut inst.type_args);
        for ((_, _, _, polarity), tparam) in type_args.iter_mut().zip(tparams) {
            *polarity = tparam.polarity;
        }
        instance.inst = InstType::new(inst);
        this_instance_t = InstanceT::new(instance);
    }
    let inst_prop_ids = (
        this_instance_t.inst.own_props.dupe(),
        this_instance_t.inst.proto_props.dupe(),
    );
    let this = Type::new(TypeInner::DefT(
        this_reason.dupe(),
        DefT::new(DefTInner::InstanceT(Rc::new(this_instance_t.dupe()))),
    ));
    let this_tp = this_tparam(x);
    let tparams_with_this_val: TypeParams = match this_tp {
        Some(tp) => tparams_with_this(&x.tparams, tp.dupe()),
        None => x.tparams.clone(),
    };
    let this_name = SubstName::name(FlowSmolStr::new("this"));
    match &tparams_with_this_val {
        Some((_, tps)) if check_polarity_flag => {
            // TODO: use tparams_map instead of calculating this here
            let mut tparams_map = BTreeMap::new();
            for tp in tps {
                tparams_map.insert(tp.name.dupe(), tp.dupe());
            }
            // Check delayed so that we don't have to force the currently unresolved OpenT
            // with implicit this tparam.
            cx.add_post_inference_polarity_check(tparams_map, Polarity::Positive, this.dupe());
        }
        _ => {}
    }
    let (t_inner, t_outer) = if structural(x) {
        match this_tp {
            Some(_) => {
                let t_inner = Type::new(TypeInner::ThisInstanceT(Box::new(ThisInstanceTData {
                    reason: this_reason.dupe(),
                    instance: this_instance_t.dupe(),
                    is_this: true,
                    subst_name: this_name.dupe(),
                })));
                let t_outer = type_util::class_type(
                    Type::new(TypeInner::ThisInstanceT(Box::new(ThisInstanceTData {
                        reason: this_reason.dupe(),
                        instance: this_instance_t.dupe(),
                        is_this: false,
                        subst_name: this_name.dupe(),
                    }))),
                    None,
                );
                (t_inner, t_outer)
            }
            None => (this.dupe(), type_util::class_type(this, None)),
        }
    } else {
        let t_inner = Type::new(TypeInner::ThisInstanceT(Box::new(ThisInstanceTData {
            reason: this_reason.dupe(),
            instance: this_instance_t.dupe(),
            is_this: true,
            subst_name: this_name.dupe(),
        })));
        let t_outer = type_util::class_type(
            Type::new(TypeInner::ThisInstanceT(Box::new(ThisInstanceTData {
                reason: this_reason,
                instance: this_instance_t,
                is_this: false,
                subst_name: this_name,
            }))),
            None,
        );
        (t_inner, t_outer)
    };
    let poly = |t: Type| -> Type {
        type_util::poly_type_of_tparams(
            poly::Id::generate_id(),
            merged_tparams.clone(),
            t,
            cx.type_strictness_kind(),
            PolyTKind::Regular,
        )
    };
    (t_inner, poly(t_outer), inst_prop_ids)
}

pub fn mk_class_binding<'a, C: ConfigTypes>(
    _cx: &mut Context<'a>,
    x: &class_types::Class<C>,
) -> ClassBinding {
    ClassBinding {
        class_binding_id: x.id.dupe(),
    }
}

pub fn make_thises<'a, C: ConfigTypes>(
    cx: &mut Context<'a>,
    x: &class_types::Class<C>,
) -> (Type, Type, Type, Type) {
    let super_reason = x.instance.reason.dupe().replace_desc(None);
    match &x.super_ {
        class_types::Super::Interface(_) => {
            panic!("tried to evaluate toplevel of interface")
        }
        class_types::Super::Class(class_super) => {
            let (super_, static_super) = match &class_super.extends {
                class_types::Extends::Explicit((annot_loc, c, targs)) => {
                    // Eagerly specialize when there are no targs
                    // TODO: We can also specialize when there are targs, because this
                    // code is not instantiated. However, the type normalizer
                    // expects a PolyT here.
                    let c = if targs.is_none() {
                        let use_op = UseOp::Op(Arc::new(RootUseOp::ClassExtendsCheck(Box::new(
                            ClassExtendsCheckData {
                                def_loc: x.instance.reason.loc.dupe(),
                                def: TypeOrTypeDescT::Type(class_super.this_tparam.bound.dupe()),
                                extends_loc: type_util::loc_of_t(c).dupe(),
                                extends: TypeOrTypeDescT::Type(c.dupe()),
                            },
                        ))));
                        specialize(cx, use_op, targs.clone(), c.dupe())
                    } else {
                        c.dupe()
                    };
                    let t = type_util::this_typeapp(
                        c,
                        class_super.this_t.dupe(),
                        targs.clone(),
                        Some(annot_loc.dupe()),
                    );
                    (t.dupe(), type_util::class_type(t, Some(annot_loc.dupe())))
                }
                class_types::Extends::Implicit { null } => {
                    let t = if *null {
                        Type::new(TypeInner::NullProtoT(super_reason.dupe()))
                    } else {
                        Type::new(TypeInner::ObjProtoT(super_reason.dupe()))
                    };
                    let static_super = Type::new(TypeInner::FunProtoT(super_reason));
                    (t, static_super)
                }
            };
            (
                class_super.this_t.dupe(),
                type_util::class_type(class_super.this_t.dupe(), None),
                super_,
                static_super,
            )
        }
    }
}

/// Processes the bodies of instance and static class members.
pub fn toplevels<'a, C: crate::func_params_intf::Config>(
    cx: &mut Context<'a>,
    x: &class_types::Class<C>,
) -> Result<(), CheckExprError> {
    type_env::in_class_scope(cx, x.class_loc.dupe(), |cx| {
        let method_ = |cx: &mut Context<'a>,
                       set_asts: &class_types::SetAsts<C>,
                       f: &func_class_sig_types::func::Func<C>| {
            match (&f.body, &f.kind) {
                (None, func_class_sig_types::func::Kind::FieldInit { .. }) | (Some(_), _) => {
                    let (params_ast, body_ast, init_ast) = func_sig::toplevels(cx, f)?;
                    if let Some(ref init) = init_ast {
                        let (_, t) = init.loc();
                        cx.add_missing_local_annot_lower_bound(f.ret_annot_loc.dupe(), t.dupe());
                    }
                    (set_asts)((params_ast, body_ast, init_ast));
                }
                (None, _) => (set_asts)((None, None, None)),
            }
            Ok(())
        };
        let field = |cx: &mut Context<'a>,
                     entry: &class_types::FieldPrime<C>|
         -> Result<(), CheckExprError> {
            let (_, _, value) = entry;
            match value {
                class_types::Field::Annot(_) => {}
                class_types::Field::Infer(fsig, set_asts) => {
                    method_(cx, set_asts, fsig)?;
                }
            }
            Ok(())
        };
        with_sig(
            true,
            |s| -> Result<(), CheckExprError> {
                // process static methods and fields
                iter_methods(
                    &mut |fi: &class_types::FuncInfo<C>| method_(cx, &fi.set_asts, &fi.func_sig),
                    s,
                )?;
                for entry in s.fields.values() {
                    field(cx, entry)?;
                }
                for entry in s.private_fields.values() {
                    field(cx, entry)?;
                }
                Ok(())
            },
            x,
        )?;
        with_sig(
            false,
            |s| -> Result<(), CheckExprError> {
                // process constructor
                for fi in &x.constructor {
                    method_(cx, &fi.set_asts, &fi.func_sig)?;
                }
                // process instance methods and fields
                iter_methods(
                    &mut |fi: &class_types::FuncInfo<C>| method_(cx, &fi.set_asts, &fi.func_sig),
                    s,
                )?;
                for entry in s.fields.values() {
                    field(cx, entry)?;
                }
                for entry in s.private_fields.values() {
                    field(cx, entry)?;
                }
                for entry in s.proto_fields.values() {
                    field(cx, entry)?;
                }
                Ok(())
            },
            x,
        )?;
        Ok(())
    })
}
