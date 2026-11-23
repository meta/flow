/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common_modulename::HasteModuleInfo;
use flow_common_ty::ty::ALocTy;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::statement;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc;
use flow_parser::loc_sig::LocSig;
use flow_type_sig::compact_table::Index;
use flow_type_sig::packed_type_sig;

use crate::autofix_imports;
use crate::insert_type_utils;
use crate::insert_type_utils::error;

#[derive(Debug, Clone)]
pub struct ImportDeclaration {
    pub import_kind: statement::ImportKind,
    pub source: (Loc, ast::StringLiteral<Loc>),
    pub default: Option<ast::Identifier<Loc, Loc>>,
    pub named_specifier: Option<ast::statement::import_declaration::NamedSpecifier<Loc, Loc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UseMode {
    ValueUseMode,
    TypeUseMode,
}

pub mod modules {
    use super::*;

    pub fn resolve(
        file_options: &flow_common::files::FileOptions,
        file: &FileKey,
        module_name: &flow_common_modulename::Modulename,
    ) -> String {
        use flow_common_modulename::Modulename;
        match module_name {
            Modulename::Haste(haste_module_info) => {
                flow_common::files::chop_platform_suffix_for_haste_module(
                    file_options,
                    haste_module_info.module_name().as_str(),
                )
            }
            Modulename::Filename(f) => {
                let f_str = f.as_str().to_string();
                let local_file = std::path::Path::new(&f_str)
                    .file_name()
                    .map_or("", |f| f.to_str().unwrap_or(""))
                    .to_string();
                let dep_folder = std::path::Path::new(&f_str)
                    .parent()
                    .map_or(".", |p| p.to_str().unwrap_or("."))
                    .to_string();
                let this_file_str = file.as_str().to_string();
                let this_folder = std::path::Path::new(&this_file_str)
                    .parent()
                    .map_or(".", |p| p.to_str().unwrap_or("."))
                    .to_string();
                let local_file = if std::path::Path::new(&local_file)
                    .extension()
                    .is_some_and(|e| e == "flow")
                {
                    std::path::Path::new(&local_file)
                        .file_stem()
                        .map_or(local_file.clone(), |s| s.to_str().unwrap_or("").to_string())
                } else {
                    local_file
                };
                let local_file = std::path::Path::new(&local_file)
                    .file_stem()
                    .map_or(local_file.clone(), |s| s.to_str().unwrap_or("").to_string());
                if dep_folder == this_folder {
                    format!("./{local_file}")
                } else {
                    let relative_dir = flow_common::files::relative_path(
                        std::path::Path::new(&this_folder),
                        &dep_folder,
                    );
                    format!("{relative_dir}/{local_file}")
                }
            }
        }
    }
}

pub mod ast_helper {
    use super::*;

    pub fn mk_import_stmt(
        import_kind: statement::ImportKind,
        default: bool,
        remote_name: &str,
        local_name: &str,
        source: &str,
    ) -> ImportDeclaration {
        use dupe::Dupe;
        use flow_data_structure_wrapper::smol_str::FlowSmolStr;
        use flow_parser::ast_utils;
        let dummy_loc: Loc = LocSig::none();
        let remote_id =
            ast_utils::ident_of_source(None, dummy_loc.dupe(), FlowSmolStr::new(remote_name));
        let local_id =
            ast_utils::ident_of_source(None, dummy_loc.dupe(), FlowSmolStr::new(local_name));
        let raw_from_string = |s: &str| -> FlowSmolStr {
            FlowSmolStr::new(format!(
                "\"{}\"",
                s.replace('\\', "\\\\").replace('"', "\\\"")
            ))
        };
        let source_ast = (
            dummy_loc,
            ast::StringLiteral {
                value: FlowSmolStr::new(source),
                raw: raw_from_string(source),
                comments: None,
            },
        );
        let (default_ident, named_specifier) = if default {
            (Some(local_id), None)
        } else {
            let local = if local_id == remote_id {
                None
            } else {
                Some(local_id)
            };
            (
                None,
                Some(ast::statement::import_declaration::NamedSpecifier {
                    kind: None,
                    kind_loc: None,
                    local,
                    remote: remote_id,
                    remote_name_def_loc: None,
                }),
            )
        };
        ImportDeclaration {
            import_kind,
            source: source_ast,
            default: default_ident,
            named_specifier,
        }
    }

    pub fn mk_import_declaration_kind(use_mode: UseMode) -> statement::ImportKind {
        match use_mode {
            UseMode::ValueUseMode => statement::ImportKind::ImportTypeof,
            UseMode::TypeUseMode => statement::ImportKind::ImportType,
        }
    }
}

pub mod exports_helper {
    use super::*;

    #[derive(Debug, Clone)]
    pub struct ImportInfo {
        pub import_kind: statement::ImportKind,
        pub default: bool,
    }

    use flow_type_sig::type_sig;
    use flow_type_sig::type_sig_pack;

    fn packed_ref_name(
        type_sig: &packed_type_sig::Module<Index<ALoc>>,
        ref_: &type_sig_pack::PackedRef<Index<Index<ALoc>>>,
    ) -> flow_data_structure_wrapper::smol_str::FlowSmolStr {
        match ref_ {
            type_sig_pack::PackedRef::LocalRef(deref!(type_sig_pack::PackedRefLocal {
                index, ..
            })) => {
                let def = type_sig.local_defs.get(*index);
                def.name().dupe()
            }
            type_sig_pack::PackedRef::RemoteRef(deref!(type_sig_pack::PackedRefRemote {
                index,
                ..
            })) => {
                let remote_ref = type_sig.remote_refs.get(*index);
                remote_ref.name().dupe()
            }
            type_sig_pack::PackedRef::BuiltinRef(deref!(type_sig_pack::PackedRefBuiltin {
                name,
                ..
            })) => name.dupe(),
        }
    }

    fn from_cjs_module_exports(
        import_kind: statement::ImportKind,
        type_sig: &packed_type_sig::Module<Index<ALoc>>,
        exports: &Option<type_sig_pack::Packed<Index<Index<ALoc>>>>,
        remote_name: &str,
    ) -> Option<ImportInfo> {
        match exports {
            Some(type_sig_pack::Packed::Ref(ref_))
                if packed_ref_name(type_sig, ref_).as_str() == remote_name =>
            {
                Some(ImportInfo {
                    import_kind,
                    default: true,
                })
            }
            Some(type_sig_pack::Packed::TyRef(deref!(type_sig_pack::TyRef::Unqualified(ref_))))
                if packed_ref_name(type_sig, ref_).as_str() == remote_name =>
            {
                let import_kind = statement::ImportKind::ImportTypeof;
                Some(ImportInfo {
                    import_kind,
                    default: true,
                })
            }
            Some(type_sig_pack::Packed::Value(deref!(type_sig::Value::ObjLit(
                deref!(type_sig::ValueObjLit { props, .. }),
            )))) if props.contains_key(remote_name) => Some(ImportInfo {
                import_kind,
                default: false,
            }),
            _ => None,
        }
    }

    fn from_export_keys(
        import_kind: statement::ImportKind,
        keys: &[flow_data_structure_wrapper::smol_str::FlowSmolStr],
        remote_name: &str,
    ) -> Option<ImportInfo> {
        if keys.iter().any(|k| k.as_str() == remote_name) {
            Some(ImportInfo {
                import_kind,
                default: false,
            })
        } else {
            None
        }
    }

    fn from_default_export(
        import_kind: statement::ImportKind,
        type_sig: &packed_type_sig::Module<Index<ALoc>>,
        exports: &[type_sig_pack::Export<Index<Index<ALoc>>>],
        remote_name: &str,
    ) -> Option<ImportInfo> {
        let mut default_index = None;
        for export in exports {
            if let type_sig_pack::Export::ExportDefaultBinding(
                deref!(type_sig_pack::ExportDefaultBindingData { index, .. }),
            ) = export
            {
                default_index = Some(*index);
            }
        }
        match default_index {
            None => None,
            Some(index) => {
                let local_name = type_sig.local_defs.get(index).name();
                if remote_name == local_name.as_str() {
                    Some(ImportInfo {
                        import_kind,
                        default: true,
                    })
                } else {
                    None
                }
            }
        }
    }

    fn from_type_sig(
        import_kind: statement::ImportKind,
        type_sig: &packed_type_sig::Module<Index<ALoc>>,
        remote_name: &str,
    ) -> Option<ImportInfo> {
        match &type_sig.module_kind {
            type_sig_pack::ModuleKind::CJSModule { exports, info, .. } => None
                .or_else(|| from_cjs_module_exports(import_kind, type_sig, exports, remote_name))
                .or_else(|| from_export_keys(import_kind, &info.type_export_keys, remote_name)),
            type_sig_pack::ModuleKind::ESModule { info, exports, .. } => None
                .or_else(|| from_export_keys(import_kind, &info.export_keys, remote_name))
                .or_else(|| from_export_keys(import_kind, &info.type_export_keys, remote_name))
                .or_else(|| from_default_export(import_kind, type_sig, exports, remote_name)),
        }
    }

    // NOTE Here we assume 'react' is available as a library
    fn from_react(loc: &ALoc) -> Option<ImportInfo> {
        if insert_type_utils::is_react_loc(loc, true) {
            Some(ImportInfo {
                import_kind: statement::ImportKind::ImportType,
                default: false,
            })
        } else {
            None
        }
    }

    fn from_react_redux(loc: &ALoc) -> Option<ImportInfo> {
        if insert_type_utils::is_react_redux_loc(loc, true) {
            Some(ImportInfo {
                import_kind: statement::ImportKind::ImportType,
                default: false,
            })
        } else {
            None
        }
    }

    pub fn resolve(
        loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
        get_type_sig: &dyn Fn(&FileKey) -> Option<packed_type_sig::Module<Index<ALoc>>>,
        use_mode: UseMode,
        loc: ALoc,
        name: &str,
        is_lib_file: bool,
    ) -> Result<ImportInfo, error::ImportError> {
        match loc.source() {
            None => Err(error::ImportError::LocSourceNone),
            Some(remote_file) => match get_type_sig(remote_file) {
                None => Err(error::ImportError::NoMatchingExport(
                    name.to_string(),
                    loc_of_aloc(&loc),
                )),
                Some(type_sig) => {
                    let import_kind = ast_helper::mk_import_declaration_kind(use_mode);
                    let import_info_opt = None
                        .or_else(|| from_type_sig(import_kind, &type_sig, name))
                        .or_else(|| is_lib_file.then(|| from_react(&loc)).flatten())
                        .or_else(|| is_lib_file.then(|| from_react_redux(&loc)).flatten());
                    match import_info_opt {
                        None => Err(error::ImportError::NoMatchingExport(
                            name.to_string(),
                            loc_of_aloc(&loc),
                        )),
                        Some(import_info) => Ok(import_info),
                    }
                }
            },
        }
    }
}

pub mod imports_helper {
    use super::*;

    pub mod import_info {

        use flow_common_ty::ty_symbol::ImportMode;
        use flow_common_ty::ty_symbol::ImportedIdent;
        use flow_common_ty::ty_symbol::Provenance;
        use flow_common_ty::ty_symbol::RemoteInfo;
        use flow_common_ty::ty_symbol::Symbol;

        use super::*;

        #[derive(Debug, Clone)]
        pub struct T {
            pub index: usize,
            pub use_mode: UseMode,
            pub remote: Symbol<ALoc>,
            pub import_declaration: ImportDeclaration,
        }

        pub fn to_local_name(
            iteration: usize,
            reserved_names: &std::collections::BTreeSet<String>,
            index: usize,
            use_mode: UseMode,
            name: &str,
        ) -> String {
            // If the name does not appear in the program and this is the first occurrence
            // that we are introducing and in the first iteration, then it's fine to reuse
            // the original import name.
            if !reserved_names.contains(name) && index == 0 && iteration == 0 {
                name.to_string()
            } else {
                let prefix = match use_mode {
                    UseMode::ValueUseMode => "TYPEOF",
                    UseMode::TypeUseMode => "TYPE",
                };
                let iteration_str = match iteration {
                    0 => String::new(),
                    i => format!("{i}$"),
                };
                let index_str = match index {
                    0 => String::new(),
                    _ => format!("_{index}"),
                };
                format!("$IMPORTED_{prefix}${iteration_str}_{name}{index_str}")
            }
        }

        pub fn to_local_symbol(
            iteration: usize,
            reserved_names: &std::collections::BTreeSet<String>,
            x: &T,
        ) -> Symbol<ALoc> {
            let index = x.index;
            let use_mode = x.use_mode;
            match &x.remote {
                Symbol {
                    sym_provenance: Provenance::Remote(RemoteInfo { imported_as: _ }),
                    sym_anonymous: false,
                    sym_def_loc,
                    sym_name,
                } => {
                    let local_name = to_local_name(
                        iteration,
                        reserved_names,
                        index,
                        use_mode,
                        sym_name.as_str(),
                    );
                    let import_mode = match use_mode {
                        UseMode::ValueUseMode => ImportMode::TypeofMode,
                        UseMode::TypeUseMode => ImportMode::TypeMode,
                    };
                    Symbol {
                        sym_provenance: Provenance::Remote(RemoteInfo {
                            imported_as: Some(ImportedIdent(
                                <ALoc as LocSig>::none(),
                                local_name.clone(),
                                import_mode,
                            )),
                        }),
                        sym_anonymous: false,
                        sym_name: FlowSmolStr::new(local_name),
                        sym_def_loc: sym_def_loc.dupe(),
                    }
                }
                Symbol {
                    sym_provenance: Provenance::Library(RemoteInfo { imported_as: _ }),
                    sym_anonymous: false,
                    sym_def_loc,
                    sym_name,
                } if insert_type_utils::is_react_redux_loc(sym_def_loc, true) => {
                    let local_name = to_local_name(
                        iteration,
                        reserved_names,
                        index,
                        use_mode,
                        sym_name.as_str(),
                    );
                    let import_mode = match use_mode {
                        UseMode::ValueUseMode => ImportMode::TypeofMode,
                        UseMode::TypeUseMode => ImportMode::TypeMode,
                    };
                    Symbol {
                        sym_provenance: Provenance::Library(RemoteInfo {
                            imported_as: Some(ImportedIdent(
                                <ALoc as LocSig>::none(),
                                local_name.clone(),
                                import_mode,
                            )),
                        }),
                        sym_anonymous: false,
                        sym_name: FlowSmolStr::new(local_name),
                        sym_def_loc: sym_def_loc.dupe(),
                    }
                }
                s => s.clone(),
            }
        }
    }

    type ImportedNameMap = BTreeMap<FlowSmolStr, Vec<import_info::T>>;

    type SymbolWithUseModeMap = BTreeMap<
        (flow_common_ty::ty_symbol::Symbol<ALoc>, UseMode),
        Result<import_info::T, error::ImportError>,
    >;

    type BatchImportMap = BTreeMap<
        (statement::ImportKind, (Loc, ast::StringLiteral<Loc>)),
        Vec<ast::statement::import_declaration::NamedSpecifier<Loc, Loc>>,
    >;

    type BatchImportBindingsMap =
        BTreeMap<(statement::ImportKind, String), Vec<autofix_imports::NamedBinding>>;

    fn imported_name_map_add(info: import_info::T, m: &mut ImportedNameMap) {
        let name = info.remote.sym_name.dupe();
        m.entry(name).or_insert_with(Vec::new).push(info);
    }

    fn imported_name_map_next_index(
        remote_symbol: &flow_common_ty::ty_symbol::Symbol<ALoc>,
        mode: UseMode,
        m: &ImportedNameMap,
    ) -> usize {
        let sym_name = &remote_symbol.sym_name;
        match m.get(sym_name) {
            None => 0,
            Some(lst) => {
                let max_index = lst
                    .iter()
                    .filter(|info| info.use_mode == mode)
                    .map(|info| info.index)
                    .max()
                    .unwrap_or(0);
                max_index + 1
            }
        }
    }

    fn imported_name_map_fold<A, F>(f: F, m: &ImportedNameMap, mut acc: A) -> A
    where
        F: Fn(&import_info::T, A) -> A,
    {
        for lst in m.values() {
            for info in lst.iter().rev() {
                acc = f(info, acc);
            }
        }
        acc
    }

    pub struct RemoteConverter<'a> {
        loc_of_aloc: Box<dyn Fn(&ALoc) -> Loc + 'a>,
        file_options: Arc<flow_common::files::FileOptions>,
        get_haste_module_info: Box<dyn Fn(&FileKey) -> Option<HasteModuleInfo> + 'a>,
        get_type_sig: Box<dyn Fn(&FileKey) -> Option<packed_type_sig::Module<Index<ALoc>>> + 'a>,
        iteration: usize,
        file: FileKey,
        reserved_names: std::collections::BTreeSet<String>,
        name_map: RefCell<ImportedNameMap>,
        symbol_cache: RefCell<SymbolWithUseModeMap>,
    }

    impl<'a> RemoteConverter<'a> {
        pub fn new(
            loc_of_aloc: Box<dyn Fn(&ALoc) -> Loc + 'a>,
            file_options: Arc<flow_common::files::FileOptions>,
            get_haste_module_info: Box<dyn Fn(&FileKey) -> Option<HasteModuleInfo> + 'a>,
            get_type_sig: Box<
                dyn Fn(&FileKey) -> Option<packed_type_sig::Module<Index<ALoc>>> + 'a,
            >,
            iteration: usize,
            file: FileKey,
            reserved_names: std::collections::BTreeSet<String>,
        ) -> Self {
            Self {
                loc_of_aloc,
                file_options,
                get_haste_module_info,
                get_type_sig,
                iteration,
                file,
                reserved_names,
                name_map: RefCell::new(ImportedNameMap::new()),
                symbol_cache: RefCell::new(SymbolWithUseModeMap::new()),
            }
        }

        fn gen_import_stmt(
            &self,
            index: usize,
            use_mode: UseMode,
            remote_symbol: &flow_common_ty::ty_symbol::Symbol<ALoc>,
        ) -> Result<ImportDeclaration, error::ImportError> {
            use flow_common_modulename::Modulename;
            let remote_name = &remote_symbol.sym_name;
            let sym_def_loc = &remote_symbol.sym_def_loc;
            let is_lib_file = matches!(
                &remote_symbol.sym_provenance,
                flow_common_ty::ty_symbol::Provenance::Library(_)
            );
            let module_name = match sym_def_loc.source() {
                Some(remote_source) => {
                    if insert_type_utils::is_react_file_key(remote_source, is_lib_file) {
                        Ok("react".to_string())
                    } else if insert_type_utils::is_react_redux_file_key(remote_source, is_lib_file)
                    {
                        Ok("react-redux".to_string())
                    } else {
                        let module_name = match (self.get_haste_module_info)(remote_source) {
                            Some(haste_module_info) => Modulename::Haste(haste_module_info),
                            None => Modulename::Filename(flow_common::files::chop_flow_ext(
                                remote_source,
                            )),
                        };
                        Err(module_name)
                    }
                }
                None => return Err(error::ImportError::LocSourceNone),
            };
            let remote_name_str = remote_name.as_str();
            let export_info = exports_helper::resolve(
                &*self.loc_of_aloc,
                &*self.get_type_sig,
                use_mode,
                sym_def_loc.dupe(),
                remote_name_str,
                is_lib_file,
            )?;
            let exports_helper::ImportInfo {
                import_kind,
                default,
            } = export_info;
            let source = match module_name {
                Ok(special_cased) => special_cased,
                Err(module_name) => modules::resolve(&self.file_options, &self.file, &module_name),
            };
            let local_name = import_info::to_local_name(
                self.iteration,
                &self.reserved_names,
                index,
                use_mode,
                remote_name_str,
            );
            Ok(ast_helper::mk_import_stmt(
                import_kind,
                default,
                remote_name_str,
                &local_name,
                &source,
            ))
        }

        fn create_symbol(
            &self,
            use_mode: UseMode,
            remote_symbol: &flow_common_ty::ty_symbol::Symbol<ALoc>,
        ) -> Result<import_info::T, error::ImportError> {
            let index =
                imported_name_map_next_index(remote_symbol, use_mode, &self.name_map.borrow());
            let info_result =
                self.gen_import_stmt(index, use_mode, remote_symbol)
                    .map(|import_declaration| import_info::T {
                        index,
                        use_mode,
                        remote: remote_symbol.clone(),
                        import_declaration,
                    });
            self.symbol_cache
                .borrow_mut()
                .insert((remote_symbol.clone(), use_mode), info_result.clone());
            if let Ok(ref info) = info_result {
                imported_name_map_add(info.clone(), &mut self.name_map.borrow_mut());
            }
            info_result
        }

        fn convert_symbol(
            &self,
            use_mode: UseMode,
            remote_symbol: &flow_common_ty::ty_symbol::Symbol<ALoc>,
        ) -> Result<flow_common_ty::ty_symbol::Symbol<ALoc>, error::ImportError> {
            let cached = self
                .symbol_cache
                .borrow()
                .get(&(remote_symbol.clone(), use_mode))
                .cloned();
            let result = match cached {
                None => self.create_symbol(use_mode, remote_symbol)?,
                Some(info) => info.clone()?,
            };
            Ok(import_info::to_local_symbol(
                self.iteration,
                &self.reserved_names,
                &result,
            ))
        }

        pub fn type_(&self, ty: ALocTy) -> Result<ALocTy, error::Kind> {
            use flow_common_ty::ty;
            use flow_common_ty::ty_symbol::ImportMode;
            use flow_common_ty::ty_symbol::ImportedIdent;
            use flow_common_ty::ty_symbol::Provenance;
            use flow_common_ty::ty_symbol::RemoteInfo;
            use flow_common_ty::ty_symbol::Symbol;

            let old_name_map = self.name_map.borrow().clone();
            let old_symbol_cache = self.symbol_cache.borrow().clone();

            struct ConvertTyVisitor<'a> {
                converter: &'a RemoteConverter<'a>,
                error: Option<error::ImportError>,
            }

            impl flow_common_ty::ty_ancestors::TyEndoBase<(), ALoc> for ConvertTyVisitor<'_> {
                fn on_symbol(&mut self, _env: &(), s: Symbol<ALoc>) -> Symbol<ALoc> {
                    if self.error.is_some() {
                        return s;
                    }
                    let converter = self.converter;
                    match &s {
                        Symbol {
                            sym_provenance: Provenance::Remote(RemoteInfo { imported_as: None }),
                            sym_anonymous: false,
                            ..
                        } => match converter.convert_symbol(UseMode::TypeUseMode, &s) {
                            Ok(sym) => sym,
                            Err(err) => {
                                self.error = Some(err);
                                s
                            }
                        },
                        Symbol {
                            sym_provenance:
                                Provenance::Remote(RemoteInfo {
                                    imported_as: Some(ImportedIdent(_, local_name, _)),
                                }),
                            sym_anonymous: false,
                            sym_def_loc,
                            ..
                        } => Symbol {
                            sym_provenance: Provenance::Local,
                            sym_anonymous: false,
                            sym_name: FlowSmolStr::new(local_name.as_str()),
                            sym_def_loc: sym_def_loc.dupe(),
                        },
                        Symbol {
                            sym_provenance: Provenance::Library(RemoteInfo { imported_as: None }),
                            sym_anonymous: false,
                            sym_def_loc,
                            ..
                        } if insert_type_utils::is_react_redux_loc(sym_def_loc, true) => {
                            match converter.convert_symbol(UseMode::TypeUseMode, &s) {
                                Ok(sym) => sym,
                                Err(err) => {
                                    self.error = Some(err);
                                    s
                                }
                            }
                        }
                        Symbol {
                            sym_provenance:
                                Provenance::Library(RemoteInfo {
                                    imported_as: Some(ImportedIdent(_, local_name, _)),
                                }),
                            sym_anonymous: false,
                            sym_def_loc,
                            ..
                        } if insert_type_utils::is_react_redux_loc(sym_def_loc, true) => Symbol {
                            sym_provenance: Provenance::Local,
                            sym_anonymous: false,
                            sym_name: FlowSmolStr::new(local_name.as_str()),
                            sym_def_loc: sym_def_loc.dupe(),
                        },
                        _ => s,
                    }
                }
            }

            impl flow_common_ty::ty::TyEndoTy<ALoc, ()> for ConvertTyVisitor<'_> {
                fn on_t(&mut self, env: &(), t: Arc<ty::Ty<ALoc>>) -> Arc<ty::Ty<ALoc>> {
                    if self.error.is_some() {
                        return t;
                    }
                    let converter = self.converter;
                    match t.as_ref() {
                        ty::Ty::TypeOf(deref!((ty::BuiltinOrSymbol::TSymbol(s), None)))
                            if !s.sym_anonymous
                                && matches!(
                                    &s.sym_provenance,
                                    Provenance::Remote(RemoteInfo {
                                        imported_as: None
                                            | Some(ImportedIdent(_, _, ImportMode::TypeMode)),
                                    })
                                ) =>
                        {
                            match converter.convert_symbol(UseMode::ValueUseMode, s) {
                                Ok(local) => Arc::new(ty::Ty::Generic(Box::new((
                                    local,
                                    ty::GenKind::ClassKind,
                                    None,
                                )))),
                                Err(err) => {
                                    self.error = Some(err);
                                    t
                                }
                            }
                        }
                        ty::Ty::TypeOf(deref!((
                            ty::BuiltinOrSymbol::TSymbol(Symbol {
                                sym_provenance:
                                    Provenance::Remote(RemoteInfo {
                                        imported_as:
                                            Some(ImportedIdent(
                                                sym_def_loc,
                                                sym_name,
                                                ImportMode::TypeofMode,
                                            )),
                                    }),
                                sym_anonymous: false,
                                ..
                            }),
                            None,
                        ))) => {
                            let local = Symbol {
                                sym_provenance: Provenance::Local,
                                sym_def_loc: sym_def_loc.dupe(),
                                sym_name: FlowSmolStr::new(sym_name.as_str()),
                                sym_anonymous: false,
                            };
                            Arc::new(ty::Ty::Generic(Box::new((
                                local,
                                ty::GenKind::ClassKind,
                                None,
                            ))))
                        }
                        _ => self.default_on_t(env, t),
                    }
                }
            }

            let mut visitor = ConvertTyVisitor {
                converter: self,
                error: None,
            };

            let ty_result =
                <ConvertTyVisitor as ty::TyEndoTy<ALoc, ()>>::on_t(&mut visitor, &(), ty);

            if let Some(err) = visitor.error {
                *self.name_map.borrow_mut() = old_name_map;
                *self.symbol_cache.borrow_mut() = old_symbol_cache;
                Err(error::Kind::ImportError(err))
            } else {
                Ok(ty_result)
            }
        }

        pub fn to_import_stmts(&self) -> Vec<statement::Statement<Loc, Loc>> {
            let dummy_loc: Loc = LocSig::none();
            let (default_list, named_map) = imported_name_map_fold(
                |info, (mut default_acc, mut named_acc): (Vec<ImportDeclaration>, BatchImportMap)| {
                    let import_declaration = &info.import_declaration;
                    let import_kind = import_declaration.import_kind;
                    let source = &import_declaration.source;
                    let default = &import_declaration.default;
                    let named_specifier = &import_declaration.named_specifier;
                    match (default, named_specifier) {
                        (None, Some(named_specifier)) => {
                            named_acc
                                .entry((import_kind, source.clone()))
                                .or_insert_with(Vec::new)
                                .insert(0, named_specifier.clone());
                            (default_acc, named_acc)
                        }
                        _ => {
                            default_acc.insert(0, import_declaration.clone());
                            (default_acc, named_acc)
                        }
                    }
                },
                &self.name_map.borrow(),
                (Vec::new(), BatchImportMap::new()),
            );
            let mut import_stmts: Vec<statement::Statement<Loc, Loc>> = default_list
                .into_iter()
                .map(|import_declaration| {
                    let specifiers = import_declaration.named_specifier.map(|specifier| {
                        ast::statement::import_declaration::Specifier::ImportNamedSpecifiers(vec![
                            specifier,
                        ])
                    });
                    statement::Statement::new(statement::StatementInner::ImportDeclaration {
                        loc: dummy_loc.dupe(),
                        inner: Arc::new(ast::statement::ImportDeclaration {
                            import_kind: import_declaration.import_kind,
                            // Synthesised type imports are never phase-modified.
                            phase: None,
                            source: import_declaration.source,
                            default: import_declaration.default.map(|identifier| {
                                ast::statement::import_declaration::DefaultIdentifier {
                                    identifier,
                                    remote_default_name_def_loc: None,
                                }
                            }),
                            specifiers,
                            attributes: None,
                            comments: None,
                        }),
                    })
                })
                .collect();
            for ((import_kind, source), named_specifiers) in named_map {
                let specifiers = Some(
                    ast::statement::import_declaration::Specifier::ImportNamedSpecifiers(
                        named_specifiers,
                    ),
                );
                let import_stmt =
                    statement::Statement::new(statement::StatementInner::ImportDeclaration {
                        loc: dummy_loc.dupe(),
                        inner: Arc::new(ast::statement::ImportDeclaration {
                            import_kind,
                            phase: None,
                            source,
                            default: None,
                            specifiers,
                            attributes: None,
                            comments: None,
                        }),
                    });
                // `BatchImportMap` (asc-sorted) yields desc-sorted output.
                import_stmts.insert(0, import_stmt);
            }
            import_stmts
        }

        pub fn to_import_bindings(&self) -> Vec<(String, autofix_imports::Bindings)> {
            let (mut bindings, named_map) = imported_name_map_fold(
                |info,
                 (mut default_acc, mut named_acc): (
                    Vec<(String, autofix_imports::Bindings)>,
                    BatchImportBindingsMap,
                )| {
                    let import_declaration = &info.import_declaration;
                    let import_kind = import_declaration.import_kind;
                    let from = import_declaration.source.1.value.to_string();
                    let default = &import_declaration.default;
                    let named_specifier = &import_declaration.named_specifier;
                    match (default, named_specifier) {
                        (None, Some(named_spec)) => {
                            let remote_name = named_spec.remote.name.to_string();
                            let local_name = named_spec.local.as_ref().map(|l| l.name.to_string());
                            let named_binding = autofix_imports::NamedBinding {
                                remote_name,
                                local_name,
                            };
                            named_acc
                                .entry((import_kind, from))
                                .or_insert_with(Vec::new)
                                .push(named_binding);
                            (default_acc, named_acc)
                        }
                        (Some(default_id), _) => {
                            let name = default_id.name.to_string();
                            default_acc.push((from, autofix_imports::Bindings::Default(name)));
                            (default_acc, named_acc)
                        }
                        _ => (default_acc, named_acc),
                    }
                },
                &self.name_map.borrow(),
                (Vec::new(), BatchImportBindingsMap::new()),
            );
            for ((import_kind, from), named_bindings) in named_map {
                match import_kind {
                    statement::ImportKind::ImportType => {
                        bindings.insert(
                            0,
                            (from, autofix_imports::Bindings::NamedType(named_bindings)),
                        );
                    }
                    _ => {
                        bindings.push((from, autofix_imports::Bindings::Named(named_bindings)));
                    }
                }
            }
            bindings
        }
    }
}
