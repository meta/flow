/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocTable;
use flow_aloc::PackedALocTable;
use flow_common::docblock::Docblock;
use flow_common::files;
use flow_common::flow_import_specifier::FlowImportSpecifier;
use flow_common_modulename::HasteModuleInfo;
use flow_common_modulename::Modulename;
use flow_imports_exports::exports::Exports;
use flow_imports_exports::imports::Imports;
use flow_parser::ast::Program;
use flow_parser::dts_file_kind::DtsFileKind;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc;
use flow_parser_utils::file_sig::FileSig;
use flow_parser_utils::package_json::PackageJson;
use flow_type_sig::packed_type_sig::Module as TypeSigModule;
use flow_type_sig::signature_error::TolerableError;
use flow_type_sig::type_sig_options::TypeSigOptions;

use crate::haste_module::HasteModule;
use crate::parse::FileEntry;
use crate::parse::MergeHashes;
use crate::parse::PackageParse;
use crate::parse::Parse;
use crate::parse::TypedParse;
use crate::resolved_requires::Dependency;
use crate::resolved_requires::DependencyTarget;
use crate::resolved_requires::ResolvedModule;
use crate::resolved_requires::ResolvedRequires;
pub use crate::transaction::ActiveTransaction;
pub use crate::transaction::HashStats;
pub use crate::transaction::Transaction;

impl Transaction {
    pub fn latest_heap_reader(&self) -> crate::transaction::HeapAccess<'_> {
        self.latest_reader()
    }

    pub fn get_haste_info(&self, file: &FileKey) -> Option<HasteModuleInfo> {
        self.latest_reader().reader().get_haste_info(file)
    }

    pub fn get_haste_info_committed(&self, file: &FileKey) -> Option<HasteModuleInfo> {
        self.committed_reader()
            .reader()
            .get_haste_info_committed(file)
    }

    pub fn get_haste_module_info(&self, file: &FileKey) -> Option<HasteModuleInfo> {
        self.get_haste_info(file)
    }

    pub fn get_haste_module(&self, info: &HasteModuleInfo) -> Option<HasteModule> {
        self.latest_reader().reader().get_haste_module(info)
    }

    pub fn get_haste_module_unsafe(&self, info: &HasteModuleInfo) -> HasteModule {
        self.get_haste_module(info)
            .unwrap_or_else(|| panic!("Haste module not found: {:?}", info))
    }

    pub fn get_haste_provider_candidates(&self, info: &HasteModuleInfo) -> Vec<FileKey> {
        self.latest_reader()
            .reader()
            .haste_provider_candidates(info)
            .into_iter()
            .collect()
    }

    pub fn get_haste_dependents(&self, info: &HasteModuleInfo) -> Vec<FileKey> {
        self.latest_reader()
            .reader()
            .haste_dependents(info)
            .into_iter()
            .collect()
    }

    pub fn get_dependency(&self, modulename: &Modulename) -> Option<Dependency> {
        match modulename {
            Modulename::Haste(haste_module_info) => self
                .get_haste_module(haste_module_info)
                .map(|module| module.dependency()),
            Modulename::Filename(file_key) => self
                .latest_reader()
                .reader()
                .file_entry(file_key)
                .map(|entry| entry.dependency()),
        }
    }

    pub fn get_dependency_unsafe(&self, modulename: &Modulename) -> Dependency {
        match modulename {
            Modulename::Haste(haste_module_info) => self
                .get_haste_module(haste_module_info)
                .map(|module| module.dependency())
                .unwrap_or_else(|| panic!("Haste module not found: {:?}", haste_module_info)),
            Modulename::Filename(file_key) => self
                .latest_reader()
                .reader()
                .file_entry(file_key)
                .map(|entry| entry.dependency())
                .unwrap_or_else(|| panic!("File not found: {}", file_key.as_str())),
        }
    }

    pub fn intern_dependency(&self, dependency: Dependency) -> Dependency {
        self.intern_dependency_target(dependency.target_dupe())
    }

    pub fn intern_dependency_from_modulename(&self, modulename: Modulename) -> Dependency {
        self.intern_dependency_target(match modulename {
            Modulename::Haste(info) => DependencyTarget::HasteModule(info),
            Modulename::Filename(file_key) => DependencyTarget::File(file_key),
        })
    }

    pub fn intern_resolved_module(&self, module: ResolvedModule) -> ResolvedModule {
        if let Some(dependency) = module.as_dependency() {
            self.resolved_module_for_dependency(&dependency)
        } else {
            module
        }
    }

    pub fn resolved_module_for_dependency(&self, dependency: &Dependency) -> ResolvedModule {
        ResolvedModule::dependency(self.intern_dependency(dependency.dupe()))
    }

    pub(crate) fn intern_dependency_target(&self, target: DependencyTarget) -> Dependency {
        match target {
            DependencyTarget::HasteModule(haste_info) => {
                self.get_or_create_haste_module(haste_info).dependency()
            }
            DependencyTarget::File(file_key) => Dependency::file(file_key),
        }
    }

    pub fn get_provider(&self, dependency: &Dependency) -> Option<FileKey> {
        self.latest_reader().reader().get_provider(dependency)
    }

    pub fn get_provider_committed(&self, dependency: &Dependency) -> Option<FileKey> {
        self.committed_reader()
            .reader()
            .get_provider_committed(dependency)
    }

    pub fn get_parse(&self, file: &FileKey) -> Option<Parse> {
        self.latest_reader().reader().get_parse(file)
    }

    pub fn get_parse_committed(&self, file: &FileKey) -> Option<Parse> {
        self.committed_reader().reader().get_parse_committed(file)
    }

    pub fn get_dts_file_kind(&self, file: &FileKey) -> Option<DtsFileKind> {
        self.get_parse(file).and_then(|parse| parse.dts_file_kind())
    }

    /// Returns the persisted TypeScript global-augmentation marker for `file`.
    pub fn has_ts_global_augmentation(&self, file: &FileKey) -> bool {
        self.get_parse(file)
            .is_some_and(|parse| parse.has_ts_global_augmentation())
    }

    pub fn get_typed_parse(&self, file: &FileKey) -> Option<TypedParse> {
        self.get_parse(file).and_then(|p| match p {
            Parse::Typed(t) => Some(t),
            _ => None,
        })
    }

    pub fn get_typed_parse_committed(&self, file: &FileKey) -> Option<TypedParse> {
        self.get_parse_committed(file).and_then(|p| match p {
            Parse::Typed(t) => Some(t),
            _ => None,
        })
    }

    pub fn get_package_parse(&self, file: &FileKey) -> Option<PackageParse> {
        self.get_parse(file).and_then(|p| match p {
            Parse::Package(pkg) => Some(pkg),
            _ => None,
        })
    }

    pub fn is_typed_file(&self, file: &FileKey) -> bool {
        self.get_parse(file).is_some_and(|p| p.is_typed())
    }

    pub fn is_package_file(&self, file: &FileKey) -> bool {
        self.get_parse(file).is_some_and(|p| p.is_package())
    }

    pub fn get_leader(&self, file: &FileKey) -> Option<FileKey> {
        self.get_typed_parse(file)
            .and_then(|typed| typed.leader.dupe())
    }

    pub fn has_ast(&self, file: &FileKey) -> bool {
        self.get_typed_parse(file).is_some_and(|t| t.ast.is_some())
    }

    pub fn get_parse_unsafe(&self, file: &FileKey) -> Parse {
        self.get_parse(file)
            .unwrap_or_else(|| panic!("Parse not found for file: {}", file.as_str()))
    }

    pub fn get_typed_parse_unsafe(&self, file: &FileKey) -> TypedParse {
        self.get_typed_parse(file)
            .unwrap_or_else(|| panic!("Typed parse not found for file: {}", file.as_str()))
    }

    pub fn get_package_parse_unsafe(&self, file: &FileKey) -> PackageParse {
        self.get_package_parse(file)
            .unwrap_or_else(|| panic!("Package parse not found for file: {}", file.as_str()))
    }

    pub fn get_package_info(&self, file: &FileKey) -> Option<Arc<PackageJson>> {
        self.get_package_parse(file)
            .map(|pkg| pkg.package_info.dupe())
    }

    pub fn get_package_info_unsafe(&self, file: &FileKey) -> Arc<PackageJson> {
        let pkg = self.get_package_parse_unsafe(file);
        pkg.package_info.dupe()
    }

    pub fn get_file_hash_unsafe(&self, file: &FileKey) -> u64 {
        self.get_parse_unsafe(file).get_file_hash()
    }

    pub fn get_file_hash(&self, file: &FileKey) -> Option<u64> {
        self.get_parse(file).map(|p| p.get_file_hash())
    }

    pub fn get_ast_unsafe(&self, file: &FileKey) -> Arc<Program<Loc, Loc>> {
        if let Some(cached) = self.with_reader_cache(|c| c.get_ast(file)) {
            return cached;
        }
        let typed = self.get_typed_parse_unsafe(file);
        typed.ast_unsafe(file)
    }

    pub fn get_ast(&self, file: &FileKey) -> Option<Arc<Program<Loc, Loc>>> {
        if let Some(cached) = self.with_reader_cache(|c| c.get_ast(file)) {
            return Some(cached);
        }
        self.get_typed_parse(file)
            .and_then(|typed| match &typed.ast {
                Some(bytes) => {
                    let ast = flow_heap_serialization::deserialize_ast(file, bytes);
                    self.with_reader_cache(|c| c.add_ast(file.dupe(), ast.dupe()));
                    Some(ast)
                }
                None => None,
            })
    }

    pub fn get_docblock(&self, file: &FileKey) -> Option<Arc<Docblock>> {
        self.get_typed_parse(file).and_then(|typed| {
            typed
                .docblock
                .as_ref()
                .map(|bytes| flow_heap_serialization::deserialize_docblock(file, bytes))
        })
    }

    pub fn get_docblock_unsafe(&self, file: &FileKey) -> Arc<Docblock> {
        let typed = self.get_typed_parse_unsafe(file);
        typed.docblock_unsafe(file)
    }

    pub fn get_aloc_table_unsafe(&self, file: &FileKey) -> Arc<PackedALocTable> {
        let typed = self.get_typed_parse_unsafe(file);
        typed.aloc_table_unsafe(file)
    }

    pub fn get_aloc_table(&self, file: &FileKey) -> Option<Arc<PackedALocTable>> {
        self.get_typed_parse(file).and_then(|t| {
            t.aloc_table
                .as_ref()
                .map(|bytes| flow_heap_serialization::deserialize_aloc_table(bytes))
        })
    }

    fn get_unpacked_aloc_table(&self, file: &FileKey) -> Option<Arc<ALocTable>> {
        if let Some(cached) = self.with_reader_cache(|c| c.get_aloc_table(file)) {
            return Some(cached);
        }
        self.get_aloc_table(file).map(|packed| {
            let table = Arc::new(ALocTable::unpack(file.dupe(), &packed));
            self.with_reader_cache(|c| c.add_aloc_table(file.dupe(), table.dupe()));
            table
        })
    }

    pub fn loc_of_aloc(&self, aloc: &ALoc) -> Loc {
        if !aloc.is_keyed() {
            return aloc.to_loc_exn().dupe();
        }
        let source = match aloc.source() {
            Some(s) => s.dupe(),
            None => return aloc.to_loc_exn().dupe(),
        };
        match self.get_unpacked_aloc_table(&source) {
            Some(table) => aloc.to_loc_with_table(&table),
            None => aloc.to_loc_exn().dupe(),
        }
    }

    pub fn get_type_sig(&self, file: &FileKey) -> Option<Arc<TypeSigModule<Loc>>> {
        self.get_typed_parse(file).and_then(|typed| {
            typed
                .type_sig
                .as_ref()
                .map(|bytes| flow_heap_serialization::deserialize_type_sig(file, bytes))
        })
    }

    pub fn get_exports(&self, file: &FileKey) -> Option<Arc<Exports>> {
        self.get_typed_parse(file)
            .map(|typed| flow_heap_serialization::deserialize_exports(&typed.exports))
    }

    pub fn get_imports(&self, file: &FileKey) -> Option<Arc<Imports>> {
        self.get_typed_parse(file)
            .map(|typed| flow_heap_serialization::deserialize_imports(&typed.imports))
    }

    pub fn get_tolerable_file_sig(
        &self,
        file: &FileKey,
    ) -> Option<(Arc<FileSig>, Arc<[TolerableError<Loc>]>)> {
        self.get_typed_parse(file).and_then(|typed| {
            typed
                .file_sig
                .as_ref()
                .map(|bytes| flow_heap_serialization::deserialize_file_sig_with_errors(file, bytes))
        })
    }

    pub fn get_file_sig(&self, file: &FileKey) -> Option<Arc<FileSig>> {
        self.get_tolerable_file_sig(file).map(|(sig, _)| sig)
    }

    pub fn get_type_sig_unsafe(&self, file: &FileKey) -> Arc<TypeSigModule<Loc>> {
        let typed = self.get_typed_parse_unsafe(file);
        typed.type_sig_unsafe(file)
    }

    pub fn get_exports_unsafe(&self, file: &FileKey) -> Arc<Exports> {
        let typed = self.get_typed_parse_unsafe(file);
        typed.exports_unsafe()
    }

    pub fn get_imports_unsafe(&self, file: &FileKey) -> Arc<Imports> {
        let typed = self.get_typed_parse_unsafe(file);
        typed.imports_unsafe()
    }

    pub fn get_tolerable_file_sig_unsafe(
        &self,
        file: &FileKey,
    ) -> (Arc<FileSig>, Arc<[TolerableError<Loc>]>) {
        let typed = self.get_typed_parse_unsafe(file);
        typed.tolerable_file_sig_unsafe(file)
    }

    pub fn get_file_sig_unsafe(&self, file: &FileKey) -> Arc<FileSig> {
        self.get_tolerable_file_sig_unsafe(file).0
    }

    pub fn get_requires_unsafe(&self, file: &FileKey) -> Arc<[FlowImportSpecifier]> {
        let typed = self.get_typed_parse_unsafe(file);
        typed.requires.dupe()
    }

    pub fn get_requires(&self, file: &FileKey) -> Option<Arc<[FlowImportSpecifier]>> {
        self.get_typed_parse(file)
            .map(|typed| typed.requires.dupe())
    }

    pub fn get_resolved_requires_unsafe(&self, file: &FileKey) -> ResolvedRequires {
        let typed = self.get_typed_parse_unsafe(file);
        typed.resolved_requires_unsafe()
    }

    pub fn get_resolved_modules_unsafe(
        &self,
        file: &FileKey,
    ) -> BTreeMap<FlowImportSpecifier, Result<Dependency, Option<FlowImportSpecifier>>> {
        let typed = self.get_typed_parse_unsafe(file);
        let requires = &typed.requires;
        let resolved_requires = typed.resolved_requires_unsafe();
        let resolved_modules = resolved_requires.get_resolved_modules();
        requires
            .iter()
            .zip(resolved_modules.iter())
            .map(|(req, module)| (req.dupe(), module.to_result()))
            .collect()
    }

    pub fn get_leader_unsafe(&self, file: &FileKey) -> FileKey {
        let typed = self.get_typed_parse_unsafe(file);
        typed.leader_unsafe()
    }

    pub fn iter_dependents<F>(&self, f: &mut F, modulename: &Modulename)
    where
        F: FnMut(&FileKey),
    {
        for file in self.latest_reader().reader().dependents(modulename) {
            f(&file);
        }
    }

    pub fn get_file_hash_committed(&self, file: &FileKey) -> Option<u64> {
        self.get_parse_committed(file).map(|p| p.get_file_hash())
    }

    pub fn get_exports_committed(&self, file: &FileKey) -> Option<Arc<Exports>> {
        self.get_typed_parse_committed(file)
            .map(|typed| flow_heap_serialization::deserialize_exports(&typed.exports))
    }

    pub fn get_imports_committed(&self, file: &FileKey) -> Option<Arc<Imports>> {
        self.get_typed_parse_committed(file)
            .map(|typed| flow_heap_serialization::deserialize_imports(&typed.imports))
    }

    pub fn get_resolved_requires_committed_unsafe(&self, file: &FileKey) -> ResolvedRequires {
        let typed = self.get_typed_parse_committed(file).unwrap_or_else(|| {
            panic!(
                "Committed typed parse not found for file: {}",
                file.as_str()
            )
        });
        typed.resolved_requires.dupe().unwrap_or_else(|| {
            panic!(
                "Committed resolved requires not found for file: {}",
                file.as_str()
            )
        })
    }

    pub fn get_resolved_modules_committed_unsafe(
        &self,
        file: &FileKey,
    ) -> BTreeMap<FlowImportSpecifier, Result<Dependency, Option<FlowImportSpecifier>>> {
        let typed = self.get_typed_parse_committed(file).unwrap_or_else(|| {
            panic!(
                "Committed typed parse not found for file: {}",
                file.as_str()
            )
        });
        let resolved_requires = typed.resolved_requires.dupe().unwrap_or_else(|| {
            panic!(
                "Committed resolved requires not found for file: {}",
                file.as_str()
            )
        });
        let requires = &typed.requires;
        let resolved_modules = resolved_requires.get_resolved_modules();
        requires
            .iter()
            .zip(resolved_modules.iter())
            .map(|(req, module)| (req.dupe(), module.to_result()))
            .collect()
    }

    // We choose the head file as the leader, and the tail as followers.
    // It is always OK to choose the head as leader, as explained below.
    // Note that cycles cannot happen between untyped files.
    // Why? Because files in cycles must have their dependencies recorded,
    // yet dependencies are never recorded for untyped files.
    // It follows that when the head is untyped, there are no other files.
    // We don't have to worry that some other file may be typed when the head is untyped.
    // It also follows when the head is typed, the tail must be typed too.
    pub fn typed_component(
        &self,
        leader_key: &FileKey,
        rest: &[FileKey],
    ) -> Option<Vec<(FileKey, TypedParse)>> {
        let leader_parse = self.get_typed_parse(leader_key)?;
        let mut component = Vec::with_capacity(1 + rest.len());
        component.push((leader_key.dupe(), leader_parse));
        for key in rest {
            let parse = self.get_typed_parse_unsafe(key);
            component.push((key.dupe(), parse));
        }
        Some(component)
    }

    pub fn file_has_changed(&self, file: &FileKey) -> bool {
        self.latest_reader().reader().file_has_changed(file)
    }

    pub fn get_alternate_file(&self, file: &FileKey) -> Option<FileKey> {
        self.latest_reader()
            .reader()
            .file_entry(file)
            .and_then(|entry| entry.get_alternate_file())
    }

    pub fn set_alternate_file(&self, file: &FileKey, alternate: FileKey) {
        let writer_guard = self.heap_writer();
        let writer = writer_guard.writer();
        writer.update_file_entry(file, |entry| entry.with_alternate_file(Some(alternate)));
    }

    pub fn get_or_create_haste_module(&self, info: HasteModuleInfo) -> HasteModule {
        let writer_guard = self.heap_writer();
        let writer = writer_guard.writer();
        self.get_or_create_haste_module_with_writer(&writer, info)
    }

    fn get_or_create_haste_module_with_writer(
        &self,
        writer: &crate::heap_state::HeapWriter<'_>,
        info: HasteModuleInfo,
    ) -> HasteModule {
        if let Some(module) = writer.reader().get_haste_module(&info) {
            return module;
        }
        let module = HasteModule::new(info.dupe());
        writer.set_haste_module(info, module.dupe());
        self.note_alloc();
        module
    }

    pub fn set_haste_module_provider(&self, info: &HasteModuleInfo, provider: Option<FileKey>) {
        let writer_guard = self.heap_writer();
        let writer = writer_guard.writer();
        let module = writer
            .reader()
            .get_haste_module(info)
            .unwrap_or_else(|| HasteModule::new(info.dupe()));
        writer.set_haste_module(info.dupe(), module.with_provider(provider));
    }

    fn add_haste_provider_candidate(&self, info: &HasteModuleInfo, file: &FileKey) {
        self.get_or_create_haste_module(info.dupe());
        self.heap_writer()
            .writer()
            .add_haste_provider_candidate(info.dupe(), file.dupe());
    }

    fn remove_haste_provider_candidate(&self, info: &HasteModuleInfo, file: &FileKey) {
        let writer_guard = self.heap_writer();
        let writer = writer_guard.writer();
        if let Some(module) = writer.reader().get_haste_module(info)
            && module.get_provider().as_ref() == Some(file)
        {
            writer.set_haste_module(info.dupe(), module.with_provider(None));
        }
        writer.remove_haste_provider_candidate(info.dupe(), file.dupe());
    }

    fn calc_dirty_modules(
        &self,
        file_key: &FileKey,
        old_entry: Option<&FileEntry>,
        new_entry: &FileEntry,
    ) -> BTreeSet<Modulename> {
        let old_info = old_entry.and_then(|entry| entry.get_haste_info());
        let new_info = new_entry.get_haste_info();
        let mut dirty_modules = BTreeSet::new();

        if old_info != new_info {
            if let Some(info) = old_info {
                self.remove_haste_provider_candidate(&info, file_key);
                dirty_modules.insert(Modulename::Haste(info));
            }
            if let Some(info) = new_info {
                self.add_haste_provider_candidate(&info, file_key);
                dirty_modules.insert(Modulename::Haste(info));
            }
        } else if let Some(info) = new_info {
            dirty_modules.insert(Modulename::Haste(info));
        }

        dirty_modules.insert(Modulename::Filename(files::chop_flow_ext(file_key)));

        dirty_modules
    }

    fn handle_flow_ext(&self, file: &FileKey) -> BTreeSet<Modulename> {
        if !files::has_declaration_ext(file) {
            return BTreeSet::new();
        }
        let impl_key = files::chop_declaration_ext(file);
        let writer_guard = self.heap_writer();
        let writer = writer_guard.writer();
        // This is the implementation file's entry, not the one this job was handed: a
        // worker parsing `Foo.js.flow` writes `Foo.js` while another worker may be
        // running `add_parsed` on `Foo.js` itself. Derive and store under its lock.
        let mut inserted = false;
        writer.upsert_file_entry(&impl_key, |entry| match entry {
            Some(entry) => entry.with_alternate_file(Some(file.dupe())),
            None => {
                inserted = true;
                FileEntry::new_empty(impl_key.dupe()).with_alternate_file(Some(file.dupe()))
            }
        });
        if inserted {
            self.note_alloc();
        }
        BTreeSet::new()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_parsed(
        &self,
        file: FileKey,
        file_hash: u64,
        dts_file_kind: Option<flow_parser::dts_file_kind::DtsFileKind>,
        has_ts_global_augmentation: bool,
        haste_module_info: Option<HasteModuleInfo>,
        ast: Option<Arc<Program<Loc, Loc>>>,
        docblock: Option<Arc<Docblock>>,
        aloc_table: Option<Arc<PackedALocTable>>,
        type_sig: Option<Arc<TypeSigModule<Loc>>>,
        type_sig_options: Option<Arc<TypeSigOptions>>,
        file_sig: Option<(Arc<FileSig>, Arc<[TolerableError<Loc>]>)>,
        exports: Arc<Exports>,
        requires: Arc<[FlowImportSpecifier]>,
        imports: Arc<Imports>,
    ) -> BTreeSet<Modulename> {
        let writer_guard = self.heap_writer();
        let writer = writer_guard.writer();
        // Derive and store in one critical section. This writer carries the
        // merge-written fields forward from the entry it reads, so a store built from a
        // stale read would put `leader`, `sig_hash` and `resolved_requires` back to what
        // they were before merge recorded them.
        let mut captured = None;
        writer.upsert_file_entry(&file, |existing_entry| {
            let existing_typed = existing_entry.as_ref().and_then(|entry| {
                entry.parse_latest().and_then(|p| match p {
                    Parse::Typed(t) => Some(t),
                    _ => None,
                })
            });

            let (resolved_requires, leader, sig_hash) = match existing_typed {
                Some(ref existing) => (
                    existing.resolved_requires.dupe(),
                    existing.leader.dupe(),
                    existing.sig_hash,
                ),
                None => (
                    Some(crate::resolved_requires::ResolvedRequires::new(
                        vec![],
                        vec![],
                    )),
                    None,
                    None,
                ),
            };

            let typed_parse = TypedParse::new(
                file_hash,
                dts_file_kind,
                has_ts_global_augmentation,
                ast,
                docblock,
                aloc_table,
                type_sig,
                type_sig_options,
                file_sig,
                exports,
                requires,
                resolved_requires,
                imports,
                leader,
                sig_hash,
            );
            let new_entry = match existing_entry.dupe() {
                Some(entry) => entry
                    .with_parse(Some(Parse::Typed(typed_parse)))
                    .with_haste_info(haste_module_info),
                None => {
                    self.note_alloc();
                    FileEntry::new(
                        file.dupe(),
                        Parse::Typed(typed_parse),
                        haste_module_info.clone(),
                    )
                }
            };
            captured = Some((existing_entry, new_entry.dupe()));
            new_entry
        });
        let (previous_entry, new_entry) = captured.expect("upsert always runs its update");
        let mut dirty_modules = self.calc_dirty_modules(&file, previous_entry.as_ref(), &new_entry);

        dirty_modules.extend(self.handle_flow_ext(&file));
        dirty_modules
    }
    pub fn add_unparsed(
        &self,
        file: FileKey,
        file_hash: u64,
        haste_module_info: Option<HasteModuleInfo>,
    ) -> BTreeSet<Modulename> {
        use crate::parse::UntypedParse;
        let writer_guard = self.heap_writer();
        let writer = writer_guard.writer();
        let existing_entry = writer.reader().file_entry(&file);
        let previous_entry = existing_entry.dupe();

        if let Some(existing_entry) = existing_entry.as_ref() {
            if let Some(Parse::Typed(old_typed)) = existing_entry.parse_latest() {
                if let Some(old_rr) = old_typed.resolved_requires {
                    let old_deps = old_rr.all_dependencies();
                    for dep in &old_deps {
                        self.remove_dependent_from(&writer, &file, dep);
                    }
                }
            }
        }

        let untyped_parse = UntypedParse::new(file_hash);
        let new_entry = match existing_entry {
            Some(entry) => entry
                .with_parse(Some(Parse::Untyped(untyped_parse)))
                .with_haste_info(haste_module_info),
            None => {
                self.note_alloc();
                FileEntry::new(
                    file.dupe(),
                    Parse::Untyped(untyped_parse),
                    haste_module_info.clone(),
                )
            }
        };
        let mut dirty_modules = self.calc_dirty_modules(&file, previous_entry.as_ref(), &new_entry);
        writer.set_file_entry(file.dupe(), new_entry);

        dirty_modules.extend(self.handle_flow_ext(&file));
        dirty_modules
    }

    // If this file used to exist, but no longer does, then it was deleted.
    // Record the deletion by clearing parse information.
    // Deletion might also require re-picking module providers, so we return dirty modules.
    pub fn clear_file(
        &self,
        file_key: FileKey,
        haste_module_info: Option<HasteModuleInfo>,
    ) -> BTreeSet<Modulename> {
        let writer_guard = self.heap_writer();
        let writer = writer_guard.writer();
        if let Some(existing_entry) = writer.reader().file_entry(&file_key) {
            if let Some(Parse::Typed(old_typed)) = existing_entry.parse_latest() {
                if let Some(old_rr) = old_typed.resolved_requires {
                    let old_deps = old_rr.all_dependencies();
                    for dep in &old_deps {
                        self.remove_dependent_from(&writer, &file_key, dep);
                    }
                }
            }
            let mut dirty_modules = BTreeSet::new();
            dirty_modules.insert(Modulename::Filename(files::chop_flow_ext(&file_key)));
            if let Some(haste_info) = existing_entry.get_haste_info() {
                self.remove_haste_provider_candidate(&haste_info, &file_key);
                dirty_modules.insert(Modulename::Haste(haste_info));
            }
            writer.set_file_entry(
                file_key.dupe(),
                existing_entry.with_parse(None).with_haste_info(None),
            );
            dirty_modules
        } else {
            match haste_module_info {
                None => BTreeSet::new(),
                Some(haste_module_info) => {
                    let _m = self.get_or_create_haste_module(haste_module_info.dupe());
                    let mut dirty_modules = BTreeSet::new();
                    dirty_modules.insert(Modulename::Haste(haste_module_info));
                    dirty_modules
                }
            }
        }
    }

    pub fn add_package(
        &self,
        file: FileKey,
        file_hash: u64,
        haste_module_info: Option<HasteModuleInfo>,
        package_info: Arc<PackageJson>,
    ) -> BTreeSet<Modulename> {
        use crate::parse::PackageParse;
        let writer_guard = self.heap_writer();
        let writer = writer_guard.writer();
        let existing_entry = writer.reader().file_entry(&file);
        let previous_entry = existing_entry.dupe();
        let package_parse = PackageParse::new(file_hash, package_info);

        let new_entry = match existing_entry {
            Some(entry) => entry
                .with_parse(Some(Parse::Package(package_parse)))
                .with_haste_info(haste_module_info),
            None => {
                self.note_alloc();
                FileEntry::new(
                    file.dupe(),
                    Parse::Package(package_parse),
                    haste_module_info.clone(),
                )
            }
        };
        let dirty_modules = self.calc_dirty_modules(&file, previous_entry.as_ref(), &new_entry);
        writer.set_file_entry(file.dupe(), new_entry);
        dirty_modules
    }

    // Given a file, it's old resolved requires, and new resolved requires,
    // compute the changes necessary to update the reverse dependency graph.
    pub fn set_resolved_requires(
        &self,
        file: &FileKey,
        resolved_requires: crate::resolved_requires::ResolvedRequires,
    ) {
        let writer_guard = self.heap_writer();
        let writer = writer_guard.writer();
        if let Some(entry) = writer.reader().file_entry(file) {
            if let Some(Parse::Typed(typed)) = entry.parse_latest() {
                let old_deps = typed
                    .resolved_requires
                    .as_ref()
                    .map(|rr| rr.all_dependencies())
                    .unwrap_or_default();
                let new_deps = resolved_requires.all_dependencies();

                writer.set_file_entry(
                    file.dupe(),
                    entry.with_parse(Some(Parse::Typed(
                        typed.with_resolved_requires(resolved_requires),
                    ))),
                );

                let mut new_alloc_size = 0;
                for dep in &old_deps {
                    if new_deps.binary_search(dep).is_err() {
                        self.remove_dependent_from(&writer, file, dep);
                    }
                }

                for dep in &new_deps {
                    if old_deps.binary_search(dep).is_err() {
                        new_alloc_size += self.add_dependent_to(&writer, file, dep);
                    }
                }
                self.note_alloc_many(new_alloc_size);
            }
        }
    }

    fn remove_dependent_from(
        &self,
        writer: &crate::heap_state::HeapWriter<'_>,
        file: &FileKey,
        dep: &Dependency,
    ) {
        match dep.target() {
            DependencyTarget::HasteModule(haste_info) => {
                writer.remove_haste_dependent(haste_info.dupe(), file.dupe());
            }
            DependencyTarget::File(dep_file) => {
                writer.remove_file_dependent(dep_file.dupe(), file.dupe());
            }
        }
    }

    fn add_dependent_to(
        &self,
        writer: &crate::heap_state::HeapWriter<'_>,
        file: &FileKey,
        dep: &Dependency,
    ) -> usize {
        match dep.target() {
            DependencyTarget::HasteModule(haste_info) => {
                self.get_or_create_haste_module_with_writer(writer, haste_info.dupe());
                writer.add_haste_dependent(haste_info.dupe(), file.dupe());
                0
            }
            DependencyTarget::File(dep_file) => {
                writer.add_file_dependent(dep_file.dupe(), file.dupe());
                0
            }
        }
    }

    pub fn update_typed_parse(
        &self,
        file: &FileKey,
        update: impl FnOnce(TypedParse) -> TypedParse,
    ) {
        let writer_guard = self.heap_writer();
        let writer = writer_guard.writer();
        writer.update_file_entry(file, |entry| match entry.parse_latest() {
            Some(Parse::Typed(typed)) => entry.with_parse(Some(Parse::Typed(update(typed)))),
            _ => entry,
        });
    }

    pub fn set_merge_hashes(&self, file: &FileKey, hashes: MergeHashes) {
        self.update_typed_parse(file, |typed| typed.with_merge_hashes(hashes));
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::panic::AssertUnwindSafe;
    use std::sync::Barrier;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use dupe::Dupe;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_parser::file_key::FileKeyInner;

    use super::*;
    use crate::heap_state::CommittedHeap;
    use crate::transaction::GcPhase;

    fn source_file(name: &str) -> FileKey {
        FileKey::new(FileKeyInner::SourceFile(name.to_string()))
    }

    fn committed_heap() -> Arc<CommittedHeap> {
        Arc::new(CommittedHeap::default())
    }

    // These tests hold no caches pointing into the heap, so there is nothing to drop around a
    // compaction.
    fn no_caches_to_drop() {}

    /// Stands in for a dispatcher serving one LSP request. The returned `Arc` is what the
    /// request's typed artifacts keep alive in an IDE cache.
    fn serve_one_request(heap: &Arc<CommittedHeap>) -> Arc<Transaction> {
        let transaction = ActiveTransaction::new(heap.dupe());
        // The workload reads through it and caches what it produced.
        let _ = transaction.get_parse(&source_file("read_by_the_workload.js"));
        let cached = transaction.handle();
        drop(transaction);
        cached
    }

    /// Regression test for the "collating errors" hang.
    ///
    /// A `textDocument/definition` request left its transaction in a thread-local artifact
    /// cache on the parallelizable-workload loop thread. The transaction still held a read
    /// guard on the committed heap, so the next recheck's `apply_commit_deltas` waited on
    /// `state.write()` forever — the server reported "collating errors" indefinitely at 0%
    /// CPU while the guard sat on an idle thread.
    ///
    /// The invariant that prevents it: `ActiveTransaction` releases the long-lived guard when its
    /// workload scope ends. A cache may keep the `Arc<Transaction>` and reacquire the heap for an
    /// individual lazy read, but it never keeps a guard that blocks a recheck.
    #[test]
    fn a_cached_transaction_does_not_block_the_next_recheck() {
        let heap = committed_heap();
        let cached_by_the_ide = serve_one_request(&heap);
        assert_eq!(
            Arc::strong_count(&cached_by_the_ide),
            1,
            "the cache should be the transaction's sole owner"
        );
        assert_eq!(
            heap.state
                .load()
                .read()
                .active_transactions
                .load(Ordering::Acquire),
            0,
            "a cached handle must not keep the transaction active for heap GC"
        );
        let cached_read = cached_by_the_ide.committed_reader();
        assert_eq!(
            heap.state
                .load()
                .read()
                .active_transactions
                .load(Ordering::Acquire),
            1,
            "a detached read must keep the heap active only for the read's lifetime"
        );
        drop(cached_read);
        assert_eq!(
            heap.state
                .load()
                .read()
                .active_transactions
                .load(Ordering::Acquire),
            0,
            "finishing a detached read must release the heap for GC"
        );
        assert!(
            cached_by_the_ide
                .get_parse(&source_file("another_request.js"))
                .is_none(),
            "a cached transaction should remain readable while the committed heap is unchanged"
        );

        // The next recheck publishes.
        let recheck = ActiveTransaction::new(heap.dupe());
        recheck.add_unparsed(source_file("changed.js"), 42, None);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            recheck.commit();
            tx.send(())
                .expect("the commit completion receiver should remain alive");
        });

        assert!(
            rx.recv_timeout(Duration::from_secs(30)).is_ok(),
            "the recheck's commit blocked behind a transaction that an IDE cache is still \
             holding; this is the \"collating errors\" deadlock"
        );
        drop(cached_by_the_ide);
    }

    /// The parse phase is parallel over files, which is only safe while a job writes
    /// the file it was handed. `add_parsed` also writes the *implementation* file behind
    /// a `.js.flow` declaration, via `handle_flow_ext`, so parsing `Foo.js.flow` and
    /// parsing `Foo.js` are two jobs writing one entry. Both must derive and store under
    /// that entry's lock or the later store drops the other's parse.
    #[test]
    fn parsing_a_declaration_and_its_implementation_must_not_clobber_each_other() {
        let heap = committed_heap();
        let implementation = source_file("Foo.js");
        let declaration = source_file("Foo.js.flow");
        let mut lost = 0;
        for _ in 0..200 {
            let transaction = ActiveTransaction::new(heap.dupe());
            let parse = |file: &FileKey| {
                transaction.add_parsed(
                    file.dupe(),
                    1,
                    None,
                    false,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Arc::new(Exports::empty()),
                    Arc::from(Vec::new()),
                    Arc::new(Imports::empty()),
                );
            };
            let start = Barrier::new(2);
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    start.wait();
                    parse(&implementation);
                });
                scope.spawn(|| {
                    start.wait();
                    parse(&declaration);
                });
            });
            let entry_lost = transaction.get_typed_parse(&implementation).is_none();
            let link_lost = transaction
                .get_parse(&implementation)
                .is_some_and(|_| transaction.get_alternate_file(&implementation).is_none());
            if entry_lost || link_lost {
                lost += 1;
            }
        }
        assert_eq!(
            lost, 0,
            "parsing Foo.js and Foo.js.flow concurrently lost one of the two writes \
             {lost}/200 times"
        );
    }

    /// Regression test for the `Leader should be set` crash.
    ///
    /// Merge records a component leader on the file entry while the parse for that same
    /// file is being written. `add_parsed` carries the merge-written fields forward from
    /// the entry it read, so storing a whole entry derived from a read taken before
    /// merge put `leader` back to `None` — and the server then died the next time the
    /// file missed the check cache. Both writers now derive and store under the entry's
    /// lock, so neither can be built from a stale read of the other.
    #[test]
    fn add_parsed_must_not_discard_the_leader_merge_recorded() {
        let heap = committed_heap();
        let file = source_file("C.js");
        let mut lost = 0;
        for _ in 0..200 {
            let transaction = ActiveTransaction::new(heap.dupe());
            let add = |t: &Transaction| {
                t.add_parsed(
                    file.dupe(),
                    1,
                    None,
                    false,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Arc::new(Exports::empty()),
                    Arc::from(Vec::new()),
                    Arc::new(Imports::empty()),
                );
            };
            add(&transaction);
            let start = Barrier::new(2);
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    start.wait();
                    add(&transaction);
                });
                scope.spawn(|| {
                    start.wait();
                    let component = vec![(
                        file.dupe(),
                        transaction.get_typed_parse(&file).expect("parsed"),
                    )];
                    merge_context_mutator::add_merge_on_diff(
                        &transaction,
                        false,
                        &component,
                        11,
                        vec![MergeHashes::CJS {
                            type_export_hashes: Vec::new(),
                            exports_hash: None,
                            ns_hash: 0,
                        }],
                    );
                });
            });
            if transaction.get_leader(&file).is_none() {
                lost += 1;
            }
        }
        assert_eq!(
            lost, 0,
            "add_parsed clobbered merge's leader {lost}/200 times"
        );
    }

    /// The other half of the contract: while a transaction still holds its guard, a commit
    /// waits. That is what keeps a reader from seeing a half-applied commit, and it is why
    /// the guard has to be released explicitly rather than simply never taken.
    #[test]
    fn an_unreleased_transaction_blocks_a_commit() {
        let heap = committed_heap();
        let reading = ActiveTransaction::new(heap.dupe());

        let committing = ActiveTransaction::new(heap.dupe());
        committing.add_unparsed(source_file("b.js"), 42, None);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            committing.commit();
            tx.send(())
                .expect("the commit completion receiver should remain alive");
        });

        assert!(
            rx.recv_timeout(Duration::from_millis(500)).is_err(),
            "a commit must wait while a transaction is still reading the base"
        );
        drop(reading);
        assert!(
            rx.recv_timeout(Duration::from_secs(30)).is_ok(),
            "the commit should proceed once the guard is released"
        );
    }

    #[test]
    fn writes_are_overlay_only_until_commit() {
        let heap = committed_heap();
        let transaction = ActiveTransaction::new(heap.dupe());
        let file = source_file("a.js");

        transaction.add_unparsed(file.dupe(), 1, None);
        assert_eq!(transaction.get_file_hash(&file), Some(1));
        assert_eq!(transaction.get_file_hash_committed(&file), None);
        assert_eq!(heap.heap_size(), 0);

        transaction.commit();

        let reader = ActiveTransaction::new(heap.dupe());
        assert_eq!(reader.get_file_hash(&file), Some(1));
        assert_eq!(heap.heap_size(), 1);
    }

    #[test]
    fn dropping_rolls_back_and_retry_gets_a_fresh_overlay() {
        let heap = committed_heap();
        let file = source_file("a.js");
        let failed = ActiveTransaction::new(heap.dupe());
        failed.add_unparsed(file.dupe(), 1, None);
        drop(failed);

        let retry = ActiveTransaction::new(heap.dupe());
        assert_eq!(retry.get_file_hash(&file), None);
        retry.add_unparsed(file.dupe(), 2, None);
        retry.commit();

        let reader = ActiveTransaction::new(heap.dupe());
        assert_eq!(reader.get_file_hash(&file), Some(2));
    }

    #[test]
    fn commit_rejects_retained_transaction_handles() {
        let heap = committed_heap();
        let transaction = ActiveTransaction::new(heap.dupe());
        let retained = transaction.handle();

        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            transaction.handle().commit(&heap);
        }));
        assert!(result.is_err());
        drop(retained);
        transaction.commit();
    }

    #[test]
    fn gc_stays_idle_while_a_transaction_exists() {
        let heap = committed_heap();
        let seed = ActiveTransaction::new(heap.dupe());
        seed.add_unparsed(source_file("a.js"), 1, None);
        seed.commit();
        heap.state.load().read().gc_state.lock().new_alloc_size = 1;

        let transaction = ActiveTransaction::new(heap.dupe());
        assert!(heap.collect_slice(1, &mut no_caches_to_drop));
        assert_eq!(
            heap.state.load().read().gc_state.lock().phase,
            GcPhase::Idle
        );
        drop(transaction);

        assert!(!heap.collect_slice(1, &mut no_caches_to_drop));
        assert_eq!(
            heap.state.load().read().gc_state.lock().phase,
            GcPhase::Mark
        );
    }

    #[test]
    fn compact_removes_committed_deleted_file() {
        let heap = committed_heap();
        let file = source_file("a.js");
        let insert = ActiveTransaction::new(heap.dupe());
        insert.add_unparsed(file.dupe(), 1, None);
        insert.commit();
        let delete = ActiveTransaction::new(heap.dupe());
        delete.clear_file(file, None);
        delete.commit();

        heap.compact(&mut no_caches_to_drop);
        assert_eq!(heap.heap_size(), 0);
    }

    #[test]
    fn save_heap_and_load_heap_preserve_committed_heap_data() {
        let heap = committed_heap();
        let file = source_file("a.js");
        let haste = HasteModuleInfo::mk(FlowSmolStr::new("A"));
        let write = ActiveTransaction::new(heap.dupe());
        write.add_unparsed(file.dupe(), 42, Some(haste.dupe()));
        write.set_haste_module_provider(&haste, Some(file.dupe()));
        write
            .heap_writer()
            .writer()
            .add_haste_dependent(haste.dupe(), file.dupe());
        write.commit();

        let mut bytes = Vec::new();
        heap.save_heap(&mut bytes).expect("heap should serialize");
        let loaded_heap = Arc::new(
            CommittedHeap::from_heap_dump(&mut Cursor::new(bytes))
                .expect("heap should deserialize"),
        );
        let loaded = ActiveTransaction::new(loaded_heap);

        assert_eq!(loaded.get_file_hash(&file), Some(42));
        assert_eq!(loaded.get_haste_info(&file), Some(haste));
    }

    #[test]
    fn committed_heap_publication_is_atomic_across_maps() {
        let heap = committed_heap();
        let file = source_file("a.js");
        let even_dependent = source_file("even.js");
        let odd_dependent = source_file("odd.js");
        let seed = ActiveTransaction::new(heap.dupe());
        seed.add_unparsed(file.dupe(), 0, None);
        seed.heap_writer()
            .writer()
            .add_file_dependent(file.dupe(), even_dependent.dupe());
        seed.commit();

        let done = Arc::new(AtomicBool::new(false));
        let start = Arc::new(Barrier::new(2));
        let reader_heap = heap.dupe();
        let reader_file = file.dupe();
        let reader_even_dependent = even_dependent.dupe();
        let reader_odd_dependent = odd_dependent.dupe();
        let reader_done = done.dupe();
        let reader_start = start.dupe();
        let reader = std::thread::spawn(move || {
            let mut first_read = true;
            while !reader_done.load(Ordering::Acquire) {
                let state = reader_heap.state.load_full();
                let state = state.read();
                let data = &state.data;
                if first_read {
                    reader_start.wait();
                    first_read = false;
                }
                let file_hash = match data
                    .files
                    .get(&reader_file)
                    .and_then(|entry| entry.parse_latest())
                {
                    Some(Parse::Untyped(parse)) => parse.file_hash,
                    _ => panic!("committed test file should have an untyped parse"),
                };
                let expected_dependent = if file_hash % 2 == 0 {
                    &reader_even_dependent
                } else {
                    &reader_odd_dependent
                };
                let dependents = data
                    .file_dependents
                    .get(&reader_file)
                    .expect("committed test file should have one dependent");
                assert_eq!(
                    dependents.as_slice(),
                    std::slice::from_ref(expected_dependent)
                );
            }
        });

        start.wait();
        for file_hash in 1..=100 {
            let transaction = ActiveTransaction::new(heap.dupe());
            transaction.add_unparsed(file.dupe(), file_hash, None);
            {
                let writer_guard = transaction.heap_writer();
                let writer = writer_guard.writer();
                if file_hash % 2 == 0 {
                    writer.remove_file_dependent(file.dupe(), odd_dependent.dupe());
                    writer.add_file_dependent(file.dupe(), even_dependent.dupe());
                } else {
                    writer.remove_file_dependent(file.dupe(), even_dependent.dupe());
                    writer.add_file_dependent(file.dupe(), odd_dependent.dupe());
                }
            }
            transaction.commit();
        }
        done.store(true, Ordering::Release);
        reader
            .join()
            .expect("atomic committed heap reader should not panic");
    }

    #[test]
    fn transaction_pins_committed_base_until_drop() {
        let heap = committed_heap();
        let file = source_file("a.js");
        let seed = ActiveTransaction::new(heap.dupe());
        seed.add_unparsed(file.dupe(), 1, None);
        seed.commit();

        let reader = ActiveTransaction::new(heap.dupe());
        assert_eq!(reader.get_file_hash(&file), Some(1));

        let writer = ActiveTransaction::new(heap.dupe());
        writer.add_unparsed(file.dupe(), 2, None);
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let writer_thread = std::thread::spawn(move || {
            started_tx
                .send(())
                .expect("test should observe commit starting");
            writer.commit();
            done_tx
                .send(())
                .expect("test should observe commit finishing");
        });

        started_rx
            .recv()
            .expect("writer should report commit starting");
        assert_eq!(reader.get_file_hash(&file), Some(1));
        assert!(
            done_rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "writer should wait for the transaction reading the old base"
        );

        drop(reader);
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("writer should finish after the reader is dropped");
        writer_thread.join().expect("writer should not panic");

        let reader = ActiveTransaction::new(heap);
        assert_eq!(reader.get_file_hash(&file), Some(2));
    }

    #[test]
    fn failed_heap_load_builds_no_heap() {
        let result = CommittedHeap::from_heap_dump(&mut Cursor::new(Vec::<u8>::new()));

        assert!(result.is_err());
    }

    // Reading a dump produces a heap of its own and cannot reach the one in force, so a saved
    // state that turns out to be unusable costs the server nothing.
    #[test]
    fn reading_a_dump_leaves_the_heap_in_force_alone() {
        let heap = committed_heap();
        let committed_file = source_file("committed.js");
        let write = ActiveTransaction::new(heap.dupe());
        write.add_unparsed(committed_file.dupe(), 7, None);
        write.commit();

        let loaded = heap_from_dump_of_one_unparsed_file("saved.js", 42);
        drop(loaded);

        let read = ActiveTransaction::new(heap);
        assert_eq!(
            read.get_file_hash(&committed_file),
            Some(7),
            "the heap in force should still hold what it held"
        );
        assert_eq!(
            read.get_file_hash(&source_file("saved.js")),
            None,
            "an unpublished dump should be visible only in the heap it built"
        );
    }

    // The sequence a reinit from a saved state goes through: the work that decides the saved state
    // is usable, and the recheck that follows it, are committed into the heap the dump built, and
    // only the swap makes any of it the heap the server runs on.
    #[test]
    fn swap_in_installs_the_loaded_heap_and_what_was_committed_into_it() {
        let heap = committed_heap();
        let old_file = source_file("old.js");
        let write = ActiveTransaction::new(heap.dupe());
        write.add_unparsed(old_file.dupe(), 7, None);
        write.commit();

        let loaded = heap_from_dump_of_one_unparsed_file("saved.js", 42);
        let since_saved_state = source_file("since_saved_state.js");
        let reinit = ActiveTransaction::new(loaded.dupe());
        reinit.add_unparsed(since_saved_state.dupe(), 9, None);
        reinit.commit();
        heap.swap_in(loaded);

        let read = ActiveTransaction::new(heap);
        assert_eq!(read.get_file_hash(&source_file("saved.js")), Some(42));
        assert_eq!(
            read.get_file_hash(&since_saved_state),
            Some(9),
            "work committed into the loaded heap before the swap should come with it"
        );
        assert_eq!(
            read.get_file_hash(&old_file),
            None,
            "the installed heap replaces the old one wholesale"
        );
    }

    // Abandoning the transaction abandons the heap it was checking: nothing reaches the heap in
    // force until `CommittedHeap::swap_in`.
    #[test]
    fn dropping_a_loaded_heaps_transaction_publishes_nothing() {
        let heap = committed_heap();
        let old_file = source_file("old.js");
        let write = ActiveTransaction::new(heap.dupe());
        write.add_unparsed(old_file.dupe(), 7, None);
        write.commit();

        let loaded = heap_from_dump_of_one_unparsed_file("saved.js", 42);
        drop(ActiveTransaction::new(loaded));

        let read = ActiveTransaction::new(heap);
        assert_eq!(read.get_file_hash(&old_file), Some(7));
        assert_eq!(read.get_file_hash(&source_file("saved.js")), None);
    }

    // Publishing stores a pointer rather than writing into the heap in force, so a transaction
    // that is already open holds on to the heap it opened on and keeps reading it. That is what
    // lets publishing happen without waiting for every reader to finish.
    #[test]
    fn transaction_open_across_a_publish_keeps_reading_its_own_heap() {
        let heap = committed_heap();
        let old_file = source_file("old.js");
        let write = ActiveTransaction::new(heap.dupe());
        write.add_unparsed(old_file.dupe(), 7, None);
        write.commit();

        let before_publish = ActiveTransaction::new(heap.dupe());

        let loaded = heap_from_dump_of_one_unparsed_file("saved.js", 42);
        heap.swap_in(loaded);

        assert_eq!(before_publish.get_file_hash(&old_file), Some(7));
        assert_eq!(before_publish.get_file_hash(&source_file("saved.js")), None);

        let after_publish = ActiveTransaction::new(heap.dupe());
        assert_eq!(
            after_publish.get_file_hash(&source_file("saved.js")),
            Some(42)
        );
        assert_eq!(after_publish.get_file_hash(&old_file), None);
    }

    fn heap_from_dump_of_one_unparsed_file(name: &str, hash: u64) -> Arc<CommittedHeap> {
        let heap = committed_heap();
        let write = ActiveTransaction::new(heap.dupe());
        write.add_unparsed(source_file(name), hash, None);
        write.commit();
        let mut bytes = Vec::new();
        heap.save_heap(&mut bytes).expect("heap should serialize");
        Arc::new(
            CommittedHeap::from_heap_dump(&mut Cursor::new(bytes))
                .expect("dump should deserialize"),
        )
    }

    #[test]
    fn provider_candidates_follow_latest_haste_info_in_transaction() {
        let transaction = ActiveTransaction::new(committed_heap());
        let file = source_file("a.js");
        let old_haste = HasteModuleInfo::mk(FlowSmolStr::new("Old"));
        let new_haste = HasteModuleInfo::mk(FlowSmolStr::new("New"));

        transaction.add_unparsed(file.dupe(), 1, Some(old_haste.dupe()));
        transaction.add_unparsed(file.dupe(), 2, Some(new_haste.dupe()));

        assert_eq!(
            transaction.get_haste_provider_candidates(&old_haste),
            Vec::<FileKey>::new()
        );
        assert_eq!(
            transaction.get_haste_provider_candidates(&new_haste),
            vec![file.dupe()]
        );

        transaction.add_unparsed(file.dupe(), 3, None);

        assert_eq!(
            transaction.get_haste_provider_candidates(&new_haste),
            Vec::<FileKey>::new()
        );
    }
}

pub mod merge_context_mutator {
    use super::*;

    fn sig_hash_differs(transaction: &Transaction, file: &FileKey, sig_hash: u64) -> bool {
        let prev_sig_hash = transaction
            .get_typed_parse_committed(file)
            .and_then(|parse| parse.sig_hash);

        match prev_sig_hash {
            Some(prev_hash) if prev_hash == sig_hash => false,
            _ => true,
        }
    }

    struct MergeMetadata {
        leader: FileKey,
        sig_hash: Option<Option<u64>>,
        merge_hashes: MergeHashes,
    }

    fn update_merge_metadata(transaction: &Transaction, file: &FileKey, metadata: MergeMetadata) {
        transaction.update_typed_parse(file, |mut parse| {
            parse.leader = Some(metadata.leader);
            if let Some(sig_hash) = metadata.sig_hash {
                parse.sig_hash = sig_hash;
            }
            parse.merge_hashes = Some(Arc::new(metadata.merge_hashes));
            parse
        });
    }

    pub fn add_merge_on_diff(
        transaction: &Transaction,
        for_find_all_refs: bool,
        component: &[(FileKey, TypedParse)],
        sig_hash: u64,
        merge_hashes: Vec<MergeHashes>,
    ) -> bool {
        if component.is_empty() {
            return false;
        }
        assert_eq!(
            component.len(),
            merge_hashes.len(),
            "merge hashes should match component files"
        );
        let (leader_key, _) = &component[0];
        let diff = sig_hash_differs(transaction, leader_key, sig_hash);

        for (i, ((file, _), merge_hashes)) in component.iter().zip(merge_hashes).enumerate() {
            let sig_hash = if diff && !for_find_all_refs {
                if i == 0 {
                    Some(Some(sig_hash))
                } else {
                    Some(None)
                }
            } else {
                None
            };
            update_merge_metadata(
                transaction,
                file,
                MergeMetadata {
                    leader: leader_key.dupe(),
                    sig_hash,
                    merge_hashes,
                },
            );
        }

        diff
    }
}
