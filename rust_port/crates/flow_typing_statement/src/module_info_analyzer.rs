/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::ops::Deref;

use dupe::Dupe;
use dupe::IterDupedExt;
use dupe::OptionDupedExt;
use flow_aloc::ALoc;
use flow_common::flow_import_specifier::FlowImportSpecifier;
use flow_common::flow_symbol::Symbol;
use flow_common::polarity::Polarity;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::reason::mk_reason;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::statement::StatementInner;
use flow_parser::ast_utils;
use flow_parser::loc::Loc;
use flow_typing_context::Context;
use flow_typing_context::ResolvedRequire;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::intermediate_error_types::ContextDependentUnsupportedStatement;
use flow_typing_errors::intermediate_error_types::UnsupportedSyntax;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::obj_type;
use flow_typing_flow_js::flow_js;
use flow_typing_type::type_::ExportKind;
use flow_typing_type::type_::ExportTypes;
use flow_typing_type::type_::FieldData;
use flow_typing_type::type_::ModuleType;
use flow_typing_type::type_::ModuleTypeInner;
use flow_typing_type::type_::NamedSymbol;
use flow_typing_type::type_::ObjKind;
use flow_typing_type::type_::ObjLiteral;
use flow_typing_type::type_::Property;
use flow_typing_type::type_::PropertyInner;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::exports;
use flow_typing_type::type_::properties;
use flow_typing_utils::type_env;

mod module_info {
    use std::collections::BTreeMap;

    use flow_aloc::ALoc;
    use flow_common::reason::Name;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_typing_type::type_::ModuleType;
    use flow_typing_type::type_::NamedSymbol;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::exports;

    pub(super) struct ModuleInfo {
        pub(super) kind: Kind,
        pub(super) type_named: exports::T,
        pub(super) type_star: Vec<(ALoc, Option<ModuleType>)>,
    }

    pub(super) enum CjsExportsState {
        CJSExportNames(BTreeMap<FlowSmolStr, (ALoc, Type)>),
        CJSModuleExports(ALoc, Type),
    }

    pub(super) enum Kind {
        Unknown,
        Cjs(CjsExportsState),
        ES {
            named: exports::T,
            star: Vec<(ALoc, Option<ModuleType>)>,
        },
    }

    pub(super) fn export_value(info: &mut ModuleInfo, name: Name, name_loc: ALoc, type_: Type) {
        match &mut info.kind {
            Kind::Unknown => {
                let mut named = exports::T::new();
                named.insert(name, NamedSymbol::new(Some(name_loc), None, type_, None));
                info.kind = Kind::ES {
                    named,
                    star: Vec::new(),
                };
            }
            Kind::ES { named, star: _ } => {
                named.insert(name, NamedSymbol::new(Some(name_loc), None, type_, None));
            }
            Kind::Cjs(_) => {
                // Indeterminate module. We already errored in module_exports_checker.
            }
        }
    }

    pub(super) fn export_star(
        info: &mut ModuleInfo,
        loc: ALoc,
        module_type_opt: Option<ModuleType>,
    ) {
        match &mut info.kind {
            Kind::Unknown => {
                info.kind = Kind::ES {
                    named: exports::T::new(),
                    star: vec![(loc, module_type_opt)],
                };
            }
            Kind::ES { named: _, star } => {
                star.push((loc, module_type_opt));
            }
            Kind::Cjs(_) => {
                // Indeterminate module. We already errored in module_exports_checker.
            }
        }
    }

    pub(super) fn export_type(info: &mut ModuleInfo, name: Name, name_loc: ALoc, type_: Type) {
        info.type_named
            .insert(name, NamedSymbol::new(Some(name_loc), None, type_, None));
    }

    pub(super) fn export_type_star(
        info: &mut ModuleInfo,
        loc: ALoc,
        module_type_opt: Option<ModuleType>,
    ) {
        info.type_star.push((loc, module_type_opt));
    }

    pub(super) fn cjs_mod_export(
        info: &mut ModuleInfo,
        f: impl FnOnce(CjsExportsState) -> CjsExportsState,
    ) {
        match std::mem::replace(&mut info.kind, Kind::Unknown) {
            Kind::Unknown => {
                info.kind = Kind::Cjs(f(CjsExportsState::CJSExportNames(BTreeMap::new())));
            }
            Kind::Cjs(state) => {
                info.kind = Kind::Cjs(f(state));
            }
            es @ Kind::ES { .. } => {
                info.kind = es;
            }
        }
    }

    // Re-exporting names from another file can lead to conflicts. We resolve
    // conflicts on a last-export-wins basis. Star exports are accumulated in
    // source order, so the head of each list is the last export. This helper
    // function interleaves the two reverse-sorted lists.
    pub(super) fn fold_star2<S, A, X, Y>(
        s: &mut S,
        f: &impl Fn(&mut S, A, &(ALoc, X)) -> A,
        g: &impl Fn(&mut S, A, &(ALoc, Y)) -> Result<A, flow_utils_concurrency::job_error::JobError>,
        mut acc: A,
        xs: &[(ALoc, X)],
        ys: &[(ALoc, Y)],
    ) -> Result<A, flow_utils_concurrency::job_error::JobError> {
        let mut xi = 0;
        let mut yi = 0;
        loop {
            match (xs.get(xi), ys.get(yi)) {
                (None, None) => return Ok(acc),
                (Some(_), None) => {
                    for x in &xs[xi..] {
                        acc = f(s, acc, x);
                    }
                    return Ok(acc);
                }
                (None, Some(_)) => {
                    for y in &ys[yi..] {
                        acc = g(s, acc, y)?;
                    }
                    return Ok(acc);
                }
                (Some(x), Some(y)) => {
                    if x.0.cmp(&y.0) == std::cmp::Ordering::Greater {
                        acc = f(s, acc, x);
                        xi += 1;
                    } else {
                        acc = g(s, acc, y)?;
                        yi += 1;
                    }
                }
            }
        }
    }
}

fn export_specifiers<'a>(
    cx: &mut Context<'a>,
    info: &mut module_info::ModuleInfo,
    loc: &ALoc,
    source: &Option<((ALoc, Type), ast::StringLiteral<ALoc>)>,
    export_kind: ast::statement::ExportKind,
    specifiers: &ast::statement::export_named_declaration::Specifier<ALoc, (ALoc, Type)>,
) {
    let is_ts_file = flow_common::files::has_ts_ext(cx.file());
    let classify_ts_local_export = |cx: &mut Context<'a>,
                                    kind: ast::statement::ExportKind,
                                    loc: &ALoc|
     -> ast::statement::ExportKind {
        if kind != ast::statement::ExportKind::ExportValue || !is_ts_file {
            return kind;
        }
        let binding_info = type_env::local_export_binding_at_loc(cx, loc.dupe());
        match binding_info {
            Some(type_env::LocalExportBinding {
                val_kind: flow_env_builder::env_api::ValKind::Type { .. },
                ..
            }) => ast::statement::ExportKind::ExportType,
            Some(type_env::LocalExportBinding {
                def_loc: Some(def_loc),
                val_kind: flow_env_builder::env_api::ValKind::TsImport,
            }) => {
                if flow_js_utils::import_export_utils::is_ts_import_type_only(cx, &def_loc) {
                    ast::statement::ExportKind::ExportType
                } else {
                    ast::statement::ExportKind::ExportValue
                }
            }
            _ => ast::statement::ExportKind::ExportValue,
        }
    };
    // [declare] export [type] {[type] foo [as bar]};
    let export_ref = |cx: &mut Context<'a>,
                      info: &mut module_info::ModuleInfo,
                      kind: ast::statement::ExportKind,
                      loc: ALoc,
                      local_type: Type,
                      remote_name: Name,
                      _source_local_name: FlowSmolStr| {
        let kind = classify_ts_local_export(cx, kind, &loc);
        match kind {
            ast::statement::ExportKind::ExportType => {
                module_info::export_type(info, remote_name, loc, local_type)
            }
            ast::statement::ExportKind::ExportValue => {
                module_info::export_value(info, remote_name, loc, local_type)
            }
        }
    };
    let classify_ts_from_export = |cx: &mut Context<'a>,
                                   kind: ast::statement::ExportKind,
                                   source_local_name: &FlowSmolStr|
     -> ast::statement::ExportKind {
        if kind != ast::statement::ExportKind::ExportValue || !is_ts_file {
            return kind;
        }
        match source {
            None => kind,
            Some((_, source_literal)) => {
                let source_module_name = &source_literal.value;
                let module_specifier = flow_common::flow_import_specifier::Userland::from_smol_str(
                    source_module_name.dupe(),
                );
                match cx.find_require(
                    &flow_common::flow_import_specifier::FlowImportSpecifier::Userland(
                        module_specifier,
                    ),
                ) {
                    flow_typing_context::ResolvedRequire::TypedModule(f) => match f(cx) {
                        Ok(m) => {
                            match flow_js_utils::import_export_utils::classify_named_export(
                                cx,
                                &m,
                                source_local_name,
                            ) {
                                flow_js_utils::ExportClassification::FoundTypeOnly(_) => {
                                    ast::statement::ExportKind::ExportType
                                }
                                _ => ast::statement::ExportKind::ExportValue,
                            }
                        }
                        Err(_) => kind,
                    },
                    _ => kind,
                }
            }
        }
    };
    // [declare] export [type] {[type] foo [as bar]} from 'module'
    let export_from = |cx: &mut Context<'a>,
                       info: &mut module_info::ModuleInfo,
                       kind: ast::statement::ExportKind,
                       loc: ALoc,
                       local_type: Type,
                       remote_name: Name,
                       source_local_name: FlowSmolStr| {
        let kind = classify_ts_from_export(cx, kind, &source_local_name);
        match kind {
            ast::statement::ExportKind::ExportType => {
                module_info::export_type(info, remote_name, loc, local_type)
            }
            ast::statement::ExportKind::ExportValue => {
                module_info::export_value(info, remote_name, loc, local_type)
            }
        }
    };
    let mut export_specifier =
        |cx: &mut Context<'a>,
         export: &dyn Fn(
            &mut Context<'a>,
            &mut module_info::ModuleInfo,
            ast::statement::ExportKind,
            ALoc,
            Type,
            Name,
            FlowSmolStr,
        ),
         specifier: &ast::statement::export_named_declaration::ExportSpecifier<
            ALoc,
            (ALoc, Type),
        >| {
            let ast::statement::export_named_declaration::ExportSpecifier {
                loc: _,
                local,
                exported,
                export_kind: specifier_export_kind,
                from_remote: _,
                imported_name_def_loc: _,
            } = specifier;
            let kind =
                flow_parser::ast_utils::effective_export_kind(export_kind, *specifier_export_kind);
            let (local_loc, local_type) = &local.loc;
            let local_name = &local.name;
            let remote_name = match exported {
                None => Name::new(local_name.dupe()),
                Some(exported_id) => Name::new(exported_id.name.dupe()),
            };
            export(
                cx,
                info,
                kind,
                local_loc.dupe(),
                local_type.dupe(),
                remote_name,
                local_name.dupe(),
            )
        };

    match specifiers {
        // [declare] export [type] {[type] foo [as bar]} [from ...];
        ast::statement::export_named_declaration::Specifier::ExportSpecifiers(specifiers) => {
            let export: &dyn Fn(
                &mut Context<'a>,
                &mut module_info::ModuleInfo,
                ast::statement::ExportKind,
                ALoc,
                Type,
                Name,
                FlowSmolStr,
            ) = match source {
                Some(_) => &export_from,
                None => &export_ref,
            };
            for specifier in specifiers {
                export_specifier(cx, export, specifier);
            }
        }
        // [declare] export [type] * as id from "source";
        ast::statement::export_named_declaration::Specifier::ExportBatchSpecifier(
            ast::statement::export_named_declaration::ExportBatchSpecifier {
                loc: _,
                specifier: Some(id),
            },
        ) => {
            let (_, t) = &id.loc;
            let name = &id.name;
            match export_kind {
                ast::statement::ExportKind::ExportValue => {
                    module_info::export_value(info, Name::new(name.dupe()), loc.dupe(), t.dupe());
                }
                ast::statement::ExportKind::ExportType => {
                    module_info::export_type(info, Name::new(name.dupe()), loc.dupe(), t.dupe());
                }
            }
        }
        // [declare] export [type] * from "source";
        ast::statement::export_named_declaration::Specifier::ExportBatchSpecifier(
            ast::statement::export_named_declaration::ExportBatchSpecifier {
                loc: _,
                specifier: None,
            },
        ) => {
            let (_, source_lit) = source.as_ref().unwrap();
            let value = &source_lit.value;
            let module_type_opt =
                match cx.find_require(&FlowImportSpecifier::userland(value.dupe())) {
                    ResolvedRequire::TypedModule(f) => f(cx).ok(),
                    _ => None,
                };
            match export_kind {
                ast::statement::ExportKind::ExportValue => {
                    module_info::export_star(info, loc.dupe(), module_type_opt);
                }
                ast::statement::ExportKind::ExportType => {
                    module_info::export_type_star(info, loc.dupe(), module_type_opt);
                }
            }
        }
    }
}

fn visit_toplevel_statement<'a>(
    cx: &mut Context<'a>,
    info: &mut module_info::ModuleInfo,
    in_declare_namespace: bool,
    stmt: &ast::statement::Statement<ALoc, (ALoc, Type)>,
) {
    match stmt.deref() {
        StatementInner::Empty { .. }
        | StatementInner::Block { .. }
        | StatementInner::If { .. }
        | StatementInner::Labeled { .. }
        | StatementInner::Break { .. }
        | StatementInner::Continue { .. }
        | StatementInner::Throw { .. }
        | StatementInner::Try { .. }
        | StatementInner::With { .. }
        | StatementInner::Match { .. }
        | StatementInner::Switch { .. }
        | StatementInner::Return { .. }
        | StatementInner::While { .. }
        | StatementInner::DoWhile { .. }
        | StatementInner::For { .. }
        | StatementInner::ForIn { .. }
        | StatementInner::ForOf { .. }
        | StatementInner::Debugger { .. }
        | StatementInner::FunctionDeclaration { .. }
        | StatementInner::ComponentDeclaration { .. }
        | StatementInner::VariableDeclaration { .. }
        | StatementInner::RecordDeclaration { .. }
        | StatementInner::DeclareModule { .. }
        | StatementInner::ImportDeclaration { .. }
        | StatementInner::ImportEqualsDeclaration { .. }
        | StatementInner::NamespaceExportDeclaration { .. } => {}
        StatementInner::DeclareNamespace { inner, .. } => match &inner.id {
            ast::statement::declare_namespace::Id::Global(_) => {}
            ast::statement::declare_namespace::Id::Local(id) => {
                //A declared namespace will auto-export all toplevel names
                if in_declare_namespace {
                    let (name_loc, t) = &id.loc;
                    let name = &id.name;
                    module_info::export_value(
                        info,
                        Name::new(name.dupe()),
                        name_loc.dupe(),
                        t.dupe(),
                    );
                }
            }
        },

        StatementInner::DeclareTypeAlias { inner, .. }
        | StatementInner::TypeAlias { inner, .. } => {
            if in_declare_namespace {
                let id = &inner.id;
                let (name_loc, t) = &id.loc;
                let name = &id.name;
                module_info::export_type(info, Name::new(name.dupe()), name_loc.dupe(), t.dupe());
            }
        }
        StatementInner::DeclareOpaqueType { inner, .. }
        | StatementInner::OpaqueType { inner, .. } => {
            if in_declare_namespace {
                let id = &inner.id;
                let (name_loc, t) = &id.loc;
                let name = &id.name;
                module_info::export_type(info, Name::new(name.dupe()), name_loc.dupe(), t.dupe());
            }
        }
        StatementInner::DeclareInterface { inner, .. }
        | StatementInner::InterfaceDeclaration { inner, .. } => {
            if in_declare_namespace {
                let id = &inner.id;
                let (name_loc, t) = &id.loc;
                let name = &id.name;
                module_info::export_type(info, Name::new(name.dupe()), name_loc.dupe(), t.dupe());
            }
        }

        StatementInner::DeclareVariable { inner, .. } => {
            // A declared namespace will auto-export all toplevel names
            if in_declare_namespace {
                for decl in inner.declarations.iter() {
                    if let ast::pattern::Pattern::Identifier { inner: pat_id, .. } = &decl.id {
                        let (name_loc, t) = &pat_id.name.loc;
                        let name = &pat_id.name.name;
                        module_info::export_value(
                            info,
                            Name::new(name.dupe()),
                            name_loc.dupe(),
                            t.dupe(),
                        );
                    }
                }
            }
        }
        StatementInner::DeclareFunction { inner, .. } => {
            if in_declare_namespace {
                if let Some(id) = &inner.id {
                    let (name_loc, t) = &id.loc;
                    let name = &id.name;
                    module_info::export_value(
                        info,
                        Name::new(name.dupe()),
                        name_loc.dupe(),
                        t.dupe(),
                    );
                }
            }
        }
        StatementInner::ClassDeclaration { inner, .. } => {
            if in_declare_namespace && let Some(id) = &inner.id {
                let (name_loc, t) = &id.loc;
                module_info::export_value(
                    info,
                    Name::new(id.name.dupe()),
                    name_loc.dupe(),
                    t.dupe(),
                );
            }
        }
        StatementInner::DeclareClass { inner, .. } => {
            if in_declare_namespace {
                let id = &inner.id;
                let (name_loc, t) = &id.loc;
                let name = &id.name;
                module_info::export_value(info, Name::new(name.dupe()), name_loc.dupe(), t.dupe());
            }
        }
        StatementInner::DeclareComponent { inner, .. } => {
            if in_declare_namespace {
                let id = &inner.id;
                let (name_loc, t) = &id.loc;
                let name = &id.name;
                module_info::export_value(info, Name::new(name.dupe()), name_loc.dupe(), t.dupe());
            }
        }
        StatementInner::DeclareEnum { inner, .. }
        | StatementInner::EnumDeclaration { inner, .. } => {
            if in_declare_namespace {
                let id = &inner.id;
                let (name_loc, t) = &id.loc;
                let name = &id.name;
                module_info::export_value(info, Name::new(name.dupe()), name_loc.dupe(), t.dupe());
            }
        }

        StatementInner::Expression { inner, .. } => {
            let expr = &inner.expression;
            if let ast::expression::ExpressionInner::Assignment { inner: assign, .. } = expr.deref()
                && assign.operator.is_none()
                && let flow_parser::ast::pattern::Pattern::Expression {
                    inner: left_expr, ..
                } = &assign.left
                && let ast::expression::ExpressionInner::Member { inner: member, .. } =
                    left_expr.deref().deref()
            {
                let (_, t) = &assign.right.loc();
                match member.object.deref() {
                    // module.exports = ...
                    ast::expression::ExpressionInner::Identifier { inner: obj_id, .. }
                        if obj_id.name.as_str() == "module" =>
                    {
                        let (module_loc, _) = &obj_id.loc;
                        if type_env::is_global_var(cx, module_loc.dupe()) {
                            if let ast::expression::member::Property::PropertyIdentifier(prop_id) =
                                &member.property
                                && prop_id.name.as_str() == "exports"
                            {
                                let (exports_loc, _) = &prop_id.loc;
                                let exports_loc = exports_loc.dupe();
                                let t = t.dupe();
                                module_info::cjs_mod_export(info, move |_| {
                                    module_info::CjsExportsState::CJSModuleExports(exports_loc, t)
                                });
                            }
                        }
                    }
                    // module.exports.foo = ...
                    ast::expression::ExpressionInner::Member {
                        inner: outer_member,
                        ..
                    } => {
                        if let ast::expression::ExpressionInner::Identifier {
                            inner: obj_id, ..
                        } = outer_member.object.deref()
                            && obj_id.name.as_str() == "module"
                            && let (module_loc, _) = &obj_id.loc
                            && type_env::is_global_var(cx, module_loc.dupe())
                            && let ast::expression::member::Property::PropertyIdentifier(exports_id) =
                                &outer_member.property
                            && exports_id.name.as_str() == "exports"
                            && let ast::expression::member::Property::PropertyIdentifier(prop_id) =
                                &member.property
                        {
                            let (key_loc, _) = &prop_id.loc;
                            let name = prop_id.name.dupe();
                            let key_loc = key_loc.dupe();
                            let t = t.dupe();
                            module_info::cjs_mod_export(info, move |state| match state {
                                module_info::CjsExportsState::CJSModuleExports(_, _) => state,
                                module_info::CjsExportsState::CJSExportNames(mut named) => {
                                    named.insert(name, (key_loc, t));
                                    module_info::CjsExportsState::CJSExportNames(named)
                                }
                            });
                        }
                    }
                    // exports.foo = ...
                    ast::expression::ExpressionInner::Identifier { inner: obj_id, .. }
                        if obj_id.name.as_str() == "exports" =>
                    {
                        let (exports_loc, _) = &obj_id.loc;
                        if type_env::is_global_var(cx, exports_loc.dupe()) {
                            if let ast::expression::member::Property::PropertyIdentifier(prop_id) =
                                &member.property
                            {
                                let (key_loc, _) = &prop_id.loc;
                                let name = prop_id.name.dupe();
                                let key_loc = key_loc.dupe();
                                let t = t.dupe();
                                module_info::cjs_mod_export(info, move |state| match state {
                                    module_info::CjsExportsState::CJSModuleExports(_, _) => state,
                                    module_info::CjsExportsState::CJSExportNames(mut named) => {
                                        named.insert(name, (key_loc, t));
                                        module_info::CjsExportsState::CJSExportNames(named)
                                    }
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        StatementInner::DeclareExportDeclaration { loc, inner } => {
            let ast::statement::DeclareExportDeclaration {
                default,
                declaration,
                specifiers,
                source,
                comments: _,
            } = inner.deref();
            let export_maybe_default_binding =
                |info: &mut module_info::ModuleInfo, id: &ast::Identifier<ALoc, (ALoc, Type)>| {
                    let (name_loc, t) = &id.loc;
                    match default {
                        None => {
                            module_info::export_value(
                                info,
                                Name::new(id.name.dupe()),
                                name_loc.dupe(),
                                t.dupe(),
                            );
                        }
                        Some(default_loc) => {
                            module_info::export_value(
                                info,
                                Name::new(FlowSmolStr::new("default")),
                                default_loc.dupe(),
                                t.dupe(),
                            );
                        }
                    }
                };

            if let Some(declaration) = declaration {
                match declaration {
                    ast::statement::declare_export_declaration::Declaration::Variable {
                        declaration,
                        ..
                    } => {
                        for decl in declaration.declarations.iter() {
                            if let ast::pattern::Pattern::Identifier { inner: pat_id, .. } =
                                &decl.id
                            {
                                let (name_loc, t) = &pat_id.name.loc;
                                let name = &pat_id.name.name;
                                module_info::export_value(
                                    info,
                                    Name::new(name.dupe()),
                                    name_loc.dupe(),
                                    t.dupe(),
                                );
                            }
                        }
                    }
                    ast::statement::declare_export_declaration::Declaration::Function {
                        declaration,
                        ..
                    } => match &declaration.id {
                        Some(id) => {
                            export_maybe_default_binding(info, id);
                        }
                        None => {
                            let default_loc = default.as_ref().unwrap();
                            let (_, t) = declaration.annot.annotation.loc();
                            module_info::export_value(
                                info,
                                Name::new(FlowSmolStr::new("default")),
                                default_loc.dupe(),
                                t.dupe(),
                            );
                        }
                    },
                    ast::statement::declare_export_declaration::Declaration::Class {
                        declaration,
                        ..
                    } => {
                        export_maybe_default_binding(info, &declaration.id);
                    }
                    ast::statement::declare_export_declaration::Declaration::Component {
                        declaration,
                        ..
                    } => {
                        export_maybe_default_binding(info, &declaration.id);
                    }
                    ast::statement::declare_export_declaration::Declaration::DefaultType {
                        type_,
                    } => {
                        let default_loc = default.as_ref().unwrap();
                        let (_, t) = type_.loc();
                        module_info::export_type(
                            info,
                            Name::new(FlowSmolStr::new("default")),
                            default_loc.dupe(),
                            t.dupe(),
                        );
                    }
                    ast::statement::declare_export_declaration::Declaration::NamedType {
                        declaration,
                        ..
                    } => {
                        let id = &declaration.id;
                        let (name_loc, t) = &id.loc;
                        let name = &id.name;
                        module_info::export_type(
                            info,
                            Name::new(name.dupe()),
                            name_loc.dupe(),
                            t.dupe(),
                        );
                    }
                    ast::statement::declare_export_declaration::Declaration::NamedOpaqueType {
                        declaration,
                        ..
                    } => {
                        let id = &declaration.id;
                        let (name_loc, t) = &id.loc;
                        let name = &id.name;
                        module_info::export_type(
                            info,
                            Name::new(name.dupe()),
                            name_loc.dupe(),
                            t.dupe(),
                        );
                    }
                    ast::statement::declare_export_declaration::Declaration::Interface {
                        declaration,
                        ..
                    } => {
                        let id = &declaration.id;
                        let (name_loc, t) = &id.loc;
                        let name = &id.name;
                        module_info::export_type(
                            info,
                            Name::new(name.dupe()),
                            name_loc.dupe(),
                            t.dupe(),
                        );
                    }
                    ast::statement::declare_export_declaration::Declaration::Enum {
                        declaration,
                        ..
                    } => {
                        let id = &declaration.id;
                        let (name_loc, t) = &id.loc;
                        let name = &id.name;
                        if cx.enable_enums() {
                            module_info::export_value(
                                info,
                                Name::new(name.dupe()),
                                name_loc.dupe(),
                                t.dupe(),
                            );
                        }
                    }
                    ast::statement::declare_export_declaration::Declaration::Namespace {
                        declaration,
                        ..
                    } => match &declaration.id {
                        ast::statement::declare_namespace::Id::Local(id) => {
                            let (name_loc, t) = &id.loc;
                            let name = &id.name;
                            module_info::export_value(
                                info,
                                Name::new(name.dupe()),
                                name_loc.dupe(),
                                t.dupe(),
                            );
                        }
                        ast::statement::declare_namespace::Id::Global(_) => {}
                    },
                }
            }
            let ek = ast::statement::ExportKind::ExportValue;
            if let Some(specifiers) = specifiers {
                export_specifiers(cx, info, loc, source, ek, specifiers);
            }
        }
        StatementInner::DeclareModuleExports { inner, .. } => {
            let annot = &inner.annot;
            let (_, t) = &annot.annotation.loc();
            let exports_loc = annot.loc.dupe();
            let t = t.dupe();
            module_info::cjs_mod_export(info, move |_| {
                module_info::CjsExportsState::CJSModuleExports(exports_loc, t)
            });
        }
        StatementInner::ExportAssignment { inner, .. } => match &inner.rhs {
            ast::statement::ExportAssignmentRhs::Expression(e) => {
                let (exports_loc, t) = e.loc();
                let exports_loc = exports_loc.dupe();
                let t = t.dupe();
                module_info::cjs_mod_export(info, move |_| {
                    module_info::CjsExportsState::CJSModuleExports(exports_loc, t)
                });
            }
            ast::statement::ExportAssignmentRhs::DeclareFunction(_, decl) => {
                let (exports_loc, t) = decl.annot.annotation.loc();
                let exports_loc = exports_loc.dupe();
                let t = t.dupe();
                module_info::cjs_mod_export(info, move |_| {
                    module_info::CjsExportsState::CJSModuleExports(exports_loc, t)
                });
            }
        },
        StatementInner::ExportNamedDeclaration { loc, inner } => {
            let ast::statement::ExportNamedDeclaration {
                declaration,
                specifiers,
                source,
                attributes: _,
                export_kind,
                comments: _,
            } = inner.deref();

            let export_id = |info: &mut module_info::ModuleInfo,
                             id: &ast::Identifier<ALoc, (ALoc, Type)>| {
                let (name_loc, t) = &id.loc;
                let name = &id.name;
                match export_kind {
                    ast::statement::ExportKind::ExportValue => {
                        module_info::export_value(
                            info,
                            Name::new(name.dupe()),
                            name_loc.dupe(),
                            t.dupe(),
                        );
                    }
                    ast::statement::ExportKind::ExportType => {
                        module_info::export_type(
                            info,
                            Name::new(name.dupe()),
                            name_loc.dupe(),
                            t.dupe(),
                        );
                    }
                }
            };
            if let Some(decl_stmt) = declaration {
                match decl_stmt.deref() {
                    StatementInner::FunctionDeclaration { inner, .. } => {
                        if let Some(id) = &inner.id {
                            export_id(info, id);
                        }
                    }
                    StatementInner::ClassDeclaration { inner, .. } => {
                        if let Some(id) = &inner.id {
                            export_id(info, id);
                        }
                    }
                    StatementInner::TypeAlias { inner, .. } => {
                        export_id(info, &inner.id);
                    }
                    StatementInner::OpaqueType { inner, .. } => {
                        export_id(info, &inner.id);
                    }
                    StatementInner::InterfaceDeclaration { inner, .. } => {
                        export_id(info, &inner.id);
                    }
                    StatementInner::ComponentDeclaration { inner, .. } => {
                        export_id(info, &inner.id);
                    }
                    StatementInner::EnumDeclaration { inner, .. } => {
                        export_id(info, &inner.id);
                    }
                    StatementInner::VariableDeclaration { inner, .. } => {
                        ast_utils::fold_bindings_of_variable_declarations(
                            (),
                            &inner.declarations,
                            &mut |_has_annot, (), id| {
                                export_id(info, id);
                            },
                        );
                    }
                    // Parser Error: Invalid export-declaration type!
                    _ => {}
                }
            }
            if let Some(specifiers) = specifiers {
                export_specifiers(cx, info, loc, source, *export_kind, specifiers);
            }
        }
        StatementInner::ExportDefaultDeclaration { loc: _, inner } => {
            let (_, t) = &inner.default;
            let export_loc = match &inner.declaration {
                ast::statement::export_default_declaration::Declaration::Declaration(decl_stmt) => {
                    match decl_stmt.deref() {
                        StatementInner::FunctionDeclaration { inner, .. } => {
                            if let Some(id) = &inner.id {
                                let (id_loc, _) = &id.loc;
                                id_loc.dupe()
                            } else {
                                decl_stmt.loc().dupe()
                            }
                        }
                        StatementInner::ClassDeclaration { inner, .. } => {
                            if let Some(id) = &inner.id {
                                let (id_loc, _) = &id.loc;
                                id_loc.dupe()
                            } else {
                                decl_stmt.loc().dupe()
                            }
                        }
                        StatementInner::EnumDeclaration { inner, .. } => {
                            let (id_loc, _) = &inner.id.loc;
                            id_loc.dupe()
                        }
                        StatementInner::ComponentDeclaration { inner, .. } => {
                            let (id_loc, _) = &inner.id.loc;
                            id_loc.dupe()
                        }
                        _ => decl_stmt.loc().dupe(),
                    }
                }
                ast::statement::export_default_declaration::Declaration::Expression(expr) => {
                    let (loc, _) = expr.loc();
                    loc.dupe()
                }
            };
            module_info::export_value(
                info,
                Name::new(FlowSmolStr::new("default")),
                export_loc,
                t.dupe(),
            );
        }
    }
}

// A best effort way to pick a location as the signature location of the module.
// - For cjs, we will first try to pick the location of module.exports, then
//   fallback to the first module.exports prop assignment
// - For esm, we will first try to pick the location of default exports, then
//   fallback to the first export.
fn module_exports_sig_loc(info: &module_info::ModuleInfo) -> Option<ALoc> {
    let first_loc_of_named_exports = |named: &exports::T| -> Option<ALoc> {
        let mut locs: Vec<ALoc> = named
            .values()
            .filter_map(|ns| ns.name_loc.as_ref())
            .duped()
            .collect();
        locs.sort();
        locs.into_iter().next()
    };

    match &info.kind {
        module_info::Kind::Unknown => None,
        module_info::Kind::Cjs(module_info::CjsExportsState::CJSModuleExports(l, _)) => {
            Some(l.dupe())
        }
        module_info::Kind::Cjs(module_info::CjsExportsState::CJSExportNames(names)) => {
            let mut locs: Vec<ALoc> = names.values().map(|(loc, _)| loc.dupe()).collect();
            locs.sort();
            match locs.into_iter().next() {
                None => first_loc_of_named_exports(&info.type_named),
                loc => loc,
            }
        }
        module_info::Kind::ES { named, .. } => {
            let default_name = Name::new(FlowSmolStr::new("default"));
            match named.get(&default_name) {
                Some(ns) => ns.name_loc.as_ref().duped(),
                None => match first_loc_of_named_exports(named) {
                    None => first_loc_of_named_exports(&info.type_named),
                    loc => loc,
                },
            }
        }
    }
}

// After we have seen all the export statements in a module, this function will
// calculate a ModuleT type (or a tvar that resolves to one) describing the
// exports of a file.
//
// For CommonJS modules, this is fairly simple. We have the exported value
// itself, plus any type exports. If the exported value is an object, we treat
// the fields as named exports for ES module dependents.
//
// For ES modules, we have both named exports and "star" exports, which copy the
// exports of one file into another. This can lead to conflits, which are
// resolved carefully. Note that locally named exports always win, even if they
// are followed by a star export that includes a conflicting name.
//
// Finally, both CJS and ES modules can export types, which also has a star
// export variant. Conflicts are handled in the same way.
fn mk_module_type<'a>(
    cx: &mut Context<'a>,
    info: &module_info::ModuleInfo,
    self_reason: Reason,
    exports_reason: Reason,
) -> Result<ModuleType, flow_utils_concurrency::job_error::JobError> {
    fn mk_esm_module_type<'a>(cx: &mut Context<'a>, module_reason: Reason) -> ModuleType {
        ModuleType::new(ModuleTypeInner {
            module_reason,
            module_export_types: ExportTypes {
                value_exports_tmap: cx.make_export_map(exports::T::new()),
                type_exports_tmap: cx.make_export_map(exports::T::new()),
                cjs_export: None,
                has_every_named_export: false,
            },
            module_is_strict: cx.is_strict(),
            module_available_platforms: cx.available_platforms().cloned(),
        })
    }

    // When CommonJS modules set their export type, we do two things:
    //
    // (1) Set the type in the cjs_export slot of the ModuleT container
    //
    // (2) If the type is an object, mark it's properties as named exports, via
    //     CJSExtractNamedExportsT. (this is for convenience as part of our
    //     ES <-> CJS module interop semantics)
    fn mk_commonjs_module_t<'a>(
        cx: &mut Context<'a>,
        reason_exports_module: Reason,
        reason: Reason,
        cjs_exports_state: &module_info::CjsExportsState,
    ) -> Result<ModuleType, flow_utils_concurrency::job_error::JobError> {
        let (def_loc, export_t) = match cjs_exports_state {
            module_info::CjsExportsState::CJSModuleExports(def_loc, t) => {
                (Some(def_loc.dupe()), t.dupe())
            }
            module_info::CjsExportsState::CJSExportNames(named) => {
                let props: properties::PropertiesMap = named
                    .iter()
                    .map(|(name, (key_loc, type_))| {
                        (
                            Name::new(name.dupe()),
                            Property::new(PropertyInner::Field(Box::new(FieldData {
                                preferred_def_locs: None,
                                key_loc: Some(key_loc.dupe()),
                                polarity: Polarity::Positive,
                                type_: type_.dupe(),
                            }))),
                        )
                    })
                    .collect();
                let proto = Type::new(TypeInner::ObjProtoT(reason.dupe()));
                (
                    None,
                    obj_type::mk_with_proto(
                        cx,
                        reason.dupe(),
                        ObjKind::Exact,
                        None,
                        None,
                        Some(props),
                        None,
                        proto,
                        ObjLiteral::NonLiteral,
                    ),
                )
            }
        };
        let module_export_types = ExportTypes {
            value_exports_tmap: cx.make_export_map(exports::T::new()),
            type_exports_tmap: cx.make_export_map(exports::T::new()),
            cjs_export: Some((def_loc, export_t.dupe())),
            has_every_named_export: false,
        };
        let local_module = ModuleType::new(ModuleTypeInner {
            module_reason: reason_exports_module,
            module_export_types,
            module_is_strict: cx.is_strict(),
            module_available_platforms: cx.available_platforms().cloned(),
        });
        let reason2 = reason.dupe();
        let concretize = |cx: &mut Context<'_>,
                          t: Type|
         -> Result<Type, flow_utils_concurrency::job_error::JobError> {
            match flow_js::FlowJs::singleton_concrete_type_for_cjs_extract_named_exports_and_type_exports(
                cx, &reason2, &t,
            ) {
                Ok(v) => Ok(v),
                Err(flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c)) => {
                    Err(flow_utils_concurrency::job_error::JobError::Canceled(c))
                }
                Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => {
                    Err(flow_utils_concurrency::job_error::JobError::TimedOut(t))
                }
                Err(err) => panic!("Should not be under speculation: {:?}", err),
            }
        };
        flow_js_utils::cjs_extract_named_exports_t_kit::on_type(
            cx,
            &concretize,
            reason,
            local_module,
            export_t,
        )
    }

    fn copy_star_exports<'a>(
        cx: &mut Context<'a>,
        reason: Reason,
        value_star: &[(ALoc, Option<ModuleType>)],
        type_star: &[(ALoc, Option<ModuleType>)],
        module_t: ModuleType,
    ) -> Result<ModuleType, flow_utils_concurrency::job_error::JobError> {
        let copy_named_exports = |cx: &mut Context<'a>,
                                  target: ModuleType,
                                  (_, from_ns): &(ALoc, Option<ModuleType>)|
         -> ModuleType {
            match from_ns {
                Some(src_module_type) => {
                    flow_js_utils::copy_named_exports_t_kit::mod_module_t(
                        cx,
                        &target,
                        src_module_type,
                    );
                    target
                }
                None => target,
            }
        };
        let copy_type_exports = |cx: &mut Context<'a>,
                                 target: ModuleType,
                                 (loc, from_ns): &(ALoc, Option<ModuleType>)|
         -> Result<
            ModuleType,
            flow_utils_concurrency::job_error::JobError,
        > {
            let repos_reason = reason.dupe().reposition(loc.dupe());
            match from_ns {
                Some(src_module_type) => {
                    flow_js_utils::copy_type_exports_t_kit::mod_module_t(
                        cx,
                        |cx_inner, r: Reason, t: Type| -> Result<Type, flow_utils_concurrency::job_error::JobError> {
                            match flow_js::FlowJs::singleton_concrete_type_for_cjs_extract_named_exports_and_type_exports(
                                cx_inner,
                                &r,
                                &t,
                            ) {
                                Ok(v) => Ok(v),
                                Err(flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c)) => {
                                    Err(flow_utils_concurrency::job_error::JobError::Canceled(c))
                                }
                                Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => {
                                    Err(flow_utils_concurrency::job_error::JobError::TimedOut(t))
                                }
                                Err(err) => panic!("Should not be under speculation: {:?}", err),
                            }
                        },
                        repos_reason,
                        &target,
                        src_module_type,
                    )?;
                    Ok(target)
                }
                None => Ok(target),
            }
        };
        module_info::fold_star2(
            cx,
            &copy_named_exports,
            &copy_type_exports,
            module_t,
            value_star,
            type_star,
        )
    }

    match &info.kind {
        module_info::Kind::Unknown => {
            let cjs_state = module_info::CjsExportsState::CJSExportNames(BTreeMap::new());
            let module_type =
                mk_commonjs_module_t(cx, self_reason.dupe(), exports_reason, &cjs_state)?;
            flow_js_utils::export_named_t_kit::mod_module_t(
                cx,
                exports::T::new(),
                info.type_named.dupe(),
                ExportKind::DirectExport,
                &module_type,
            );
            copy_star_exports(cx, self_reason, &[], &info.type_star, module_type)
        }
        module_info::Kind::Cjs(cjs_exports_state) => {
            let module_type =
                mk_commonjs_module_t(cx, self_reason.dupe(), exports_reason, cjs_exports_state)?;
            flow_js_utils::export_named_t_kit::mod_module_t(
                cx,
                exports::T::new(),
                info.type_named.dupe(),
                ExportKind::DirectExport,
                &module_type,
            );
            copy_star_exports(cx, self_reason, &[], &info.type_star, module_type)
        }
        module_info::Kind::ES { named, star } => {
            let module_type = mk_esm_module_type(cx, self_reason.dupe());
            flow_js_utils::export_named_t_kit::mod_module_t(
                cx,
                named.dupe(),
                info.type_named.dupe(),
                ExportKind::DirectExport,
                &module_type,
            );
            copy_star_exports(cx, self_reason, star, &info.type_star, module_type)
        }
    }
}

fn mk_namespace_t<'a>(
    cx: &mut Context<'a>,
    info: &module_info::ModuleInfo,
    namespace_symbol: Symbol,
    reason: Reason,
) -> Type {
    if !info.type_star.is_empty() {
        panic!("namespace should not have star exports");
    }
    let named = match &info.kind {
        module_info::Kind::Unknown => exports::T::new(),
        module_info::Kind::Cjs(_) => {
            panic!("namespace should never transition into CJS state")
        }
        module_info::Kind::ES { named: _, star } if !star.is_empty() => {
            panic!("namespace should not have star exports")
        }
        module_info::Kind::ES { named, star: _ } => named.dupe(),
    };
    let named_btree: BTreeMap<Name, NamedSymbol> =
        named.iter().map(|(k, v)| (k.dupe(), v.clone())).collect();
    let type_named_btree: BTreeMap<Name, NamedSymbol> = info
        .type_named
        .iter()
        .map(|(k, v)| (k.dupe(), v.clone()))
        .collect();
    flow_js_utils::namespace_type(
        cx,
        reason,
        namespace_symbol,
        &named_btree,
        &type_named_btree,
    )
}

pub fn analyze_program<'a>(
    cx: &mut Context<'a>,
    program: &ast::Program<ALoc, (ALoc, Type)>,
) -> Result<(ALoc, ModuleType), flow_utils_concurrency::job_error::JobError> {
    let prog_aloc = &program.loc;
    let mut info = module_info::ModuleInfo {
        kind: module_info::Kind::Unknown,
        type_named: exports::T::new(),
        type_star: Vec::new(),
    };
    for stmt in program.statements.iter() {
        visit_toplevel_statement(cx, &mut info, false, stmt);
    }
    let module_sig_loc = module_exports_sig_loc(&info).unwrap_or_else(|| prog_aloc.dupe());
    let self_reason = mk_reason(None, prog_aloc.dupe());
    let file_loc = ALoc::of_loc(Loc {
        source: Some(cx.file().dupe()),
        ..Loc::default()
    });
    let exports_reason = mk_reason(VirtualReasonDesc::RExports, file_loc);
    let module_t = mk_module_type(cx, &info, self_reason, exports_reason)?;
    Ok((module_sig_loc, module_t))
}

pub fn analyze_declare_namespace<'a>(
    cx: &mut Context<'a>,
    namespace_symbol: Symbol,
    reason: Reason,
    statements: &[ast::statement::Statement<ALoc, (ALoc, Type)>],
    declaration_context: ast_utils::DeclarationContext,
) -> Type {
    let mut info = module_info::ModuleInfo {
        kind: module_info::Kind::Unknown,
        type_named: exports::T::new(),
        type_star: Vec::new(),
    };
    for stmt in statements {
        match ast_utils::acceptable_statement_in_declaration_context(declaration_context, stmt) {
            Ok(()) => {
                visit_toplevel_statement(cx, &mut info, true, stmt);
            }
            Err(kind) => {
                let unsupported_statement = match declaration_context {
                    ast_utils::DeclarationContext::DeclareModule => {
                        ContextDependentUnsupportedStatement::UnsupportedStatementInDeclareModule(
                            FlowSmolStr::from(kind),
                        )
                    }
                    ast_utils::DeclarationContext::DeclareNamespace => {
                        ContextDependentUnsupportedStatement::UnsupportedStatementInDeclareNamespace(
                            FlowSmolStr::from(kind),
                        )
                    }
                    ast_utils::DeclarationContext::DeclareGlobal => {
                        ContextDependentUnsupportedStatement::UnsupportedStatementInDeclareGlobal(
                            FlowSmolStr::from(kind),
                        )
                    }
                };
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EUnsupportedSyntax(Box::new((
                        stmt.loc().dupe(),
                        UnsupportedSyntax::ContextDependentUnsupportedStatement(
                            unsupported_statement,
                        ),
                    ))),
                );
            }
        }
    }
    mk_namespace_t(cx, &info, namespace_symbol, reason)
}
