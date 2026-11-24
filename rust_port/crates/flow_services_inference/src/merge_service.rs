/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::Cell;
use std::cell::LazyCell;
use std::cell::OnceCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::rc::Rc;
use std::rc::Weak;
use std::sync::Arc;
use std::time::Instant;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocTable;
use flow_check_cache::CheckCache;
use flow_check_cache::CheckContentsCache;
use flow_check_cache::CheckedContext;
use flow_common::docblock::Docblock;
use flow_common::docblock::FlowMode;
use flow_common::files;
use flow_common::flow_import_specifier::FlowImportSpecifier;
use flow_common::flow_import_specifier::Userland;
use flow_common::options::Options;
use flow_common_cycle_hash as cycle_hash;
use flow_common_utils::graph::GraphLike;
use flow_common_xx as xx;
use flow_common_xx::content_hash_of;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_heap::parse::MergeHashes;
use flow_heap::parse::TypedParse;
use flow_heap::parsing_heaps::Transaction;
use flow_heap::parsing_heaps::merge_context_mutator;
use flow_heap::resolved_requires::ResolvedModule;
use flow_parser::ast;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::FileKeyInner;
use flow_parser::loc::LOC_NONE;
use flow_parser::loc::Loc;
use flow_parser_utils::file_sig::FileSig;
use flow_services_coverage::FileCoverage;
use flow_services_coverage::file_coverage;
use flow_type_sig::compact_table::Index;
use flow_type_sig::compact_table::Table;
use flow_type_sig::type_sig_hash;
use flow_type_sig::type_sig_hash::CheckedDep;
use flow_type_sig::type_sig_hash::Dependency;
use flow_type_sig::type_sig_hash::ReadHash;
use flow_type_sig::type_sig_options::TypeSigOptions;
use flow_type_sig::type_sig_pack::ModuleKind;
use flow_typing::merge::get_lint_severities;
use flow_typing::type_inference::scan_for_suppressions;
use flow_typing_context::Context;
use flow_typing_context::ContextArena;
use flow_typing_context::MasterContext;
use flow_typing_context::Metadata;
use flow_typing_context::OverridableMetadata;
use flow_typing_errors::error_message::InternalError;
use flow_typing_errors::error_suppressions::ErrorSuppressions;
use flow_typing_errors::flow_error::ErrorSet;
use flow_typing_type::type_::Type;
use flow_utils_concurrency::thread_pool::ThreadPool;
use flow_utils_concurrency::worker_cancel;
use vec1::Vec1;

use crate::check_service;
use crate::inference_utils;
use crate::merge_stream::Component;
use crate::merge_stream::MergeResult;
use crate::merge_stream::MergeStream;
use crate::obj_to_obj_hook;

pub type UnitResult<A> = Result<A, (ALoc, InternalError)>;

pub struct SigOptsData {
    pub skipped_count: usize,
    pub sig_new_or_changed: FlowOrdSet<FileKey>,
}

pub type MergeResults<A> = (Vec<A>, SigOptsData);

pub fn sig_hash(
    check_dirty_set: bool,
    _root: &Path,
    transaction: &Transaction,
    component: &[(FileKey, TypedParse)],
) -> (u64, Vec<MergeHashes>) {
    type ComponentRec = Rc<OnceCell<Vec<Rc<CheckedDep<Rc<cycle_hash::Node>>>>>>;

    fn upgrade_file(file: &Weak<type_sig_hash::File>) -> Rc<type_sig_hash::File> {
        file.upgrade()
            .expect("signature hash file should remain alive while it is traversed")
    }

    fn hash_file_key(file_key: &FileKey) -> u64 {
        xx::hash(file_key.as_str().as_bytes(), 0)
    }

    let resource_dep = |f: &FileKey| -> Dependency {
        let ext = std::path::Path::new(f.as_str())
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .expect("resource file without extension");
        let hash = xx::hash(ext.as_bytes(), 0);
        Dependency::Resource(Box::new(move || hash))
    };

    let acyclic_dep = |dep_key: &FileKey, dep_parse: &TypedParse| -> CheckedDep<ReadHash> {
        let filename_hash = hash_file_key(dep_key);
        let filename: ReadHash = Box::new(move || filename_hash);

        match dep_parse.get_merge_hashes() {
            Some(MergeHashes::CJS {
                type_export_hashes,
                exports_hash,
                ns_hash,
            }) => {
                let type_exports = type_export_hashes
                    .into_iter()
                    .map(|(k, h)| {
                        let rh: ReadHash = Box::new(move || h);
                        (k, rh)
                    })
                    .collect();
                let exports = exports_hash.map(|h| {
                    let rh: ReadHash = Box::new(move || h);
                    rh
                });
                let ns_h = ns_hash;
                let ns: ReadHash = Box::new(move || ns_h);
                CheckedDep::CJS {
                    filename,
                    type_exports,
                    exports,
                    ns,
                }
            }
            Some(MergeHashes::ES {
                type_export_hashes,
                export_hashes,
                ns_hash,
            }) => {
                let type_exports = type_export_hashes
                    .into_iter()
                    .map(|(k, h)| {
                        let rh: ReadHash = Box::new(move || h);
                        (k, rh)
                    })
                    .collect();
                let exports = export_hashes
                    .into_iter()
                    .map(|(k, h)| {
                        let rh: ReadHash = Box::new(move || h);
                        (k, rh)
                    })
                    .collect();
                let ns_h = ns_hash;
                let ns: ReadHash = Box::new(move || ns_h);
                CheckedDep::ES {
                    filename,
                    type_exports,
                    exports,
                    ns,
                }
            }
            None => {
                fn structural_hash<T: std::hash::Hash>(item: &T) -> ReadHash {
                    let hash = content_hash_of(item);
                    Box::new(move || hash)
                }
                let module = dep_parse.type_sig_unsafe(dep_key);
                match &module.module_kind {
                    ModuleKind::CJSModule {
                        type_exports,
                        exports,
                        info,
                    } => {
                        let te = info
                            .type_export_keys
                            .iter()
                            .enumerate()
                            .map(|(i, key)| (key.dupe(), structural_hash(&type_exports[i])))
                            .collect();
                        let exp = exports.as_ref().map(|exp| structural_hash(exp));
                        let ns = structural_hash(info);
                        CheckedDep::CJS {
                            filename,
                            type_exports: te,
                            exports: exp,
                            ns,
                        }
                    }
                    ModuleKind::ESModule {
                        type_exports,
                        exports,
                        ts_pending: _,
                        info,
                    } => {
                        let te = info
                            .type_export_keys
                            .iter()
                            .enumerate()
                            .map(|(i, key)| (key.dupe(), structural_hash(&type_exports[i])))
                            .collect();
                        let exp = info
                            .export_keys
                            .iter()
                            .enumerate()
                            .map(|(i, key)| (key.dupe(), structural_hash(&exports[i])))
                            .collect();
                        let ns = structural_hash(info);
                        CheckedDep::ES {
                            filename,
                            type_exports: te,
                            exports: exp,
                            ns,
                        }
                    }
                }
            }
        }
    };

    let cyclic_dep = |file_key: &FileKey,
                      parse: &TypedParse,
                      file: &Rc<type_sig_hash::File>|
     -> CheckedDep<Rc<cycle_hash::Node>> {
        let filename_hash = hash_file_key(file_key);
        let filename: ReadHash = Box::new(move || filename_hash);
        let module = parse.type_sig_unsafe(file_key);

        match &module.module_kind {
            ModuleKind::CJSModule {
                type_exports,
                exports,
                info,
            } => {
                let type_export_nodes: Vec<Rc<cycle_hash::Node>> = type_exports
                    .iter()
                    .map(|texport| {
                        let init_hash = content_hash_of(texport);
                        let hash = Rc::new(Cell::new(init_hash));
                        let hash_r = hash.dupe();
                        let read_hash: ReadHash = Box::new(move || hash_r.get());
                        let hash_w = hash.dupe();
                        let write_hash: cycle_hash::WriteHash = Box::new(move |h| hash_w.set(h));
                        let texport = texport.clone();
                        let file = Rc::downgrade(file);
                        let visit: Box<dyn Fn(&dyn Fn(Rc<cycle_hash::Node>), &dyn Fn(&ReadHash))> =
                            Box::new(move |edge, _dep_edge| {
                                let file = upgrade_file(&file);
                                type_sig_hash::visit_type_export(edge, &file, &texport);
                            });
                        Rc::new(cycle_hash::create_node(visit, read_hash, write_hash))
                    })
                    .collect();

                let exports_node: Option<Rc<cycle_hash::Node>> = exports.as_ref().map(|packed| {
                    let init_hash = content_hash_of(packed);
                    let hash = Rc::new(Cell::new(init_hash));
                    let hash_r = hash.dupe();
                    let read_hash: ReadHash = Box::new(move || hash_r.get());
                    let hash_w = hash.dupe();
                    let write_hash: cycle_hash::WriteHash = Box::new(move |h| hash_w.set(h));
                    let packed = packed.clone();
                    let file = Rc::downgrade(file);
                    let visit: Box<dyn Fn(&dyn Fn(Rc<cycle_hash::Node>), &dyn Fn(&ReadHash))> =
                        Box::new(move |edge, dep_edge| {
                            let file = upgrade_file(&file);
                            type_sig_hash::visit_packed(edge, dep_edge, &file, &packed);
                        });
                    Rc::new(cycle_hash::create_node(visit, read_hash, write_hash))
                });

                let ns_init_hash = content_hash_of(info);
                let ns_hash = Rc::new(Cell::new(ns_init_hash));
                let ns_hash_r = ns_hash.dupe();
                let ns_read: ReadHash = Box::new(move || ns_hash_r.get());
                let ns_hash_w = ns_hash.dupe();
                let ns_write: cycle_hash::WriteHash = Box::new(move |h| ns_hash_w.set(h));
                let te_nodes_for_ns = type_export_nodes.clone();
                let exp_node_for_ns = exports_node.clone();
                let type_stars = info.type_stars.clone();
                let file_for_ns = Rc::downgrade(file);
                let ns_visit: Box<dyn Fn(&dyn Fn(Rc<cycle_hash::Node>), &dyn Fn(&ReadHash))> =
                    Box::new(move |edge, dep_edge| {
                        for te in &te_nodes_for_ns {
                            edge(te.dupe());
                        }
                        if let Some(exp) = &exp_node_for_ns {
                            edge(exp.dupe());
                        }
                        let file_for_ns = upgrade_file(&file_for_ns);
                        for (_, index) in &type_stars {
                            type_sig_hash::edge_import_ns(edge, dep_edge, &file_for_ns, *index);
                        }
                    });
                let ns = Rc::new(cycle_hash::create_node(ns_visit, ns_read, ns_write));

                let type_export_map: BTreeMap<FlowSmolStr, Rc<cycle_hash::Node>> = info
                    .type_export_keys
                    .iter()
                    .zip(type_export_nodes)
                    .map(|(key, node)| (key.dupe(), node))
                    .collect();

                CheckedDep::CJS {
                    filename,
                    type_exports: type_export_map,
                    exports: exports_node,
                    ns,
                }
            }

            ModuleKind::ESModule {
                type_exports,
                exports,
                ts_pending,
                info,
            } => {
                let type_export_nodes: Vec<Rc<cycle_hash::Node>> = type_exports
                    .iter()
                    .map(|texport| {
                        let init_hash = content_hash_of(texport);
                        let hash = Rc::new(Cell::new(init_hash));
                        let hash_r = hash.dupe();
                        let read_hash: ReadHash = Box::new(move || hash_r.get());
                        let hash_w = hash.dupe();
                        let write_hash: cycle_hash::WriteHash = Box::new(move |h| hash_w.set(h));
                        let texport = texport.clone();
                        let file = Rc::downgrade(file);
                        let visit: Box<dyn Fn(&dyn Fn(Rc<cycle_hash::Node>), &dyn Fn(&ReadHash))> =
                            Box::new(move |edge, _dep_edge| {
                                let file = upgrade_file(&file);
                                type_sig_hash::visit_type_export(edge, &file, &texport);
                            });
                        Rc::new(cycle_hash::create_node(visit, read_hash, write_hash))
                    })
                    .collect();

                let export_nodes: Vec<Rc<cycle_hash::Node>> = exports
                    .iter()
                    .map(|export| {
                        let init_hash = content_hash_of(export);
                        let hash = Rc::new(Cell::new(init_hash));
                        let hash_r = hash.dupe();
                        let read_hash: ReadHash = Box::new(move || hash_r.get());
                        let hash_w = hash.dupe();
                        let write_hash: cycle_hash::WriteHash = Box::new(move |h| hash_w.set(h));
                        let export = export.clone();
                        let file = Rc::downgrade(file);
                        let visit: Box<dyn Fn(&dyn Fn(Rc<cycle_hash::Node>), &dyn Fn(&ReadHash))> =
                            Box::new(move |edge, dep_edge| {
                                let file = upgrade_file(&file);
                                type_sig_hash::visit_export(edge, dep_edge, &file, &export);
                            });
                        Rc::new(cycle_hash::create_node(visit, read_hash, write_hash))
                    })
                    .collect();

                let ts_pending_nodes: Vec<Rc<cycle_hash::Node>> = ts_pending
                    .iter()
                    .map(|pending| {
                        let init_hash = content_hash_of(pending);
                        let hash = Rc::new(Cell::new(init_hash));
                        let hash_r = hash.dupe();
                        let read_hash: ReadHash = Box::new(move || hash_r.get());
                        let hash_w = hash.dupe();
                        let write_hash: cycle_hash::WriteHash = Box::new(move |h| hash_w.set(h));
                        let pending = pending.clone();
                        let file = Rc::downgrade(file);
                        let visit: Box<dyn Fn(&dyn Fn(Rc<cycle_hash::Node>), &dyn Fn(&ReadHash))> =
                            Box::new(move |edge, dep_edge| {
                                let file = upgrade_file(&file);
                                type_sig_hash::visit_ts_pending_export(
                                    edge, dep_edge, &file, &pending,
                                );
                            });
                        Rc::new(cycle_hash::create_node(visit, read_hash, write_hash))
                    })
                    .collect();

                let ns_init_hash = content_hash_of(info);
                let ns_hash = Rc::new(Cell::new(ns_init_hash));
                let ns_hash_r = ns_hash.dupe();
                let ns_read: ReadHash = Box::new(move || ns_hash_r.get());
                let ns_hash_w = ns_hash.dupe();
                let ns_write: cycle_hash::WriteHash = Box::new(move |h| ns_hash_w.set(h));
                let te_nodes_for_ns = type_export_nodes.clone();
                let exp_nodes_for_ns = export_nodes.clone();
                let ts_pending_nodes_for_ns = ts_pending_nodes.clone();
                let type_stars = info.type_stars.clone();
                let stars = info.stars.clone();
                let file_for_ns = Rc::downgrade(file);
                let ns_visit: Box<dyn Fn(&dyn Fn(Rc<cycle_hash::Node>), &dyn Fn(&ReadHash))> =
                    Box::new(move |edge, dep_edge| {
                        for te in &te_nodes_for_ns {
                            edge(te.dupe());
                        }
                        for exp in &exp_nodes_for_ns {
                            edge(exp.dupe());
                        }
                        for tp in &ts_pending_nodes_for_ns {
                            edge(tp.dupe());
                        }
                        let file_for_ns = upgrade_file(&file_for_ns);
                        for (_, index) in &type_stars {
                            type_sig_hash::edge_import_ns(edge, dep_edge, &file_for_ns, *index);
                        }
                        for (_, index) in &stars {
                            type_sig_hash::edge_import_ns(edge, dep_edge, &file_for_ns, *index);
                        }
                    });
                let ns = Rc::new(cycle_hash::create_node(ns_visit, ns_read, ns_write));

                let type_export_map: BTreeMap<FlowSmolStr, Rc<cycle_hash::Node>> = info
                    .type_export_keys
                    .iter()
                    .zip(type_export_nodes)
                    .map(|(key, node)| (key.dupe(), node))
                    .collect();

                let export_map: BTreeMap<FlowSmolStr, Rc<cycle_hash::Node>> = info
                    .export_keys
                    .iter()
                    .zip(export_nodes)
                    .map(|(key, node)| (key.dupe(), node))
                    .collect();

                CheckedDep::ES {
                    filename,
                    type_exports: type_export_map,
                    exports: export_map,
                    ns,
                }
            }
        }
    };

    let file_dependency = |component_rec: &ComponentRec,
                           component_map: &BTreeMap<FileKey, usize>,
                           resolved_module: &ResolvedModule|
     -> Dependency {
        match resolved_module.to_result() {
            Err(_) => Dependency::Unchecked,
            Ok(dep_ref) => match transaction.get_provider(&dep_ref) {
                None => Dependency::Unchecked,
                Some(dep_file) => {
                    if matches!(dep_file.inner(), FileKeyInner::ResourceFile(_)) {
                        resource_dep(&dep_file)
                    } else {
                        match transaction.get_typed_parse(&dep_file) {
                            None => Dependency::Unchecked,
                            Some(dep_parse) => {
                                if let Some(&i) = component_map.get(&dep_file) {
                                    let component_rec = Rc::downgrade(component_rec);
                                    Dependency::Cyclic(LazyCell::new(Box::new(move || {
                                        component_rec
                                            .upgrade()
                                            .expect("signature hash component should remain alive while it is traversed")
                                            .get()
                                            .expect("signature hash component should be initialized before traversal")[i]
                                            .dupe()
                                    })))
                                } else {
                                    let dep_key = dep_file.dupe();
                                    Dependency::Acyclic(LazyCell::new(Box::new(move || {
                                        acyclic_dep(&dep_key, &dep_parse)
                                    })))
                                }
                            }
                        }
                    }
                }
            },
        }
    };

    let component_file = |component_rec: &ComponentRec,
                          component_map: &BTreeMap<FileKey, usize>,
                          file_key: &FileKey,
                          parse: &TypedParse|
     -> (
        Rc<CheckedDep<Rc<cycle_hash::Node>>>,
        Rc<type_sig_hash::File>,
    ) {
        let module = parse.type_sig_unsafe(file_key);

        let resolved_modules_map: BTreeMap<Userland, ResolvedModule> = {
            let requires = parse.requires();
            let resolved_requires = parse.resolved_requires_unsafe();
            let resolved = resolved_requires.get_resolved_modules();
            requires
                .iter()
                .zip(resolved.iter())
                .map(|(spec, resolved)| {
                    let FlowImportSpecifier::Userland(u) = spec;
                    (u.dupe(), resolved.clone())
                })
                .collect()
        };

        let dependencies: Table<Dependency> = Table::init(module.module_refs.len(), |i| {
            let mref = module.module_refs.get(Index::<Userland>::new(i));
            match resolved_modules_map.get(mref) {
                Some(resolved) => file_dependency(component_rec, component_map, resolved),
                None => Dependency::Unchecked,
            }
        });

        let file = Rc::new_cyclic(|file: &Weak<type_sig_hash::File>| {
            let local_defs = {
                let dirty_indices = &module.dirty_local_defs;
                Table::init(module.local_defs.len(), |i| {
                    let def = module.local_defs.get(Index::<()>::new(i));
                    let mut hash_val = content_hash_of(def);
                    if check_dirty_set && dirty_indices.contains(&i) {
                        hash_val = !hash_val;
                    }
                    let hash = Rc::new(Cell::new(hash_val));
                    let hash_r = hash.dupe();
                    let read_hash: ReadHash = Box::new(move || hash_r.get());
                    let hash_w = hash.dupe();
                    let write_hash: cycle_hash::WriteHash = Box::new(move |h| hash_w.set(h));
                    let def = def.clone();
                    let file = file.clone();
                    let visit: Box<dyn Fn(&dyn Fn(Rc<cycle_hash::Node>), &dyn Fn(&ReadHash))> =
                        Box::new(move |edge, dep_edge| {
                            let file = upgrade_file(&file);
                            type_sig_hash::visit_def(edge, dep_edge, &file, &def);
                        });
                    Rc::new(cycle_hash::create_node(visit, read_hash, write_hash))
                })
            };

            let remote_refs = Table::init(module.remote_refs.len(), |i| {
                let rref = module.remote_refs.get(Index::<()>::new(i));
                let hash_val = content_hash_of(rref);
                let hash = Rc::new(Cell::new(hash_val));
                let hash_r = hash.dupe();
                let read_hash: ReadHash = Box::new(move || hash_r.get());
                let hash_w = hash.dupe();
                let write_hash: cycle_hash::WriteHash = Box::new(move |h| hash_w.set(h));
                let rref = rref.clone();
                let file = file.clone();
                let visit: Box<dyn Fn(&dyn Fn(Rc<cycle_hash::Node>), &dyn Fn(&ReadHash))> =
                    Box::new(move |edge, dep_edge| {
                        let file = upgrade_file(&file);
                        type_sig_hash::visit_remote_ref(edge, dep_edge, &file, &rref);
                    });
                Rc::new(cycle_hash::create_node(visit, read_hash, write_hash))
            });

            let pattern_defs = {
                let dirty_indices = &module.dirty_pattern_defs;
                Table::init(module.pattern_defs.len(), |i| {
                    let pdef = module.pattern_defs.get(Index::<()>::new(i));
                    let mut hash_val = content_hash_of(pdef);
                    if check_dirty_set && dirty_indices.contains(&i) {
                        hash_val = !hash_val;
                    }
                    let hash = Rc::new(Cell::new(hash_val));
                    let hash_r = hash.dupe();
                    let read_hash: ReadHash = Box::new(move || hash_r.get());
                    let hash_w = hash.dupe();
                    let write_hash: cycle_hash::WriteHash = Box::new(move |h| hash_w.set(h));
                    let pdef = pdef.clone();
                    let file = file.clone();
                    let visit: Box<dyn Fn(&dyn Fn(Rc<cycle_hash::Node>), &dyn Fn(&ReadHash))> =
                        Box::new(move |edge, dep_edge| {
                            let file = upgrade_file(&file);
                            type_sig_hash::visit_packed(edge, dep_edge, &file, &pdef);
                        });
                    Rc::new(cycle_hash::create_node(visit, read_hash, write_hash))
                })
            };

            let patterns = Table::init(module.patterns.len(), |i| {
                let pattern = module.patterns.get(Index::<()>::new(i));
                let hash_val = content_hash_of(pattern);
                let hash = Rc::new(Cell::new(hash_val));
                let hash_r = hash.dupe();
                let read_hash: ReadHash = Box::new(move || hash_r.get());
                let hash_w = hash.dupe();
                let write_hash: cycle_hash::WriteHash = Box::new(move |h| hash_w.set(h));
                let pattern = pattern.clone();
                let file = file.clone();
                let visit: Box<dyn Fn(&dyn Fn(Rc<cycle_hash::Node>), &dyn Fn(&ReadHash))> =
                    Box::new(move |edge, _dep_edge| {
                        let file = upgrade_file(&file);
                        type_sig_hash::visit_pattern(edge, &file, &pattern);
                    });
                Rc::new(cycle_hash::create_node(visit, read_hash, write_hash))
            });

            type_sig_hash::File {
                dependencies,
                local_defs,
                remote_refs,
                pattern_defs,
                patterns,
            }
        });

        (Rc::new(cyclic_dep(file_key, parse, &file)), file)
    };

    if component.is_empty() {
        return (0u64, Vec::new());
    }

    let component_map: BTreeMap<FileKey, usize> = component
        .iter()
        .enumerate()
        .map(|(i, (f, _))| (f.dupe(), i))
        .collect();

    let component_rec: ComponentRec = Rc::new(OnceCell::new());
    let component_files: Vec<(
        Rc<CheckedDep<Rc<cycle_hash::Node>>>,
        Rc<type_sig_hash::File>,
    )> = component
        .iter()
        .map(|(file_key, parse)| component_file(&component_rec, &component_map, file_key, parse))
        .collect();
    let files = component_files
        .iter()
        .map(|(checked_dep, _file)| checked_dep.dupe())
        .collect();
    assert!(component_rec.set(files).is_ok(), "Should be initialized");

    let cx = cycle_hash::create_cx();
    // `component_rec` was built by mapping over `component`, so the two zip in step.
    let component_hash =
        component_sig_hash(component.iter().zip(component_rec.get().unwrap()).map(
            |((file_key, _), checked_dep)| match checked_dep.as_ref() {
                CheckedDep::CJS { ns, .. } | CheckedDep::ES { ns, .. } => {
                    cycle_hash::root(&cx, ns);
                    (file_key, cycle_hash::read_hash(ns))
                }
            },
        ));

    let merge_hashes = component_rec
        .get()
        .unwrap()
        .iter()
        .map(|checked_dep| match checked_dep.as_ref() {
            CheckedDep::CJS {
                type_exports,
                exports,
                ns,
                ..
            } => MergeHashes::CJS {
                type_export_hashes: type_exports
                    .iter()
                    .map(|(k, node)| (k.dupe(), cycle_hash::read_hash(node)))
                    .collect(),
                exports_hash: exports.as_ref().map(|node| cycle_hash::read_hash(node)),
                ns_hash: cycle_hash::read_hash(ns),
            },
            CheckedDep::ES {
                type_exports,
                exports,
                ns,
                ..
            } => MergeHashes::ES {
                type_export_hashes: type_exports
                    .iter()
                    .map(|(k, node)| (k.dupe(), cycle_hash::read_hash(node)))
                    .collect(),
                export_hashes: exports
                    .iter()
                    .map(|(k, node)| (k.dupe(), cycle_hash::read_hash(node)))
                    .collect(),
                ns_hash: cycle_hash::read_hash(ns),
            },
        })
        .collect();

    debug_assert_eq!(Rc::strong_count(&component_rec), 1);
    debug_assert!(
        component_files
            .iter()
            .all(|(_checked_dep, file)| Rc::strong_count(file) == 1)
    );

    (component_hash, merge_hashes)
}

/// A component's signature hash, from each of its files' namespace hashes.
///
/// It changes whenever any file's namespace hash does, whatever the order of the files. The files
/// of an import cycle share one namespace hash, so each hash is mixed with its file's path before
/// they are added up; added or XORed bare, equal hashes would cancel out.
fn component_sig_hash<'a>(files: impl IntoIterator<Item = (&'a FileKey, u64)>) -> u64 {
    files.into_iter().fold(0, |hash, (file_key, ns_hash)| {
        hash.wrapping_add(xx::hash(file_key.as_str().as_bytes(), ns_hash))
    })
}

fn merge_component(
    transaction: &Transaction,
    options: &Options,
    for_find_all_refs: bool,
    component: Vec1<FileKey>,
) -> Result<(bool, Option<(ErrorSuppressions, f64)>), worker_cancel::WorkerCanceled> {
    let start_time = Instant::now();

    let typed_component: Vec<_> = component
        .iter()
        .filter_map(|f| transaction.get_typed_parse(f).map(|p| (f.dupe(), p)))
        .collect();

    if typed_component.is_empty() {
        return Ok((false, None));
    }

    worker_cancel::check_should_cancel()?;
    let (hash, merge_hashes) = sig_hash(
        for_find_all_refs,
        &options.root,
        transaction,
        &typed_component,
    );
    worker_cancel::check_should_cancel()?;

    let mut suppressions = ErrorSuppressions::empty();
    for (file, typed_parse) in &typed_component {
        worker_cancel::check_should_cancel()?;
        let docblock = typed_parse.docblock_unsafe(file);
        let metadata = Metadata {
            overridable: OverridableMetadata {
                strict: docblock.flow == Some(FlowMode::OptInStrict),
                strict_local: docblock.flow == Some(FlowMode::OptInStrictLocal),
                ..Default::default()
            },
            ..Default::default()
        };
        let lint_severities = get_lint_severities(
            &metadata,
            &options.strict_mode,
            options.lint_severities.clone(), // clone needed: LintSettings doesn't implement Dupe
        );
        let ast = typed_parse.ast_unsafe(file);
        let (_, new_suppressions, _) = scan_for_suppressions(
            false,
            &lint_severities,
            vec![(file.dupe(), &ast.all_comments)],
        );
        suppressions.union(new_suppressions);
    }

    worker_cancel::check_should_cancel()?;
    let diff = merge_context_mutator::add_merge_on_diff(
        transaction,
        for_find_all_refs,
        &typed_component,
        hash,
        merge_hashes,
    );
    let duration = start_time.elapsed().as_secs_f64();

    Ok((diff, Some((suppressions, duration))))
}

pub type CheckFileArtifacts<'cx> = (
    Context<'cx>,
    Arc<flow_type_sig::packed_type_sig::Module<Loc>>,
    Arc<FileSig>,
    ast::Program<ALoc, (ALoc, Type)>,
);

pub type CheckFileAccs = (
    ErrorSet,
    ErrorSet,
    ErrorSuppressions,
    FileCoverage,
    Result<Vec<flow_services_references::find_refs_types::SingleRef>, String>,
    f64,
);

/// Checks files one after the other, keeping the contexts of their dependencies in its cache for
/// the files checked after them.
pub struct Checker {
    transaction: Arc<Transaction>,
    find_ref_request: flow_services_references::find_refs_types::Request,
    check_file_and_comp_env: check_service::CheckFileAndCompEnv,
    cache: CheckCache,
}

impl Checker {
    /// Checks `file`, and hands its artifacts to `finish`, which has to be done with its context
    /// by the time it returns.
    pub fn check<R>(
        &mut self,
        file: FileKey,
        finish: impl FnOnce(CheckFileArtifacts<'_>, CheckFileAccs) -> R,
    ) -> CheckJobOutcome<R> {
        match self.check_file(file.dupe(), finish) {
            Ok(opt) => CheckJobOutcome::Ok(opt),
            Err(flow_utils_concurrency::job_error::JobError::Canceled(c)) => {
                CheckJobOutcome::Canceled(c)
            }
            Err(flow_utils_concurrency::job_error::JobError::TimedOut(t)) => {
                let file_loc = ALoc::of_loc(Loc {
                    source: Some(file),
                    ..LOC_NONE
                });
                CheckJobOutcome::PerFileError((file_loc, InternalError::check_timeout(t.elapsed)))
            }
            Err(flow_utils_concurrency::job_error::JobError::DebugThrow { loc }) => {
                CheckJobOutcome::PerFileError((loc, InternalError::DebugThrow))
            }
        }
    }

    /// Drops the contexts of the dependencies of the files checked so far.
    pub fn clear_cache(&mut self) {
        self.cache.clear_dependencies();
    }

    fn check_file<R>(
        &mut self,
        file: FileKey,
        finish: impl FnOnce(CheckFileArtifacts<'_>, CheckFileAccs) -> R,
    ) -> Result<Option<R>, flow_utils_concurrency::job_error::JobError> {
        let Checker {
            transaction,
            find_ref_request,
            check_file_and_comp_env:
                check_service::CheckFileAndCompEnv {
                    make_cx,
                    check_file,
                    compute_env: _,
                },
            cache,
        } = self;
        let start_time = Instant::now();
        let parse = match transaction.get_typed_parse(&file) {
            Some(p) => p,
            None => return Ok(None),
        };
        let ast = parse.ast_unsafe(&file);
        let type_sig = parse.type_sig_unsafe(&file);
        let type_sig_options = parse.type_sig_options_unsafe(&file);
        let (file_sig, tolerable_errors) = parse.tolerable_file_sig_unsafe(&file);
        let docblock = parse.docblock_unsafe(&file);
        let aloc_table: flow_aloc::LazyALocTable = {
            let file_for_aloc = file.dupe();
            let parse_for_aloc = parse.dupe();
            Rc::new(LazyCell::new(Box::new(move || {
                Rc::new(ALocTable::unpack(
                    file_for_aloc.dupe(),
                    &parse_for_aloc.aloc_table_unsafe(&file_for_aloc),
                ))
            })
                as Box<dyn FnOnce() -> Rc<ALocTable>>))
        };
        let resolved_modules: Vec<(FlowImportSpecifier, ResolvedModule)> = {
            let requires = parse.requires();
            let resolved_requires = parse.resolved_requires_unsafe();
            let resolved = resolved_requires.get_resolved_modules();
            requires
                .iter()
                .zip(resolved.iter())
                .map(|(specifier, module)| (specifier.dupe(), module.clone()))
                .collect()
        };
        let docblock_for_find_refs = docblock.dupe();
        let dependencies = cache.dependencies().id();
        let mut arena = ContextArena::new();
        let own = arena.id();
        {
            let mut cx = make_cx(
                cache.store(&mut arena),
                own,
                dependencies,
                file.dupe(),
                resolved_modules,
                ast.dupe(),
                docblock,
                aloc_table,
            );
            let ast_ref = ast;
            let ast::Program {
                all_comments: comments,
                ..
            } = ast_ref.as_ref();
            let aloc_ast = flow_aloc::loc_to_aloc_ast(ast_ref.as_ref());
            let metadata = cx.metadata().clone();
            let enabled = matches!(
                &find_ref_request.def_info,
                flow_services_get_def::get_def_types::DefInfo::PropertyDefinition(_)
            );
            let loc_of_aloc = |aloc: &ALoc| transaction.loc_of_aloc(aloc);
            let (typed_ast_result, obj_to_obj_map) =
                obj_to_obj_hook::with_obj_to_obj_hook(enabled, &loc_of_aloc, || {
                    check_file(
                        &mut cx,
                        &file,
                        &type_sig_options,
                        file_sig.dupe(),
                        &metadata,
                        comments,
                        ast_ref.as_ref(),
                        aloc_ast,
                        Some(type_sig.dupe()),
                    )
                });
            let typed_ast = typed_ast_result?;
            let find_refs_result = match &find_ref_request.def_info {
                flow_services_get_def::get_def_types::DefInfo::NoDefinition(_) => Ok(Vec::new()),
                _ => {
                    let ast_info: flow_services_get_def::find_refs_utils::AstInfo = (
                        ast_ref.dupe(),
                        file_sig.dupe(),
                        docblock_for_find_refs.dupe(),
                    );
                    let result =
                        flow_services_references::find_refs_js::local_refs_of_find_ref_request(
                            &loc_of_aloc,
                            &ast_info,
                            &mut cx,
                            &typed_ast,
                            &obj_to_obj_map,
                            &file,
                            find_ref_request,
                        )?;
                    match result {
                        Ok(
                            flow_services_references::find_refs_types::FindRefsOk::FoundReferences(
                                refs,
                            ),
                        ) => Ok(refs),
                        Ok(
                            flow_services_references::find_refs_types::FindRefsOk::NoDefinition(_),
                        ) => Ok(Vec::new()),
                        Err(e) => Err(e),
                    }
                }
            };
            drop(ast_ref);
            let coverage = file_coverage(&mut cx, &typed_ast);
            let errors = cx.errors();
            let errors = if tolerable_errors.is_empty() {
                errors
            } else {
                let tolerable_error_set = inference_utils::set_of_file_sig_tolerable_errors(
                    file.dupe(),
                    &tolerable_errors,
                );
                errors.union(&tolerable_error_set)
            };
            let mut suppressions = cx.take_error_suppressions();
            let severity_cover = cx.severity_cover().dupe();
            let include_suppressions = cx.include_suppressions();
            let (errors, warnings) = suppressions.filter_lints(
                errors,
                cx.aloc_tables(),
                include_suppressions,
                &severity_cover,
            );
            let duration = start_time.elapsed().as_secs_f64();
            if duration > 5.0 {
                eprintln!("[SLOW-CHECK] {:>8.3}s  {}", duration, file.as_str());
            }
            Ok(Some(finish(
                (cx, type_sig, file_sig, typed_ast),
                (
                    errors,
                    warnings,
                    suppressions,
                    coverage,
                    find_refs_result,
                    duration,
                ),
            )))
        }
    }
}

// When we check the contents of a file, we create types from the signatures of its dependencies.
// `cache` holds those, so the next request for a file with the same dependencies does not rebuild
// them. It is owned by the workload runner and lent to each workload; see `CheckContentsCache`.
pub fn check_contents_context(
    cache: &mut CheckContentsCache,
    transaction: Arc<Transaction>,
    options: Arc<Options>,
    all_unordered_libs: Arc<BTreeSet<FlowSmolStr>>,
    master_cx: Arc<MasterContext>,
    file: FileKey,
    ast: Arc<ast::Program<Loc, Loc>>,
    docblock: Arc<Docblock>,
    file_sig: Arc<FileSig>,
    type_sig_options: Arc<TypeSigOptions>,
) -> Result<
    (CheckedContext, ast::Program<ALoc, (ALoc, Type)>),
    flow_utils_concurrency::job_error::JobError,
> {
    let aloc_table: flow_aloc::LazyALocTable = {
        let file_for_aloc = file.dupe();
        let transaction_for_aloc = transaction.dupe();
        Rc::new(LazyCell::new(Box::new(move || {
            match transaction_for_aloc.get_aloc_table(&file_for_aloc) {
                Some(packed) => Rc::new(ALocTable::unpack(file_for_aloc.dupe(), &packed)),
                None => Rc::new(ALocTable::empty(file_for_aloc.dupe())),
            }
        })
            as Box<dyn FnOnce() -> Rc<ALocTable>>))
    };
    let resolved_modules: Vec<(FlowImportSpecifier, ResolvedModule)> = {
        let node_modules_containers = files::node_modules_containers.read().unwrap();
        let f = |mref: &FlowImportSpecifier| {
            flow_services_module::imported_module(
                &options,
                &transaction,
                &node_modules_containers,
                &file,
                None,
                mref,
            )
        };
        file_sig
            .require_loc_map()
            .into_keys()
            .map(|mref| {
                let result = f(&mref);
                (mref, ResolvedModule::from_result(result))
            })
            .collect()
    };
    let cache = cache.check_cache();
    let check_service::CheckFileAndCompEnv {
        mut make_cx,
        mut check_file,
        compute_env: _,
    } = check_service::mk_check_file(
        transaction.dupe(),
        options.dupe(),
        all_unordered_libs,
        master_cx.as_ref(),
        cache,
    );
    let mut arena = ContextArena::new();
    let own = arena.id();
    let dependencies = cache.dependencies().id();
    let (cx, typed_ast) = {
        let mut cx = make_cx(
            cache.store(&mut arena),
            own,
            dependencies,
            file.dupe(),
            resolved_modules,
            ast.dupe(),
            docblock,
            aloc_table,
        );
        let cx_id = cx.id();
        let ast_ref = ast;
        let ast::Program {
            all_comments: comments,
            ..
        } = ast_ref.as_ref();
        let aloc_ast = flow_aloc::loc_to_aloc_ast(ast_ref.as_ref());
        let metadata = cx.metadata().clone();
        let typed_ast = check_file(
            &mut cx,
            &file,
            &type_sig_options,
            file_sig,
            &metadata,
            comments,
            ast_ref.as_ref(),
            aloc_ast,
            // Fresh contents may not match the transaction's ALoc table. Repack
            // the small set of roots needed by declared-types analysis from the
            // fresh AST instead of reusing its keyed signature.
            None,
        )?;
        (cx_id, typed_ast)
    };
    Ok((CheckedContext::new(arena, cx, cache), typed_ast))
}

pub fn compute_env_of_contents(
    cache: &mut CheckContentsCache,
    transaction: Arc<Transaction>,
    options: Arc<Options>,
    all_unordered_libs: Arc<BTreeSet<FlowSmolStr>>,
    master_cx: Arc<MasterContext>,
    file: FileKey,
    ast: Arc<ast::Program<Loc, Loc>>,
    docblock: Arc<Docblock>,
    file_sig: Arc<FileSig>,
    type_sig_options: Arc<TypeSigOptions>,
) -> Result<CheckedContext, flow_utils_concurrency::job_error::JobError> {
    let aloc_table: flow_aloc::LazyALocTable = {
        let file_for_aloc = file.dupe();
        let transaction_for_aloc = transaction.dupe();
        Rc::new(LazyCell::new(Box::new(move || {
            match transaction_for_aloc.get_aloc_table(&file_for_aloc) {
                Some(packed) => Rc::new(ALocTable::unpack(file_for_aloc.dupe(), &packed)),
                None => Rc::new(ALocTable::empty(file_for_aloc.dupe())),
            }
        })
            as Box<dyn FnOnce() -> Rc<ALocTable>>))
    };
    let resolved_modules: Vec<(FlowImportSpecifier, ResolvedModule)> = {
        let node_modules_containers = files::node_modules_containers.read().unwrap();
        let f = |mref: &FlowImportSpecifier| {
            flow_services_module::imported_module(
                &options,
                &transaction,
                &node_modules_containers,
                &file,
                None,
                mref,
            )
        };
        file_sig
            .require_loc_map()
            .into_keys()
            .map(|mref| {
                let result = f(&mref);
                (mref, ResolvedModule::from_result(result))
            })
            .collect()
    };
    let cache = cache.check_cache();
    let check_service::CheckFileAndCompEnv {
        mut make_cx,
        check_file: _,
        mut compute_env,
    } = check_service::mk_check_file(
        transaction.dupe(),
        options.dupe(),
        all_unordered_libs,
        master_cx.as_ref(),
        cache,
    );
    let mut arena = ContextArena::new();
    let own = arena.id();
    let dependencies = cache.dependencies().id();
    let cx = {
        let mut cx = make_cx(
            cache.store(&mut arena),
            own,
            dependencies,
            file.dupe(),
            resolved_modules,
            ast.dupe(),
            docblock,
            aloc_table,
        );
        let aloc_ast = flow_aloc::loc_to_aloc_ast(ast.as_ref());
        compute_env(
            &mut cx,
            &type_sig_options,
            ast.as_ref(),
            aloc_ast,
            &file_sig,
        )?;
        cx.id()
    };
    Ok(CheckedContext::new(arena, cx, cache))
}

fn merge_job<A, F>(
    transaction: &Transaction,
    options: &Options,
    for_find_all_refs: bool,
    job: &F,
    batch: Vec<Component>,
) -> Result<MergeResult<A>, worker_cancel::WorkerCanceled>
where
    F: Fn(
        &Transaction,
        &Options,
        bool,
        Vec1<FileKey>,
    ) -> Result<(bool, A), worker_cancel::WorkerCanceled>,
{
    let results = batch
        .into_iter()
        .map(|Component(component)| {
            worker_cancel::check_should_cancel()?;
            let leader = component.first().dupe();
            let component_cloned = (*component).clone();
            let (diff, result) = job(transaction, options, for_find_all_refs, component_cloned)?;
            Ok((leader, diff, result))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(MergeResult(results))
}

pub fn merge_runner<A, F>(
    pool: &ThreadPool,
    transaction: &Transaction,
    options: &Options,
    for_find_all_refs: bool,
    sig_dependency_graph: &(impl GraphLike<FileKey> + ?Sized),
    components: Vec<Vec1<FileKey>>,
    recheck_set: &FlowOrdSet<FileKey>,
    job: F,
) -> Result<MergeResults<A>, worker_cancel::WorkerCanceled>
where
    A: Send + Sync + Default + std::fmt::Debug + 'static,
    F: Fn(
            &Transaction,
            &Options,
            bool,
            Vec1<FileKey>,
        ) -> Result<(bool, A), worker_cancel::WorkerCanceled>
        + Send
        + Sync
        + 'static,
{
    #[derive(Debug)]
    struct MergeAcc<A>(Result<Vec<A>, worker_cancel::WorkerCanceled>);

    impl<A> Default for MergeAcc<A> {
        fn default() -> Self {
            MergeAcc(Ok(Vec::new()))
        }
    }

    let num_workers = pool.num_workers();
    let start_time = Instant::now();
    let stream = Arc::new(MergeStream::<A>::new(
        num_workers,
        sig_dependency_graph,
        components,
        recheck_set,
    ));
    stream.update_server_status();

    let stream_ref = stream.dupe();
    let stream_merge = stream.dupe();

    let transaction_clone = transaction.dupe();

    let results: MergeAcc<A> = flow_utils_concurrency::map_reduce::call(
        pool,
        move || stream_ref.next(),
        move |acc: &mut MergeAcc<A>, batch: Vec<Component>| {
            if acc.0.is_err() {
                return;
            }
            match merge_job(transaction_clone, options, for_find_all_refs, &job, batch) {
                Ok(merged) => {
                    let acc_vec = acc.0.as_mut().unwrap();
                    let new_acc = stream_merge.merge(merged, std::mem::take(acc_vec));
                    *acc_vec = new_acc;
                }
                Err(c) => {
                    acc.0 = Err(c);
                }
            }
        },
        |a, b| match (&mut a.0, b.0) {
            (Err(_), _) => {}
            (a_slot @ Ok(_), Err(c)) => {
                *a_slot = Err(c);
            }
            (Ok(a_vec), Ok(mut b_vec)) => {
                a_vec.append(&mut b_vec);
            }
        },
    );
    let results = results.0?;

    let total_files = stream.total_files();
    let skipped_count = stream.skipped_count();
    let sig_new_or_changed = stream.sig_new_or_changed();
    let message = format!("Merge skipped {} of {} modules", skipped_count, total_files);

    flow_hh_logger::info!("{}", message);
    let elapsed = start_time.elapsed().as_secs_f64();
    if options.profile {
        flow_hh_logger::info!("merged in {}", elapsed);
    }

    Ok((
        results,
        SigOptsData {
            skipped_count,
            sig_new_or_changed,
        },
    ))
}

pub fn merge(
    pool: &ThreadPool,
    transaction: &Transaction,
    options: &Options,
    for_find_all_refs: bool,
    sig_dependency_graph: &(impl GraphLike<FileKey> + ?Sized),
    components: Vec<Vec1<FileKey>>,
    recheck_set: &FlowOrdSet<FileKey>,
) -> Result<MergeResults<Option<(ErrorSuppressions, f64)>>, worker_cancel::WorkerCanceled> {
    merge_runner(
        pool,
        transaction,
        options,
        for_find_all_refs,
        sig_dependency_graph,
        components,
        recheck_set,
        |transaction, options, for_find_all_refs, component| {
            merge_component(transaction, options, for_find_all_refs, component)
        },
    )
}

/// Outcome of a single per-file check job. Replaces the prior
/// `UnitResult<Option<CheckFileResult>>` shape so cancellation can travel as a
/// distinct typed signal rather than a per-file `InternalError`. Per-file
/// failures (timeout, `$Flow$DebugThrow`) ride `PerFileError`; worker-level
/// cancellation rides `Canceled` so the worker-pool driver can stop pulling
/// files and unwind back to the recheck loop.
pub enum CheckJobOutcome<R> {
    Ok(Option<R>),
    PerFileError((ALoc, InternalError)),
    Canceled(flow_utils_concurrency::worker_cancel::WorkerCanceled),
}

pub fn mk_check(
    transaction: Arc<Transaction>,
    options: Arc<Options>,
    all_unordered_libs: Arc<BTreeSet<FlowSmolStr>>,
    master_cx: &MasterContext,
    find_ref_request: flow_services_references::find_refs_types::Request,
) -> Checker {
    let mut cache = CheckCache::create(10_000_000);
    let check_file_and_comp_env = check_service::mk_check_file(
        transaction.dupe(),
        options,
        all_unordered_libs,
        master_cx,
        &mut cache,
    );
    Checker {
        transaction,
        find_ref_request,
        check_file_and_comp_env,
        cache,
    }
}

#[cfg(test)]
mod tests {
    use flow_parser::file_key::FileKey;
    use flow_parser::file_key::FileKeyInner;

    use super::component_sig_hash;

    fn file(name: &str) -> FileKey {
        FileKey::new(FileKeyInner::SourceFile(name.to_owned()))
    }

    #[test]
    fn each_hash_counts_for_its_own_file() {
        let (a, b) = (file("a.js"), file("b.js"));
        let hash = component_sig_hash([(&a, 1), (&b, 2)]);
        assert_ne!(hash, component_sig_hash([(&a, 1), (&b, 3)]));
        assert_ne!(
            hash,
            component_sig_hash([(&a, 2), (&b, 1)]),
            "swapping two files' hashes should change the component's hash"
        );
    }
}
