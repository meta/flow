/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ops::Deref;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::expression;
use flow_parser::ast::expression::object;
use flow_parser::ast::statement;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::loc::Loc;
use flow_typing_context::Context;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_utils::members;
use flow_typing_utils::members::GenericT;
use vec1::Vec1;

use crate::get_def_types::DefInfo;
use crate::get_def_types::PropertyDefInfo;
use crate::get_def_types::SinglePropertyDefInfo;

#[derive(Debug, Clone)]
pub enum DefKind<Loc> {
    /// Use of a property, e.g. `foo.bar`. Includes type of receiver (`foo`) and name of the
    /// property `bar`
    Use(Type, /* name */ FlowSmolStr),
    /// In a class, where a property/method is defined. Includes the type of the class and the name
    /// of the property.
    ClassDef(Type, /* name */ FlowSmolStr, bool),
    /// In an object type. Includes the location of the property definition and its name.
    ObjDef(Loc, /* name */ FlowSmolStr),
    /// In an object literal. Includes the location of the property definition, the object type,
    /// and the prop name.
    ObjLiteral(
        Loc,
        /* type of object */ Type,
        /* name */ FlowSmolStr,
    ),
    PrivateName {
        def_loc: Loc,
        references: Vec<Loc>,
        name: FlowSmolStr,
    },
}

pub fn map_def_kind_loc<A: Clone, B>(f: impl Fn(A) -> B, dk: DefKind<A>) -> DefKind<B> {
    match dk {
        DefKind::Use(t, name) => DefKind::Use(t, name),
        DefKind::ClassDef(t, name, static_) => DefKind::ClassDef(t, name, static_),
        DefKind::ObjDef(loc, name) => DefKind::ObjDef(f(loc), name),
        DefKind::ObjLiteral(loc, t, name) => DefKind::ObjLiteral(f(loc), t, name),
        DefKind::PrivateName {
            def_loc,
            references,
            name,
        } => DefKind::PrivateName {
            def_loc: f(def_loc),
            references: references.into_iter().map(&f).collect(),
            name,
        },
    }
}

mod def_kind_search {
    use flow_data_structure_wrapper::ord_map::FlowOrdMap;

    use super::*;

    struct FoundClassDef {
        name: FlowSmolStr,
        static_: bool,
    }

    struct FoundObjProp {
        name: FlowSmolStr,
    }

    struct FoundImport {
        name: FlowSmolStr,
    }

    #[derive(Clone)]
    struct AvailablePrivateName {
        def_loc: ALoc,
        inner: std::rc::Rc<std::cell::RefCell<AvailablePrivateNameInner>>,
    }

    #[derive(Clone)]
    struct AvailablePrivateNameInner {
        references: Vec<ALoc>,
        has_covered_target_reference: bool,
    }

    enum SearchException {
        Found(DefKind<ALoc>),
        FoundClassDef(FoundClassDef),
        FoundObjProp(FoundObjProp),
        FoundImport(FoundImport),
    }

    struct Searcher<F: Fn(&ALoc) -> bool> {
        covers_target: F,
        available_private_names: FlowOrdMap<FlowSmolStr, AvailablePrivateName>,
    }

    impl<F: Fn(&ALoc) -> bool> Searcher<F> {
        fn new(covers_target: F) -> Self {
            Searcher {
                covers_target,
                available_private_names: FlowOrdMap::new(),
            }
        }

        fn visit_class_key(
            &self,
            static_: bool,
            key: &object::Key<ALoc, (ALoc, Type)>,
        ) -> Result<(), SearchException> {
            match key {
                object::Key::Identifier(id) => {
                    let (loc, _) = &id.loc;
                    if (self.covers_target)(loc) {
                        return Err(SearchException::FoundClassDef(FoundClassDef {
                            name: id.name.dupe(),
                            static_,
                        }));
                    }
                }
                _ => {}
            }
            Ok(())
        }
    }

    impl<'ast, F: Fn(&ALoc) -> bool>
        AstVisitor<'ast, ALoc, (ALoc, Type), &'ast ALoc, SearchException> for Searcher<F>
    {
        fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
            loc
        }

        fn normalize_type(type_: &'ast (ALoc, Type)) -> &'ast ALoc {
            &type_.0
        }

        fn expression(
            &mut self,
            x: &'ast expression::Expression<ALoc, (ALoc, Type)>,
        ) -> Result<(), SearchException> {
            let (l, t) = &x.loc();
            match x.deref() {
                expression::ExpressionInner::Class { .. } => {
                    match ast_visitor::expression_default(self, x) {
                        Ok(()) => Ok(()),
                        Err(SearchException::FoundClassDef(FoundClassDef { name, static_ })) => {
                            Err(SearchException::Found(DefKind::ClassDef(
                                t.dupe(),
                                name,
                                static_,
                            )))
                        }
                        Err(e) => Err(e),
                    }
                }
                expression::ExpressionInner::Object { .. } => {
                    match ast_visitor::expression_default(self, x) {
                        Ok(()) => Ok(()),
                        Err(SearchException::FoundObjProp(FoundObjProp { name })) => Err(
                            SearchException::Found(DefKind::ObjLiteral(l.dupe(), t.dupe(), name)),
                        ),
                        Err(e) => Err(e),
                    }
                }
                _ => ast_visitor::expression_default(self, x),
            }
        }

        fn class_declaration(
            &mut self,
            loc: &'ast ALoc,
            cls: &'ast ast::class::Class<ALoc, (ALoc, Type)>,
        ) -> Result<(), SearchException> {
            let id = &cls.id;
            let class_type = match id {
                Some(id) => {
                    let (_, annot) = &id.loc;
                    annot.dupe()
                }
                None => panic!("Class declaration must have an id"),
            };
            match ast_visitor::class_declaration_default(self, loc, cls) {
                Ok(()) => Ok(()),
                Err(SearchException::FoundClassDef(FoundClassDef { name, static_ })) => Err(
                    SearchException::Found(DefKind::ClassDef(class_type, name, static_)),
                ),
                Err(e) => Err(e),
            }
        }

        fn class_body(
            &mut self,
            body: &'ast ast::class::Body<ALoc, (ALoc, Type)>,
        ) -> Result<(), SearchException> {
            fn add_private_name(
                all: &mut FlowOrdMap<FlowSmolStr, AvailablePrivateName>,
                new_names: &mut Vec<FlowSmolStr>,
                name: &ast::PrivateName<ALoc>,
            ) {
                let entry = AvailablePrivateName {
                    def_loc: name.loc.dupe(),
                    inner: std::rc::Rc::new(std::cell::RefCell::new(AvailablePrivateNameInner {
                        references: Vec::new(),
                        has_covered_target_reference: false,
                    })),
                };
                all.insert(name.name.dupe(), entry);
                new_names.push(name.name.dupe());
            }

            let mut all_available_private_names = self.available_private_names.dupe();
            let mut new_names: Vec<FlowSmolStr> = Vec::new();

            for member in body.body.iter() {
                match member {
                    ast::class::BodyElement::Method(m) => {
                        if let object::Key::PrivateName(name) = &m.key {
                            add_private_name(
                                &mut all_available_private_names,
                                &mut new_names,
                                name,
                            );
                        }
                    }
                    ast::class::BodyElement::PrivateField(pf) => {
                        add_private_name(&mut all_available_private_names, &mut new_names, &pf.key);
                    }
                    ast::class::BodyElement::Property(_) => {}
                    ast::class::BodyElement::StaticBlock(_) => {}
                    ast::class::BodyElement::DeclareMethod(_) => {}
                    ast::class::BodyElement::AbstractMethod(_) => {}
                    ast::class::BodyElement::AbstractProperty(_) => {}
                    ast::class::BodyElement::IndexSignature(_) => {}
                }
            }

            let saved_available_private_names = std::mem::replace(
                &mut self.available_private_names,
                all_available_private_names,
            );
            let result = (|| -> Result<(), SearchException> {
                ast_visitor::class_body_default(self, body)?;
                for name in &new_names {
                    if let Some(entry) = self.available_private_names.get(name) {
                        let inner = entry.inner.borrow();
                        if inner.has_covered_target_reference {
                            return Err(SearchException::Found(DefKind::PrivateName {
                                def_loc: entry.def_loc.dupe(),
                                references: inner.references.clone(),
                                name: name.dupe(),
                            }));
                        }
                    }
                }
                Ok(())
            })();
            self.available_private_names = saved_available_private_names;
            result
        }

        fn private_name(
            &mut self,
            pn: &'ast ast::PrivateName<ALoc>,
        ) -> Result<(), SearchException> {
            let loc = &pn.loc;
            let name = &pn.name;
            match self.available_private_names.get(name) {
                Some(entry) => {
                    let mut inner = entry.inner.borrow_mut();
                    inner.has_covered_target_reference =
                        inner.has_covered_target_reference || (self.covers_target)(loc);
                    if *loc != entry.def_loc {
                        inner.references.push(loc.dupe());
                    }
                }
                None => {}
            }
            Ok(())
        }

        fn class_method(
            &mut self,
            prop: &'ast ast::class::Method<ALoc, (ALoc, Type)>,
        ) -> Result<(), SearchException> {
            let key = &prop.key;
            let static_ = prop.static_;
            self.visit_class_key(static_, key)?;
            ast_visitor::class_method_default(self, prop)
        }

        fn class_property(
            &mut self,
            prop: &'ast ast::class::Property<ALoc, (ALoc, Type)>,
        ) -> Result<(), SearchException> {
            let key = &prop.key;
            let static_ = prop.static_;
            self.visit_class_key(static_, key)?;
            ast_visitor::class_property_default(self, prop)
        }

        fn import_declaration(
            &mut self,
            loc: &'ast ALoc,
            decl: &'ast ast::statement::ImportDeclaration<ALoc, (ALoc, Type)>,
        ) -> Result<(), SearchException> {
            match ast_visitor::import_declaration_default(self, loc, decl) {
                Ok(()) => Ok(()),
                Err(SearchException::FoundImport(FoundImport { name })) => {
                    let (_, module_t) = &decl.source.0;
                    Err(SearchException::Found(DefKind::Use(module_t.dupe(), name)))
                }
                Err(e) => Err(e),
            }
        }

        fn import_default_specifier(
            &mut self,
            _import_kind: &'ast ast::statement::ImportKind,
            id: &'ast ast::Identifier<ALoc, (ALoc, Type)>,
        ) -> Result<(), SearchException> {
            let (loc, _) = &id.loc;
            if (self.covers_target)(loc) {
                return Err(SearchException::FoundImport(FoundImport {
                    name: "default".into(),
                }));
            }
            Ok(())
        }

        fn import_named_specifier(
            &mut self,
            _import_kind: ast::statement::ImportKind,
            specifier: &'ast ast::statement::import_declaration::NamedSpecifier<ALoc, (ALoc, Type)>,
        ) -> Result<(), SearchException> {
            let id = specifier.local.as_ref().unwrap_or(&specifier.remote);
            let (loc, _) = &id.loc;
            let name = &id.name;
            if (self.covers_target)(loc) {
                return Err(SearchException::FoundImport(FoundImport {
                    name: name.dupe(),
                }));
            }
            Ok(())
        }

        fn export_default_declaration(
            &mut self,
            loc: &'ast ALoc,
            decl: &'ast ast::statement::ExportDefaultDeclaration<ALoc, (ALoc, Type)>,
        ) -> Result<(), SearchException> {
            let (default_loc, _) = &decl.default;
            if (self.covers_target)(default_loc) {
                return Err(SearchException::Found(DefKind::ObjDef(
                    default_loc.dupe(),
                    "default".into(),
                )));
            }
            ast_visitor::export_default_declaration_default(self, loc, decl)
        }

        fn export_named_declaration(
            &mut self,
            loc: &'ast ALoc,
            decl: &'ast ast::statement::ExportNamedDeclaration<ALoc, (ALoc, Type)>,
        ) -> Result<(), SearchException> {
            if let Some(declaration) = &decl.declaration {
                match declaration.deref() {
                    statement::StatementInner::FunctionDeclaration { inner, .. } => {
                        if let Some(id) = &inner.id {
                            let (id_loc, _) = &id.loc;
                            if (self.covers_target)(id_loc) {
                                return Err(SearchException::Found(DefKind::ObjDef(
                                    id_loc.dupe(),
                                    id.name.dupe(),
                                )));
                            }
                        }
                    }
                    statement::StatementInner::ClassDeclaration { inner, .. } => {
                        if let Some(id) = &inner.id {
                            let (id_loc, _) = &id.loc;
                            if (self.covers_target)(id_loc) {
                                return Err(SearchException::Found(DefKind::ObjDef(
                                    id_loc.dupe(),
                                    id.name.dupe(),
                                )));
                            }
                        }
                    }
                    statement::StatementInner::TypeAlias { inner, .. } => {
                        let (id_loc, _) = &inner.id.loc;
                        if (self.covers_target)(id_loc) {
                            return Err(SearchException::Found(DefKind::ObjDef(
                                id_loc.dupe(),
                                inner.id.name.dupe(),
                            )));
                        }
                    }
                    statement::StatementInner::OpaqueType { inner, .. } => {
                        let (id_loc, _) = &inner.id.loc;
                        if (self.covers_target)(id_loc) {
                            return Err(SearchException::Found(DefKind::ObjDef(
                                id_loc.dupe(),
                                inner.id.name.dupe(),
                            )));
                        }
                    }
                    statement::StatementInner::InterfaceDeclaration { inner, .. } => {
                        let (id_loc, _) = &inner.id.loc;
                        if (self.covers_target)(id_loc) {
                            return Err(SearchException::Found(DefKind::ObjDef(
                                id_loc.dupe(),
                                inner.id.name.dupe(),
                            )));
                        }
                    }
                    statement::StatementInner::EnumDeclaration { inner, .. } => {
                        let (id_loc, _) = &inner.id.loc;
                        if (self.covers_target)(id_loc) {
                            return Err(SearchException::Found(DefKind::ObjDef(
                                id_loc.dupe(),
                                inner.id.name.dupe(),
                            )));
                        }
                    }
                    statement::StatementInner::VariableDeclaration { inner, .. } => {
                        flow_parser::ast_utils::fold_bindings_of_variable_declarations(
                            Ok(()),
                            &inner.declarations,
                            &mut |_has_annot, acc: Result<(), SearchException>, id| {
                                acc?;
                                let (id_loc, _) = &id.loc;
                                let name = &id.name;
                                if (self.covers_target)(id_loc) {
                                    return Err(SearchException::Found(DefKind::ObjDef(
                                        id_loc.dupe(),
                                        name.dupe(),
                                    )));
                                }
                                Ok(())
                            },
                        )?;
                    }
                    _ => {}
                }
            }
            ast_visitor::export_named_declaration_default(self, loc, decl)
        }

        fn member(
            &mut self,
            loc: &'ast (ALoc, Type),
            expr: &'ast expression::Member<ALoc, (ALoc, Type)>,
        ) -> Result<(), SearchException> {
            let _object = &expr.object;
            let property = &expr.property;
            match property {
                expression::member::Property::PropertyIdentifier(id) => {
                    let (id_loc, _) = &id.loc;
                    let name = &id.name;
                    if (self.covers_target)(id_loc) {
                        let (_, obj_t) = &_object.loc();
                        return Err(SearchException::Found(DefKind::Use(
                            obj_t.dupe(),
                            name.dupe(),
                        )));
                    }
                }
                expression::member::Property::PropertyPrivateName(_)
                | expression::member::Property::PropertyExpression(_) => {}
            }
            ast_visitor::member_default(self, loc, expr)
        }

        fn match_member_pattern(
            &mut self,
            member_pattern: &'ast ast::match_pattern::MemberPattern<ALoc, (ALoc, Type)>,
        ) -> Result<(), SearchException> {
            match &member_pattern.property {
                ast::match_pattern::member_pattern::Property::PropertyIdentifier(id) => {
                    let (id_loc, _) = &id.loc;
                    let name = &id.name;
                    if (self.covers_target)(id_loc) {
                        let base_t = match &member_pattern.base {
                            ast::match_pattern::member_pattern::Base::BaseIdentifier(id) => {
                                let (_, t) = &id.loc;
                                t.dupe()
                            }
                            ast::match_pattern::member_pattern::Base::BaseMember(mem) => {
                                let (_, t) = &mem.loc;
                                t.dupe()
                            }
                        };
                        return Err(SearchException::Found(DefKind::Use(base_t, name.dupe())));
                    }
                }
                _ => {}
            }
            ast_visitor::match_member_pattern_default(self, member_pattern)
        }

        fn object_property(
            &mut self,
            prop: &'ast object::NormalProperty<ALoc, (ALoc, Type)>,
        ) -> Result<(), SearchException> {
            let key = match prop {
                object::NormalProperty::Init { key, .. } => key,
                object::NormalProperty::Method { key, .. } => key,
                object::NormalProperty::Get { key, .. } => key,
                object::NormalProperty::Set { key, .. } => key,
            };
            match key {
                object::Key::Identifier(id) => {
                    let (id_loc, _) = &id.loc;
                    if (self.covers_target)(id_loc) {
                        return Err(SearchException::FoundObjProp(FoundObjProp {
                            name: id.name.dupe(),
                        }));
                    }
                }
                _ => {}
            }
            ast_visitor::object_property_default(self, prop)
        }

        fn object_property_type(
            &mut self,
            prop: &'ast ast::types::object::NormalProperty<ALoc, (ALoc, Type)>,
        ) -> Result<(), SearchException> {
            match &prop.key {
                object::Key::Identifier(id) => {
                    let (loc, _) = &id.loc;
                    if (self.covers_target)(loc) {
                        return Err(SearchException::Found(DefKind::ObjDef(
                            loc.dupe(),
                            id.name.dupe(),
                        )));
                    }
                }
                _ => {}
            }
            ast_visitor::object_property_type_default(self, prop)
        }

        fn pattern(
            &mut self,
            kind: Option<ast::VariableKind>,
            expr: &'ast ast::pattern::Pattern<ALoc, (ALoc, Type)>,
        ) -> Result<(), SearchException> {
            let (_, ty) = expr.loc();
            match expr {
                ast::pattern::Pattern::Object { inner: obj, .. } => {
                    for prop in obj.properties.iter() {
                        if let ast::pattern::object::Property::NormalProperty(p) = prop {
                            if let ast::pattern::object::Key::Identifier(id) = &p.key {
                                let (loc, _) = &id.loc;
                                if (self.covers_target)(loc) {
                                    return Err(SearchException::Found(DefKind::Use(
                                        ty.dupe(),
                                        id.name.dupe(),
                                    )));
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            ast_visitor::pattern_default(self, kind, expr)
        }
    }

    pub fn search<'ast>(
        f: impl Fn(&ALoc) -> bool,
        ast: &'ast ast::Program<ALoc, (ALoc, Type)>,
    ) -> Option<DefKind<ALoc>> {
        let mut s = Searcher::new(f);
        match s.program(ast) {
            Ok(()) => None,
            Err(SearchException::Found(def_kind)) => Some(def_kind),
            Err(_) => unreachable!("internal search exceptions should be caught by their handlers"),
        }
    }
}

pub fn loc_of_single_def_info(info: &SinglePropertyDefInfo) -> &Loc {
    match info {
        SinglePropertyDefInfo::ClassProperty(loc) => loc,
        SinglePropertyDefInfo::ObjectProperty(loc) => loc,
    }
}

pub fn all_locs_of_ordinary_property_def_info(
    props_info: &Vec1<SinglePropertyDefInfo>,
) -> Vec1<Loc> {
    props_info.mapped_ref(|info| loc_of_single_def_info(info).clone())
}

pub fn all_locs_of_def_info(info: &DefInfo) -> Vec<Loc> {
    match info {
        DefInfo::VariableDefinition(locs, _) => locs.clone(),
        DefInfo::PropertyDefinition(PropertyDefInfo::OrdinaryProperty {
            props_info,
            name: _,
        }) => {
            let nel = all_locs_of_ordinary_property_def_info(props_info);
            nel.into_iter().collect()
        }
        DefInfo::PropertyDefinition(PropertyDefInfo::PrivateNameProperty { def_loc, .. }) => {
            vec![def_loc.clone()]
        }
        DefInfo::NoDefinition(_) => Vec::new(),
    }
}

#[derive(Debug, Clone)]
pub enum DefLoc {
    /// We found a class property. Include all overridden implementations. Superclass implementations
    /// are listed last.
    FoundClass(Vec1<Loc>),
    /// We found an object property.
    FoundObject(Loc),
    FoundUnion(Vec1<DefLoc>),
    /// This means we resolved the receiver type but did not find the definition. If this happens
    /// there must be a type error (which may be suppresssed)
    NoDefFound,
    /// This means it's a known type that we deliberately do not currently support.
    UnsupportedType,
    /// This means it's not well-typed, and could be anything
    AnyType,
}

// Disable the unused value warning -- we want to keep this around for debugging
#[allow(dead_code)]
fn debug_string_of_locs(locs: &Vec1<Loc>) -> String {
    locs.iter()
        .map(|loc| format!("{:?}", loc))
        .collect::<Vec<_>>()
        .join(", ")
}

#[allow(dead_code)]
fn debug_string_of_single_def_info(info: &SinglePropertyDefInfo) -> String {
    match info {
        SinglePropertyDefInfo::ClassProperty(loc) => format!("ClassProperty ({:?})", loc),
        SinglePropertyDefInfo::ObjectProperty(loc) => format!("ObjectProperty ({:?})", loc),
    }
}

#[allow(dead_code)]
fn debug_string_of_property_def_info(def_info: &Vec1<SinglePropertyDefInfo>) -> String {
    let items: Vec<String> = def_info
        .iter()
        .map(debug_string_of_single_def_info)
        .collect();
    format!("[{}]", items.join(", "))
}

#[allow(dead_code)]
fn debug_string_of_def_info(def_info: &Vec1<SinglePropertyDefInfo>, name: &str) -> String {
    format!(
        "({}, {})",
        debug_string_of_property_def_info(def_info),
        name
    )
}

#[allow(dead_code)]
pub fn debug_string_of_def_loc(dl: &DefLoc) -> String {
    match dl {
        DefLoc::FoundClass(locs) => format!("FoundClass ({})", debug_string_of_locs(locs)),
        DefLoc::FoundObject(loc) => format!("FoundObject ({:?})", loc),
        DefLoc::FoundUnion(def_locs) => {
            let items: Vec<String> = def_locs.iter().map(debug_string_of_def_loc).collect();
            format!("FoundUnion ({})", items.join(", "))
        }
        DefLoc::NoDefFound => "NoDefFound".to_string(),
        DefLoc::UnsupportedType => "UnsupportedType".to_string(),
        DefLoc::AnyType => "AnyType".to_string(),
    }
}

// Re-enable the unused value warning
fn extract_instancet<'cx>(cx: &mut Context<'cx>, ty: &Type) -> Result<Type, String> {
    use flow_typing_type::type_::DefT;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::PolyTData;
    use flow_typing_type::type_::string_of_ctor;

    let resolved = members::resolve_type(cx, ty.dupe());
    if let TypeInner::DefT(_r, def) = resolved.deref() {
        let class_t = match def.deref() {
            DefTInner::ClassT(class_t) => Some(class_t),
            DefTInner::PolyT(box PolyTData { t_out, .. })
                if let TypeInner::DefT(_, inner_def) = t_out.deref()
                    && let DefTInner::ClassT(class_t) = inner_def.deref() =>
            {
                Some(class_t)
            }
            _ => None,
        };
        if let Some(class_t) = class_t
            && let TypeInner::ThisInstanceT(box ThisInstanceTData {
                reason: r2,
                instance: t,
                ..
            }) = class_t.deref()
        {
            return Ok(Type::new(TypeInner::DefT(
                r2.dupe(),
                DefT::new(DefTInner::InstanceT(t.dupe().into())),
            )));
        }
    }
    let type_string = string_of_ctor(&resolved);
    Err(format!(
        "Expected a class type to extract an instance type from, got {}",
        type_string
    ))
}

// Must be called with the result from Members.extract_type
fn get_def_locs_from_extracted_type<'cx>(
    cx: &mut Context<'cx>,
    extracted_type: GenericT<Type, Type>,
    name: &str,
) -> Result<Option<Vec1<ALoc>>, String> {
    let extracted = members::extract_members(false, cx, extracted_type);
    let map = members::to_command_result(extracted)?;
    let smol_name = FlowSmolStr::new(name);
    match map.get(&smol_name) {
        None => Ok(None),
        // Currently some types (e.g. spreads) do not contain locations for their properties. For now
        // we'll just treat them as if the properties do not exist, but once this is fixed this case
        // should be promoted to an error
        Some((None, _)) => Ok(None),
        Some((Some(locs), _)) => Ok(Some(locs.clone())),
    }
}

pub fn extract_def_loc<'cx>(
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    cx: &mut Context<'cx>,
    ty: &Type,
    name: &str,
) -> Result<DefLoc, String> {
    let resolved = members::resolve_type(cx, ty.dupe());
    extract_def_loc_resolved(loc_of_aloc, cx, &resolved, name)
}

// The same as get_def_loc_from_extracted_type except it recursively checks for overridden
// definitions of the member in superclasses and returns those as well
fn extract_def_locs_from_instancet<'cx>(
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    cx: &mut Context<'cx>,
    extracted_type: GenericT<Type, Type>,
    super_: &Type,
    name: &str,
) -> Result<DefLoc, String> {
    let current_class_def_locs = get_def_locs_from_extracted_type(cx, extracted_type, name)?;
    match current_class_def_locs {
        None => Ok(DefLoc::NoDefFound),
        Some(alocs) => {
            let locs: Vec1<Loc> = alocs.mapped_ref(loc_of_aloc);
            let map_found_class =
                |f: &dyn Fn(Loc) -> Vec1<Loc>, locs: &Vec1<Loc>| -> Result<DefLoc, String> {
                    if locs.len() == 1 {
                        Ok(DefLoc::FoundClass(f(locs.first().clone())))
                    } else {
                        Ok(DefLoc::FoundUnion(
                            locs.mapped_ref(|loc| DefLoc::FoundClass(f(loc.clone()))),
                        ))
                    }
                };
            let super_def_loc = extract_def_loc(loc_of_aloc, cx, super_, name)?;
            match super_def_loc {
                DefLoc::FoundClass(lst) => {
                    // Avoid duplicate entries. This can happen if a class does not override a method,
                    // so the definition points to the method definition in the parent class. Then we
                    // look at the parent class and find the same definition.
                    let add = |loc: Loc| -> Vec1<Loc> {
                        if *lst.first() == loc {
                            lst.clone()
                        } else {
                            let mut result = Vec1::new(loc);
                            result.extend(lst.iter().cloned());
                            result
                        }
                    };
                    map_found_class(&add, &locs)
                }
                DefLoc::FoundObject(_) => {
                    Err("A superclass should be a class, not an object".to_string())
                }
                DefLoc::FoundUnion(_) => {
                    Err("A superclass should be a class, not a union".to_string())
                }
                DefLoc::NoDefFound | DefLoc::UnsupportedType | DefLoc::AnyType => {
                    // If the superclass does not have a definition for this method, or it is for some reason
                    // not a class type, or we don't know its type, just return the location we already know
                    // about.
                    let one = |loc: Loc| -> Vec1<Loc> { Vec1::new(loc) };
                    map_found_class(&one, &locs)
                }
            }
        }
    }
}

fn extract_def_loc_resolved<'cx>(
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    cx: &mut Context<'cx>,
    ty: &Type,
    name: &str,
) -> Result<DefLoc, String> {
    use flow_typing_type::type_::DefTInner;
    #[allow(unused_imports)]
    use flow_typing_type::type_::union_rep;

    let extracted = members::extract_type(cx, ty.dupe());
    match &extracted {
        GenericT::Success(inner_t) => match inner_t.deref() {
            TypeInner::DefT(_, def) => match def.deref() {
                DefTInner::InstanceT(inst) => {
                    let super_ = &inst.super_;
                    let extracted_dupe = members::extract_type(cx, ty.dupe());
                    extract_def_locs_from_instancet(loc_of_aloc, cx, extracted_dupe, super_, name)
                }
                DefTInner::ObjT(_) => {
                    let extracted_dupe = members::extract_type(cx, ty.dupe());
                    let result = get_def_locs_from_extracted_type(cx, extracted_dupe, name)?;
                    Ok(match result {
                        None => DefLoc::NoDefFound,
                        Some(alocs) => {
                            if alocs.len() == 1 {
                                DefLoc::FoundObject(loc_of_aloc(alocs.first()))
                            } else {
                                DefLoc::FoundUnion(
                                    alocs.mapped_ref(|loc| DefLoc::FoundObject(loc_of_aloc(loc))),
                                )
                            }
                        }
                    })
                }
                _ => Ok(DefLoc::UnsupportedType),
            },
            TypeInner::ThisInstanceT(box ThisInstanceTData { instance: inst, .. }) => {
                let super_ = &inst.super_;
                let extracted_dupe = members::extract_type(cx, ty.dupe());
                extract_def_locs_from_instancet(loc_of_aloc, cx, extracted_dupe, super_, name)
            }
            TypeInner::UnionT(_, rep) => {
                let union_members: Result<Vec<DefLoc>, String> = rep
                    .members_iter()
                    .map(|member| extract_def_loc(loc_of_aloc, cx, member, name))
                    .collect();
                let members = union_members?;
                let members_nel = Vec1::try_from_vec(members)
                    .map_err(|_| "Union should have at least one member".to_string())?;
                Ok(DefLoc::FoundUnion(members_nel))
            }
            _ => Ok(DefLoc::UnsupportedType),
        },
        GenericT::SuccessNamespace(_) => {
            let extracted_dupe = members::extract_type(cx, ty.dupe());
            let result = get_def_locs_from_extracted_type(cx, extracted_dupe, name)?;
            Ok(match result {
                None => DefLoc::NoDefFound,
                Some(alocs) => {
                    if alocs.len() == 1 {
                        DefLoc::FoundObject(loc_of_aloc(alocs.first()))
                    } else {
                        DefLoc::FoundUnion(
                            alocs.mapped_ref(|loc| DefLoc::FoundObject(loc_of_aloc(loc))),
                        )
                    }
                }
            })
        }
        GenericT::FailureNullishType
        | GenericT::FailureUnhandledType(_)
        | GenericT::FailureUnhandledMembers(_) => Ok(DefLoc::UnsupportedType),
        GenericT::FailureAnyType => Ok(DefLoc::AnyType),
    }
}

fn get_loc_of_prop(
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    props: &flow_typing_type::type_::properties::PropertiesMap,
    name: &str,
) -> Option<Loc> {
    use flow_common::reason::Name;
    let key = Name::new(FlowSmolStr::new(name));
    match props.get(&key) {
        Some(prop) => {
            let read_loc = flow_typing_type::type_::property::read_loc(prop);
            read_loc.map(|aloc| loc_of_aloc(&aloc))
        }
        None => None,
    }
}

fn def_info_of_typecheck_results<'cx>(
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    cx: &mut Context<'cx>,
    obj_to_obj_map: &BTreeMap<Loc, BTreeSet<flow_typing_type::type_::properties::Id>>,
    props_access_info: DefKind<Loc>,
) -> Result<Option<PropertyDefInfo>, String> {
    // We want to include the immediate implementation as well as all superclass implementations.
    // If we wanted a mode where superclass implementations were not included, for example, we
    // could choose to take only the first extracted location.
    let def_info_of_class_member_locs = |locs: Vec1<Loc>| -> Vec1<SinglePropertyDefInfo> {
        locs.mapped(SinglePropertyDefInfo::ClassProperty)
    };

    fn def_info_of_def_loc(dl: DefLoc) -> Option<Vec1<SinglePropertyDefInfo>> {
        match dl {
            DefLoc::FoundClass(locs) => Some(locs.mapped(SinglePropertyDefInfo::ClassProperty)),
            DefLoc::FoundObject(loc) => Some(Vec1::new(SinglePropertyDefInfo::ObjectProperty(loc))),
            DefLoc::FoundUnion(def_locs) => {
                let results: Vec<Vec1<SinglePropertyDefInfo>> = def_locs
                    .into_iter()
                    .filter_map(def_info_of_def_loc)
                    .collect();
                if results.is_empty() {
                    None
                } else {
                    let mut iter = results.into_iter();
                    let mut acc = iter.next().unwrap();
                    for next in iter {
                        acc.extend(next);
                    }
                    Some(acc)
                }
            }
            DefLoc::NoDefFound | DefLoc::UnsupportedType | DefLoc::AnyType => None,
        }
    }

    let mut def_info_of_type =
        |name: &str, ty: &Type| -> Result<Option<Vec1<SinglePropertyDefInfo>>, String> {
            let dl = extract_def_loc(loc_of_aloc, cx, ty, name)?;
            Ok(def_info_of_def_loc(dl))
        };

    match props_access_info {
        DefKind::ObjDef(loc, name) => Ok(Some(PropertyDefInfo::OrdinaryProperty {
            props_info: Vec1::new(SinglePropertyDefInfo::ObjectProperty(loc)),
            name,
        })),

        DefKind::ClassDef(ty, name, static_) => {
            if static_ {
                // Here, `ty` ends up resolving to `ObjT` so we lose the knowledge that this is a static
                // property. This means that we don't get the fancy look-up-the-inheritance-chain behavior
                // that we get with class instances. That would be nice to add at some point.
                let result = def_info_of_type(&name, &ty)?;
                Ok(result.map(|def_info| PropertyDefInfo::OrdinaryProperty {
                    props_info: def_info,
                    name,
                }))
            } else {
                // We get the type of the class back here, so we need to extract the type of an instance
                let ty = extract_instancet(cx, &ty)?;
                let dl = extract_def_loc_resolved(loc_of_aloc, cx, &ty, &name)?;
                match dl {
                    DefLoc::FoundClass(locs) => Ok(Some(PropertyDefInfo::OrdinaryProperty {
                        props_info: def_info_of_class_member_locs(locs),
                        name,
                    })),
                    DefLoc::FoundUnion(_) | DefLoc::FoundObject(_) => {
                        Err("Expected to extract class def info from a class".to_string())
                    }
                    _ => {
                        Err("Unexpectedly failed to extract definition from known type".to_string())
                    }
                }
            }
        }

        DefKind::Use(ty, name) => {
            let result = def_info_of_type(&name, &ty)?;
            Ok(result.map(|def_info| PropertyDefInfo::OrdinaryProperty {
                props_info: def_info,
                name,
            }))
        }

        DefKind::ObjLiteral(loc, ty, name) => {
            use flow_typing_type::type_::DefTInner;

            match ty.deref() {
                TypeInner::DefT(_, def) => {
                    match def.deref() {
                        DefTInner::ObjT(obj) => {
                            let literal_obj_props_tmap_id = obj.props_tmap.dupe();
                            let literal_props = cx.find_props(literal_obj_props_tmap_id);
                            let literal_result =
                                match get_loc_of_prop(loc_of_aloc, &literal_props, &name) {
                                    Some(loc) => {
                                        Ok(Vec1::new(SinglePropertyDefInfo::ObjectProperty(loc)))
                                    }
                                    None => Err("Expected to find property on object definition"
                                        .to_string()),
                                };
                            let result = match literal_result {
                                Err(e) => Err(e),
                                Ok(literal_result) => {
                                    // Look up the objects that this object maps to
                                    match obj_to_obj_map.get(&loc) {
                                        Some(obj_prop_tmap_ids) => {
                                            let mut acc = Ok(literal_result);
                                            for props_tmap_set in obj_prop_tmap_ids {
                                                let props = cx.find_props(props_tmap_set.dupe());
                                                // Get the loc of the specific prop def
                                                match get_loc_of_prop(loc_of_aloc, &props, &name) {
                                                    Some(loc) => {
                                                        acc = acc.map(|mut a| {
                                                        a.push(SinglePropertyDefInfo::ObjectProperty(loc));
                                                        a
                                                    });
                                                    }
                                                    None => {
                                                        acc = Err("Expected to find property on object definition".to_string());
                                                    }
                                                }
                                            }
                                            acc
                                        }
                                        None => {
                                            // object literal has no upper bound objects
                                            Ok(literal_result)
                                        }
                                    }
                                }
                            };
                            result.map(|res| {
                                Some(PropertyDefInfo::OrdinaryProperty {
                                    props_info: res,
                                    name,
                                })
                            })
                        }
                        _ => Err("Expected to find an object".to_string()),
                    }
                }
                _ => Err("Expected to find an object".to_string()),
            }
        }

        DefKind::PrivateName {
            def_loc,
            references,
            name,
        } => Ok(Some(PropertyDefInfo::PrivateNameProperty {
            def_loc,
            references,
            name,
        })),
    }
}

pub fn get_property_def_info<'cx>(
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    cx: &mut Context<'cx>,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    obj_to_obj_map: &BTreeMap<Loc, BTreeSet<flow_typing_type::type_::properties::Id>>,
    loc: &Loc,
) -> Result<Option<PropertyDefInfo>, String> {
    let def_kind = def_kind_search::search(
        |aloc| {
            let l = loc_of_aloc(aloc);
            l.contains(loc)
        },
        typed_ast,
    )
    .map(|dk| map_def_kind_loc(|aloc: ALoc| loc_of_aloc(&aloc), dk));

    match def_kind {
        Some(dk) => def_info_of_typecheck_results(loc_of_aloc, cx, obj_to_obj_map, dk),
        None => Ok(None),
    }
}

pub fn get_def_info<'cx>(
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    purpose: &crate::get_def_types::Purpose,
    ast_info: &crate::find_refs_utils::AstInfo,
    cx: &mut Context<'cx>,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    obj_to_obj_map: &BTreeMap<Loc, BTreeSet<flow_typing_type::type_::properties::Id>>,
    loc: &Loc,
) -> Result<Result<DefInfo, String>, flow_utils_concurrency::job_error::JobError> {
    let prop_info = match get_property_def_info(loc_of_aloc, cx, typed_ast, obj_to_obj_map, loc) {
        Ok(p) => p,
        Err(e) => return Ok(Err(e)),
    };
    match prop_info {
        Some(props_info) => Ok(Ok(DefInfo::PropertyDefinition(props_info))),
        None => {
            let (ast, file_sig, _) = ast_info;
            let available_ast =
                flow_typing_utils::typed_ast_utils::AvailableAst::TypedAst(typed_ast);
            let result = crate::get_def_js::get_def(
                loc_of_aloc,
                cx,
                file_sig,
                None,
                ast,
                available_ast,
                purpose,
                loc,
            )?;
            Ok(match result {
                crate::get_def_js::GetDefResult::Def(locs, name) => Ok(
                    DefInfo::VariableDefinition(locs.into_iter().collect(), name),
                ),
                crate::get_def_js::GetDefResult::Partial(locs, name, _) => Ok(
                    DefInfo::VariableDefinition(locs.into_iter().collect(), name),
                ),
                crate::get_def_js::GetDefResult::BadLoc(error) => {
                    Ok(DefInfo::NoDefinition(Some(error)))
                }
                crate::get_def_js::GetDefResult::DefError(error) => Err(error),
            })
        }
    }
}
