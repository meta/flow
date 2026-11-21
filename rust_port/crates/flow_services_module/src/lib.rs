/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::sync::LazyLock;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_common::files;
use flow_common::flow_import_specifier::FlowImportSpecifier;
use flow_common::options::ModuleSystem;
use flow_common::options::Options;
use flow_common_modulename::HasteModuleInfo;
use flow_common_modulename::Modulename;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_heap::heap_state::HeapReader;
use flow_heap::parse::Parse;
use flow_heap::parsing_heaps::Transaction;
use flow_heap::resolved_requires::Dependency;
use flow_parser::file_key::FileKey;
use flow_parser_utils::package_json::PackageJson;
use regex::Regex;
use vec1::Vec1;

fn choose_provider_and_warn_about_duplicates(
    options: &Options,
    m: &FlowSmolStr,
    errmap: &mut BTreeMap<FlowSmolStr, (FileKey, Vec1<FileKey>)>,
    providers: Vec<FileKey>,
    fallback: impl FnOnce() -> Option<FileKey>,
) -> Option<FileKey> {
    fn warn_duplicate_providers(
        m: &FlowSmolStr,
        provider: &FileKey,
        duplicates: Vec<FileKey>,
        acc: &mut BTreeMap<FlowSmolStr, (FileKey, Vec1<FileKey>)>,
    ) {
        let Ok(duplicates) = Vec1::try_from_vec(duplicates) else {
            return;
        };
        acc.insert(m.clone(), (provider.dupe(), duplicates));
    }

    fn is_common_code_path(options: &Options, key: &FileKey) -> bool {
        options.projects_options.is_common_code_path(key.as_str())
    }

    fn common_interface_implementation_pair(
        options: &Options,
        defn: &FileKey,
        impl_: &FileKey,
    ) -> bool {
        files::has_declaration_ext(defn)
            && !files::has_declaration_ext(impl_)
            && is_common_code_path(options, defn)
            && !is_common_code_path(options, impl_)
    }

    let (mut definitions, mut implementations): (Vec<_>, Vec<_>) =
        providers.into_iter().partition(files::has_declaration_ext);

    if implementations.is_empty() && definitions.is_empty() {
        fallback()
    } else if definitions.is_empty() {
        let impl_ = implementations.remove(0);
        let duplicates = implementations;
        warn_duplicate_providers(m, &impl_, duplicates, errmap);
        Some(impl_)
    } else if implementations.is_empty() {
        let defn = definitions.remove(0);
        let duplicates = definitions;
        warn_duplicate_providers(m, &defn, duplicates, errmap);
        Some(defn)
    } else {
        let impl_ = implementations.remove(0);
        let dup_impls = implementations;
        let defn = definitions.remove(0);
        let dup_defns = definitions;
        let all_impls: Vec<FileKey> = std::iter::once(impl_.dupe())
            .chain(dup_impls.iter().duped())
            .collect();
        let (mut common_interface_impls, other_impls): (Vec<_>, Vec<_>) = all_impls
            .into_iter()
            .partition(|impl_| common_interface_implementation_pair(options, &defn, impl_));

        let def_with_flow_ext_chopped = files::chop_declaration_ext(&defn);
        let file_options = &options.file_options;
        let impl_with_platform_suffix_chopped =
            files::chop_platform_suffix_for_file(file_options, &impl_);

        if !common_interface_impls.is_empty() {
            let selected_impl = common_interface_impls.remove(0);
            let duplicates = common_interface_impls
                .into_iter()
                .chain(other_impls)
                .collect();
            warn_duplicate_providers(m, &selected_impl, duplicates, errmap);
            warn_duplicate_providers(m, &defn, dup_defns, errmap);
            Some(selected_impl)
        } else if def_with_flow_ext_chopped != impl_with_platform_suffix_chopped {
            if files::chop_platform_suffix_for_file(file_options, &def_with_flow_ext_chopped)
                != impl_with_platform_suffix_chopped
            {
                warn_duplicate_providers(m, &defn, vec![impl_.dupe()], errmap);
            }
            warn_duplicate_providers(m, &impl_, dup_impls, errmap);
            warn_duplicate_providers(m, &defn, dup_defns, errmap);
            Some(defn)
        } else {
            warn_duplicate_providers(m, &impl_, dup_impls, errmap);
            warn_duplicate_providers(m, &defn, dup_defns, errmap);
            Some(defn)
        }
    }
}

fn module_name_candidates(options: &Options, name: &str) -> Vec1<String> {
    fn map_name(
        name: &str,
        expand_project_root_token: impl Fn(&str) -> String,
        mapped_names: &mut Vec<String>,
        (regexp, template): &(Regex, String),
    ) {
        let new_name = {
            let result = regexp.replace_all(name, template.as_str());
            expand_project_root_token(&result)
        };

        if new_name != name {
            mapped_names.push(new_name);
        }
    }

    let mappers = &options.module_name_mappers;
    let root = &options.root;
    let expand_project_root_token = |s: &str| files::expand_project_root_token(root, s);

    let mut mapped_names = Vec::new();
    for mapper in mappers.iter() {
        map_name(name, expand_project_root_token, &mut mapped_names, mapper);
    }

    Vec1::from_vec_push(mapped_names, name.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageIncompatibleReason {
    New,
    BecameInvalid,
    BecameValid,
    NameChanged(Option<FlowSmolStr>, Option<FlowSmolStr>),
    MainChanged(Option<FlowSmolStr>, Option<FlowSmolStr>),
    /// The `types`/`typings` property changed from the former to the latter
    TypesChanged(Option<FlowSmolStr>, Option<FlowSmolStr>),
    /// The `haste_commonjs` property changed to this value
    HasteCommonjsChanged(bool),
    ExportsChanged,
    Unknown,
}

impl std::fmt::Display for PackageIncompatibleReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn string_of_option(opt: &Option<FlowSmolStr>) -> &str {
            match opt {
                None => "<None>",
                Some(x) => x.as_str(),
            }
        }

        match self {
            Self::New => write!(f, "new"),
            Self::BecameInvalid => write!(f, "became invalid"),
            Self::BecameValid => write!(f, "became valid"),
            Self::NameChanged(old, new_) => write!(
                f,
                "name changed from `{}` to `{}`",
                string_of_option(old),
                string_of_option(new_)
            ),
            Self::MainChanged(old, new_) => write!(
                f,
                "main changed from `{}` to `{}`",
                string_of_option(old),
                string_of_option(new_)
            ),
            Self::TypesChanged(old, new_) => write!(
                f,
                "types changed from `{}` to `{}`",
                string_of_option(old),
                string_of_option(new_)
            ),
            Self::HasteCommonjsChanged(new_) => {
                write!(f, "haste_commonjs changed from `{}` to `{}`", !new_, new_)
            }
            Self::ExportsChanged => write!(f, "exports changed"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageIncompatibleReturn {
    Compatible,
    Incompatible(PackageIncompatibleReason),
}

pub fn package_incompatible(
    _filename: &FileKey,
    old_package: Option<Result<PackageJson, ()>>,
    new_package: Result<PackageJson, ()>,
) -> PackageIncompatibleReturn {
    match (old_package, new_package) {
        (None, Ok(_)) => PackageIncompatibleReturn::Incompatible(PackageIncompatibleReason::New),
        (None, Err(_)) => PackageIncompatibleReturn::Compatible,
        (Some(Err(())), Err(_)) => PackageIncompatibleReturn::Compatible,
        (Some(Err(())), Ok(_)) => {
            PackageIncompatibleReturn::Incompatible(PackageIncompatibleReason::BecameValid)
        }
        (Some(Ok(_)), Err(_)) => {
            PackageIncompatibleReturn::Incompatible(PackageIncompatibleReason::BecameInvalid)
        }
        (Some(Ok(old_package)), Ok(new_package)) => {
            let old_main = old_package.main();
            let new_main = new_package.main();
            let old_types = old_package.types();
            let new_types = new_package.types();
            let old_name = old_package.name();
            let new_name = new_package.name();
            let old_haste_commonjs = old_package.haste_commonjs();
            let new_haste_commonjs = new_package.haste_commonjs();
            let old_exports = old_package.exports();
            let new_exports = new_package.exports();

            if old_name == new_name
                && old_main == new_main
                && old_types == new_types
                && old_haste_commonjs == new_haste_commonjs
                && old_exports == new_exports
            {
                return PackageIncompatibleReturn::Compatible;
            }

            if old_name != new_name {
                PackageIncompatibleReturn::Incompatible(PackageIncompatibleReason::NameChanged(
                    old_name, new_name,
                ))
            } else if old_main != new_main {
                PackageIncompatibleReturn::Incompatible(PackageIncompatibleReason::MainChanged(
                    old_main, new_main,
                ))
            } else if old_types != new_types {
                PackageIncompatibleReturn::Incompatible(PackageIncompatibleReason::TypesChanged(
                    old_types, new_types,
                ))
            } else if old_haste_commonjs != new_haste_commonjs {
                PackageIncompatibleReturn::Incompatible(
                    PackageIncompatibleReason::HasteCommonjsChanged(new_haste_commonjs),
                )
            } else if old_exports != new_exports {
                PackageIncompatibleReturn::Incompatible(PackageIncompatibleReason::ExportsChanged)
            } else {
                PackageIncompatibleReturn::Incompatible(PackageIncompatibleReason::Unknown)
            }
        }
    }
}

pub struct PackageInfo(Option<PackageJson>);

impl PackageInfo {
    pub fn new(package_json: Option<PackageJson>) -> Self {
        Self(package_json)
    }

    pub fn none() -> Self {
        Self(None)
    }
}

#[derive(Default)]
pub struct PhantomAcc(BTreeMap<Modulename, Option<Dependency>>);

impl PhantomAcc {
    pub fn keys(&self) -> impl Iterator<Item = &Modulename> {
        self.0.keys()
    }
}

struct ModuleResolver<'a> {
    options: &'a Options,
    transaction: &'a Transaction,
    access: flow_heap::transaction::HeapAccess<'a>,
    symlink_paths: Option<&'a files::SymlinkMap>,
}

impl<'a> ModuleResolver<'a> {
    fn new(options: &'a Options, transaction: &'a Transaction) -> Self {
        let symlink_paths = transaction.symlink_paths();
        Self {
            options,
            transaction,
            access: transaction.latest_heap_reader(),
            symlink_paths: symlink_paths.is_complete().then_some(symlink_paths),
        }
    }

    fn resolve_symlinks<'b>(&self, path: &'b str) -> Cow<'b, str> {
        let normalized_path = files::lexically_normalize_path(path);
        let resolved_path = match self.symlink_paths {
            Some(symlink_paths) => symlink_paths
                .try_resolve(&normalized_path)
                .unwrap_or_else(|| Cow::Borrowed(normalized_path.as_ref())),
            None => {
                let crawl_covers_path = files::initial_crawl_covers_path(
                    &self.options.file_options,
                    &self.options.root,
                    Path::new(normalized_path.as_ref()),
                );
                if crawl_covers_path {
                    files::canonicalize_path(&normalized_path)
                } else {
                    Cow::Borrowed(normalized_path.as_ref())
                }
            }
        };
        if resolved_path == path {
            Cow::Borrowed(path)
        } else {
            Cow::Owned(resolved_path.into_owned())
        }
    }

    fn get_dependency(&self, modulename: &Modulename) -> Option<Dependency> {
        self.access.reader().get_dependency(modulename)
    }

    fn get_provider(&self, dependency: &Dependency) -> Option<FileKey> {
        self.access.reader().get_provider(dependency)
    }

    fn get_package_info(&self, file: &FileKey) -> Option<Arc<PackageJson>> {
        self.access.reader().get_package_info(file)
    }

    fn is_typed_file(&self, file: &FileKey) -> bool {
        self.access.reader().is_typed_file(file)
    }

    fn is_package_file(&self, file: &FileKey) -> bool {
        self.access.reader().is_package_file(file)
    }

    fn get_requires_unsafe(&self, file: &FileKey) -> Arc<[FlowImportSpecifier]> {
        self.access.reader().get_requires_unsafe(file)
    }

    fn intern_dependency_from_modulename(&self, mname: Modulename) -> Dependency {
        self.transaction.intern_dependency_from_modulename(mname)
    }
}

static CURRENT_DIR_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\.").unwrap());
static PARENT_DIR_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\.\.").unwrap());
static ABSOLUTE_PATH_REGEXP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(/|[A-Za-z]:[/\\])").unwrap());

fn is_relative_or_absolute(r: &str) -> bool {
    CURRENT_DIR_NAME.is_match(r) || PARENT_DIR_NAME.is_match(r) || ABSOLUTE_PATH_REGEXP.is_match(r)
}

fn record_phantom_dependency(
    mname: Modulename,
    dependency: Option<Dependency>,
    phantom_acc: Option<&mut PhantomAcc>,
) {
    match phantom_acc {
        None => (),
        Some(phantom_acc) => {
            phantom_acc.0.insert(mname, dependency);
        }
    }
}

trait ModuleSystemSig {
    fn exported_module(
        options: &Options,
        file: &FileKey,
        package_info: &PackageInfo,
    ) -> Option<HasteModuleInfo>;

    fn imported_module(
        options: &Options,
        transaction: &ModuleResolver<'_>,
        node_modules_containers: &BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
        importing_file: &FileKey,
        phantom_acc: Option<&mut PhantomAcc>,
        import_specifier: &FlowImportSpecifier,
    ) -> Result<Dependency, Option<FlowImportSpecifier>>;

    fn choose_provider(
        options: &Options,
        m: &str,
        errmap: &mut BTreeMap<FlowSmolStr, (FileKey, Vec1<FileKey>)>,
        files: Vec<FileKey>,
        fallback: impl FnOnce() -> Option<FileKey>,
    ) -> Option<FileKey>;
}

mod node {
    use std::collections::BTreeSet;
    use std::path::Path;
    use std::sync::Arc;

    use flow_common::files;
    use flow_common::files::FileOptions;
    use flow_common::flow_import_specifier::FlowImportSpecifier;
    use flow_common::flow_projects::ProjectsOptions;
    use flow_common::options::Options;
    use flow_common_modulename::Modulename;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_parser::file_key::FileKey;
    use flow_parser_utils::package_json::PackageJson;
    use vec1::Vec1;

    use super::*;

    pub struct Node;

    impl super::ModuleSystemSig for Node {
        fn exported_module(
            _options: &Options,
            _file: &FileKey,
            _package_info: &PackageInfo,
        ) -> Option<HasteModuleInfo> {
            None
        }

        fn imported_module(
            options: &Options,
            transaction: &ModuleResolver<'_>,
            node_modules_containers: &BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
            importing_file: &FileKey,
            phantom_acc: Option<&mut PhantomAcc>,
            import_specifier: &FlowImportSpecifier,
        ) -> Result<Dependency, Option<FlowImportSpecifier>> {
            imported_module_impl(
                options,
                transaction,
                node_modules_containers,
                importing_file,
                phantom_acc,
                import_specifier,
            )
        }

        fn choose_provider(
            options: &Options,
            m: &str,
            errmap: &mut BTreeMap<FlowSmolStr, (FileKey, Vec1<FileKey>)>,
            files: Vec<FileKey>,
            _fallback: impl FnOnce() -> Option<FileKey>,
        ) -> Option<FileKey> {
            choose_provider_impl(options, m, files, errmap)
        }
    }

    fn path_if_exists(
        transaction: &ModuleResolver<'_>,
        file_options: &FileOptions,
        phantom_acc: Option<&mut PhantomAcc>,
        path: &str,
    ) -> Option<Dependency> {
        let path = transaction.resolve_symlinks(path);
        let file_key = files::filename_from_string(file_options, &path);
        let mname = Modulename::eponymous_module(file_key);

        let dependency = transaction.get_dependency(&mname);
        match &dependency {
            Some(dep) if transaction.get_provider(dep).is_some() => dependency,
            _ => {
                super::record_phantom_dependency(mname, dependency, phantom_acc);
                None
            }
        }
    }

    fn try_dts_counterpart(
        transaction: &ModuleResolver<'_>,
        file_options: &FileOptions,
        phantom_acc: Option<&mut PhantomAcc>,
        ts_lib_support: bool,
        path: &str,
    ) -> Option<Dependency> {
        if ts_lib_support {
            match files::js_to_dts(path) {
                Some(dts_path) => path_if_exists(transaction, file_options, phantom_acc, &dts_path),
                None => None,
            }
        } else {
            None
        }
    }

    fn path_if_exists_with_file_exts(
        transaction: &ModuleResolver<'_>,
        file_options: &FileOptions,
        phantom_acc: Option<&mut PhantomAcc>,
        path: &str,
        file_exts: &[FlowSmolStr],
    ) -> Option<Dependency> {
        let mut phantom_acc = phantom_acc;
        for ext in file_exts {
            let full_path = format!("{}{}", path, ext);
            if let Some(result) = path_if_exists(
                transaction,
                file_options,
                phantom_acc.as_deref_mut(),
                &full_path,
            ) {
                return Some(result);
            }
        }
        None
    }

    fn parse_package(
        transaction: &ModuleResolver<'_>,
        phantom_acc: Option<&mut PhantomAcc>,
        package_filename: &str,
    ) -> Arc<PackageJson> {
        let package_filename = transaction.resolve_symlinks(package_filename);
        let file_key = FileKey::json_file_of_absolute(&package_filename);

        if transaction.options.incremental_package_json {
            let mname = Modulename::eponymous_module(file_key.dupe());
            let dependency = transaction.get_dependency(&mname);
            super::record_phantom_dependency(mname, dependency, phantom_acc);
        }

        transaction
            .get_package_info(&file_key)
            .unwrap_or_else(|| Arc::new(PackageJson::empty()))
    }

    fn parse_exports(
        transaction: &ModuleResolver<'_>,
        options: &Options,
        mut phantom_acc: Option<&mut PhantomAcc>,
        package_dir: &str,
        subpath: Option<&str>,
        file_exts: &[FlowSmolStr],
    ) -> Option<Dependency> {
        let subpath = subpath.unwrap_or(".");
        let file_options = &options.file_options;
        let export_conditions: Vec<FlowSmolStr> = options
            .node_package_export_conditions
            .iter()
            .map(FlowSmolStr::new)
            .collect();
        let package_json_path = Path::new(package_dir).join("package.json");
        let package = parse_package(
            transaction,
            phantom_acc.as_deref_mut(),
            package_json_path.to_str().unwrap(),
        );

        let source_path = package
            .exports()
            .and_then(|exports| exports.resolve_package(subpath, &export_conditions));

        source_path.and_then(|file| {
            let path = files::normalize_path(package_dir, file.as_str());
            let ts_lib_support = options.typescript_library_definition_support;
            try_dts_counterpart(
                transaction,
                file_options,
                phantom_acc.as_deref_mut(),
                ts_lib_support,
                &path,
            )
            .or_else(|| {
                path_if_exists(transaction, file_options, phantom_acc.as_deref_mut(), &path)
            })
            .or_else(|| {
                path_if_exists_with_file_exts(
                    transaction,
                    file_options,
                    phantom_acc,
                    &path,
                    file_exts,
                )
            })
        })
    }

    fn parse_main(
        transaction: &ModuleResolver<'_>,
        file_options: &FileOptions,
        ts_lib_support: bool,
        mut phantom_acc: Option<&mut PhantomAcc>,
        package_filename: &str,
        file_exts: &[FlowSmolStr],
    ) -> Option<Dependency> {
        let package = parse_package(transaction, phantom_acc.as_deref_mut(), package_filename);
        package.main().and_then(|main| {
            let dir = Path::new(package_filename).parent()?.to_str()?;
            let path = files::normalize_path(dir, main.as_str());
            let path_w_index = Path::new(&path).join("index");
            let path_w_index_str = path_w_index.to_str()?;

            try_dts_counterpart(
                transaction,
                file_options,
                phantom_acc.as_deref_mut(),
                ts_lib_support,
                &path,
            )
            .or_else(|| {
                path_if_exists(transaction, file_options, phantom_acc.as_deref_mut(), &path)
            })
            .or_else(|| {
                path_if_exists_with_file_exts(
                    transaction,
                    file_options,
                    phantom_acc.as_deref_mut(),
                    &path,
                    file_exts,
                )
            })
            .or_else(|| {
                path_if_exists_with_file_exts(
                    transaction,
                    file_options,
                    phantom_acc,
                    path_w_index_str,
                    file_exts,
                )
            })
        })
    }

    fn resolve_types_field(
        transaction: &ModuleResolver<'_>,
        file_options: &FileOptions,
        mut phantom_acc: Option<&mut PhantomAcc>,
        package_dir: &str,
        package: &PackageJson,
        file_exts: &[FlowSmolStr],
    ) -> Option<Dependency> {
        package.types().and_then(|file| {
            let path = files::normalize_path(package_dir, file.as_str());
            let path_w_index = Path::new(&path).join("index");
            let path_w_index_str = path_w_index.to_str()?;

            path_if_exists(transaction, file_options, phantom_acc.as_deref_mut(), &path)
                .or_else(|| {
                    path_if_exists_with_file_exts(
                        transaction,
                        file_options,
                        phantom_acc.as_deref_mut(),
                        &path,
                        file_exts,
                    )
                })
                .or_else(|| {
                    path_if_exists_with_file_exts(
                        transaction,
                        file_options,
                        phantom_acc,
                        path_w_index_str,
                        file_exts,
                    )
                })
        })
    }

    pub(super) fn resolve_package(
        options: &Options,
        transaction: &ModuleResolver<'_>,
        mut phantom_acc: Option<&mut PhantomAcc>,
        subpath: Option<&str>,
        package_dir: &str,
    ) -> Option<Dependency> {
        let file_options = &options.file_options;
        let file_exts = &file_options.module_file_exts;
        let full_package_path = subpath
            .map(|s| files::normalize_path(package_dir, s))
            .unwrap_or_else(|| package_dir.to_string());

        let ts_lib_support = options.typescript_library_definition_support;
        let package_json_path = Path::new(package_dir).join("package.json");
        let package = std::cell::OnceCell::new();
        // `parse_exports` below reads the same package.json first and records it as a dependency.
        let get_package = || -> Arc<PackageJson> {
            package
                .get_or_init(|| {
                    parse_package(transaction, None, package_json_path.to_str().unwrap())
                })
                .clone()
        };

        parse_exports(
            transaction,
            options,
            phantom_acc.as_deref_mut(),
            package_dir,
            subpath,
            file_exts,
        )
        .or_else(|| {
            if ts_lib_support
                && get_package().exports().is_none()
                && (subpath.is_none() || subpath == Some("."))
            {
                resolve_types_field(
                    transaction,
                    file_options,
                    phantom_acc.as_deref_mut(),
                    package_dir,
                    &get_package(),
                    file_exts,
                )
            } else {
                None
            }
        })
        .or_else(|| {
            let package_json = Path::new(&full_package_path).join("package.json");
            parse_main(
                transaction,
                file_options,
                ts_lib_support,
                phantom_acc.as_deref_mut(),
                package_json.to_str().unwrap(),
                file_exts,
            )
        })
        .or_else(|| {
            let index_path = Path::new(&full_package_path).join("index");
            path_if_exists_with_file_exts(
                transaction,
                file_options,
                phantom_acc,
                index_path.to_str().unwrap(),
                file_exts,
            )
        })
    }

    pub(super) fn ordered_allowed_implicit_platform_specific_import(
        file_options: &FileOptions,
        projects_options: &ProjectsOptions,
        importing_file: &str,
        explicit_available_platforms: Option<Vec<FlowSmolStr>>,
    ) -> Option<Vec<FlowSmolStr>> {
        use std::collections::BTreeSet;

        use flow_common::platform_set;

        let available_platform_set = platform_set::available_platforms(
            file_options,
            projects_options,
            importing_file,
            explicit_available_platforms.as_deref(),
        )
        .map(|ps| ps.to_platform_string_set(file_options))?;

        let single_platform = if available_platform_set.len() == 1 {
            vec![available_platform_set.first().unwrap().clone()]
        } else {
            Vec::new()
        };

        let grouped_platform: Vec<FlowSmolStr> = file_options
            .multi_platform_extension_group_mapping
            .iter()
            .find_map(|(group_ext, platforms)| {
                let platforms_set: BTreeSet<FlowSmolStr> = platforms.iter().cloned().collect();
                if available_platform_set.is_subset(&platforms_set) {
                    group_ext.strip_prefix('.').map(FlowSmolStr::new)
                } else {
                    None
                }
            })
            .into_iter()
            .collect();

        let result: Vec<_> = single_platform
            .into_iter()
            .chain(grouped_platform)
            .collect();
        Some(result)
    }

    pub(super) fn resolve_relative(
        options: &Options,
        transaction: &ModuleResolver<'_>,
        mut phantom_acc: Option<&mut PhantomAcc>,
        importing_file: &FileKey,
        relative_to_directory: &str,
        subpath: Option<&str>,
        rel_path: &str,
    ) -> Option<Dependency> {
        let file_options = &options.file_options;
        let projects_options = &options.projects_options;
        let package_path = files::normalize_path(relative_to_directory, rel_path);
        let full_path = subpath
            .map(|s| files::normalize_path(&package_path, s))
            .unwrap_or_else(|| package_path.clone());

        let file_exts = &file_options.module_file_exts;
        let ts_lib_support = options.typescript_library_definition_support;

        match ordered_allowed_implicit_platform_specific_import(
            file_options,
            projects_options,
            importing_file.as_str(),
            None,
        ) {
            None => try_dts_counterpart(
                transaction,
                file_options,
                phantom_acc.as_deref_mut(),
                ts_lib_support,
                &full_path,
            )
            .or_else(|| {
                path_if_exists(
                    transaction,
                    file_options,
                    phantom_acc.as_deref_mut(),
                    &full_path,
                )
            })
            .or_else(|| {
                path_if_exists_with_file_exts(
                    transaction,
                    file_options,
                    phantom_acc.as_deref_mut(),
                    &full_path,
                    file_exts,
                )
            })
            .or_else(|| {
                resolve_package(
                    options,
                    transaction,
                    phantom_acc.as_deref_mut(),
                    subpath,
                    &package_path,
                )
            }),
            Some(ordered_platforms) => try_dts_counterpart(
                transaction,
                file_options,
                phantom_acc.as_deref_mut(),
                ts_lib_support,
                &full_path,
            )
            .or_else(|| {
                path_if_exists(
                    transaction,
                    file_options,
                    phantom_acc.as_deref_mut(),
                    &full_path,
                )
            })
            .or_else(|| {
                for platform in &ordered_platforms {
                    let platform_path = format!("{}.{}", full_path, platform);
                    if let Some(result) = path_if_exists_with_file_exts(
                        transaction,
                        file_options,
                        phantom_acc.as_deref_mut(),
                        &platform_path,
                        file_exts,
                    ) {
                        return Some(result);
                    }
                }
                None
            })
            .or_else(|| {
                path_if_exists_with_file_exts(
                    transaction,
                    file_options,
                    phantom_acc.as_deref_mut(),
                    &full_path,
                    file_exts,
                )
            })
            .or_else(|| resolve_package(options, transaction, phantom_acc, subpath, &package_path)),
        }
    }

    fn parse_package_name(specifier: &str) -> (String, String) {
        if specifier.is_empty() {
            return (String::new(), ".".to_string());
        }

        let initial_char = specifier.chars().next().unwrap();
        let first_separator_index = specifier.find('/');

        let separator_index = if initial_char == '@'
            && let Some(first_separator_index) = first_separator_index
        {
            specifier[first_separator_index + 1..]
                .find('/')
                .map(|i| first_separator_index + 1 + i)
        } else {
            first_separator_index
        };

        let package_name = if let Some(separator_index) = separator_index {
            specifier[..separator_index].to_string()
        } else {
            specifier.to_string()
        };

        let package_subpath = if let Some(separator_index) = separator_index {
            format!(".{}", &specifier[separator_index..])
        } else {
            ".".to_string()
        };

        (package_name, package_subpath)
    }

    pub(super) fn node_module(
        options: &Options,
        transaction: &ModuleResolver<'_>,
        node_modules_containers: &BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
        mut phantom_acc: Option<&mut PhantomAcc>,
        importing_file: &FileKey,
        possible_node_module_container_dir: &str,
        import_specifier: &str,
    ) -> Option<Dependency> {
        let file_options = &options.file_options;
        let (package_name, package_subpath) = parse_package_name(import_specifier);

        if let Some(existing_node_modules_dirs) =
            node_modules_containers.get(possible_node_module_container_dir)
        {
            for dirname in &file_options.node_resolver_dirnames {
                if existing_node_modules_dirs.contains(&FlowSmolStr::new(dirname)) {
                    if let Some(result) = resolve_relative(
                        options,
                        transaction,
                        phantom_acc.as_deref_mut(),
                        importing_file,
                        possible_node_module_container_dir,
                        Some(&package_subpath),
                        &format!("{}{}{}", dirname, std::path::MAIN_SEPARATOR, package_name),
                    ) {
                        return Some(result);
                    }
                }
            }
        }

        let parent_dir = Path::new(possible_node_module_container_dir).parent()?;
        let parent_str = parent_dir.to_str()?;
        if parent_str != possible_node_module_container_dir {
            node_module(
                options,
                transaction,
                node_modules_containers,
                phantom_acc,
                importing_file,
                parent_str,
                import_specifier,
            )
        } else {
            None
        }
    }

    pub(super) fn flow_typed_module(
        options: &Options,
        transaction: &ModuleResolver<'_>,
        mut phantom_acc: Option<&mut PhantomAcc>,
        importing_file: &FileKey,
        import_specifier: &str,
    ) -> Option<Dependency> {
        for relative_to_directory in &options.file_options.module_declaration_dirnames {
            if let Some(result) = resolve_relative(
                options,
                transaction,
                phantom_acc.as_deref_mut(),
                importing_file,
                relative_to_directory,
                Some("."),
                import_specifier,
            ) {
                return Some(result);
            }
        }
        None
    }

    // Maps a package name to its @types equivalent name (no subpath):
    //    "foo" -> "@types/foo"
    //    "@scope/foo" -> "@types/scope__foo"
    fn at_types_package_name(package_name: &str) -> String {
        let parts: Vec<&str> = package_name.split('/').collect();
        match parts.as_slice() {
            [scope, name] if scope.len() > 1 && scope.starts_with('@') => {
                format!("@types/{}__{}", &scope[1..], name)
            }
            _ => format!("@types/{}", package_name),
        }
    }

    // Packages whose types must come from Flow's own lib definitions (e.g.
    // lib/react.js), not from @types. Flow's JSX support implicitly imports
    // from the "react" builtin module, so @types/react must never shadow it.
    const AT_TYPES_EXCLUDED_PACKAGES: &[&str] = &["react"];

    fn resolve_at_types(
        options: &Options,
        transaction: &ModuleResolver<'_>,
        node_modules_containers: &BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
        phantom_acc: Option<&mut PhantomAcc>,
        importing_file: &FileKey,
        possible_node_module_container_dir: &str,
        import_specifier: &str,
    ) -> Option<Dependency> {
        if options.typescript_library_definition_support {
            let (package_name, package_subpath) = parse_package_name(import_specifier);
            if package_name.is_empty()
                || AT_TYPES_EXCLUDED_PACKAGES.contains(&package_name.as_str())
                // Don't rewrite an explicit @types/* import: e.g. `import '@types/foo'`
                // must not fall through to `@types/types__foo`.
                || package_name.starts_with("@types/")
            {
                None
            } else {
                let at_types_import = format!(
                    "{}{}",
                    at_types_package_name(&package_name),
                    package_subpath
                        .strip_prefix('.')
                        .unwrap_or(&package_subpath)
                );
                node_module(
                    options,
                    transaction,
                    node_modules_containers,
                    phantom_acc,
                    importing_file,
                    possible_node_module_container_dir,
                    &at_types_import,
                )
            }
        } else {
            None
        }
    }

    // Resolve via node_modules, falling back to @types when the resolved provider
    // is untyped. This handles the common case where a package ships JS without
    // type declarations and a separate @types package provides them.
    pub(super) fn node_module_or_at_types(
        options: &Options,
        transaction: &ModuleResolver<'_>,
        node_modules_containers: &BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
        mut phantom_acc: Option<&mut PhantomAcc>,
        importing_file: &FileKey,
        possible_node_module_container_dir: &str,
        import_specifier: &str,
    ) -> Option<Dependency> {
        let result = node_module(
            options,
            transaction,
            node_modules_containers,
            phantom_acc.as_deref_mut(),
            importing_file,
            possible_node_module_container_dir,
            import_specifier,
        );
        if !options.typescript_library_definition_support {
            result
        } else {
            let try_at_types = |phantom_acc: Option<&mut PhantomAcc>| {
                resolve_at_types(
                    options,
                    transaction,
                    node_modules_containers,
                    phantom_acc,
                    importing_file,
                    possible_node_module_container_dir,
                    import_specifier,
                )
            };
            match result {
                None => {
                    // No phantom dep needed here: the underlying [node_module] call already
                    // populated [phantom_acc] with the paths it tried, so re-resolution will
                    // trigger when any of those files appear.
                    try_at_types(phantom_acc)
                }
                Some(dep) => {
                    let is_typed = match transaction.get_provider(&dep) {
                        Some(file_addr) => transaction.is_typed_file(&file_addr),
                        None => false,
                    };
                    if is_typed {
                        Some(dep)
                    } else {
                        match try_at_types(phantom_acc.as_deref_mut()) {
                            Some(at_types_result) => {
                                // Record the original (untyped) dep as a phantom so re-resolution
                                // fires when its typed status changes (commit_modules treats
                                // untyped<->typed transitions as "changed modules"), allowing the
                                // package's own types to override @types.
                                record_phantom_dependency(
                                    dep.to_modulename(),
                                    Some(dep),
                                    phantom_acc,
                                );
                                Some(at_types_result)
                            }
                            None => Some(dep),
                        }
                    }
                }
            }
        }
    }

    pub(super) fn resolve_root_relative(
        options: &Options,
        transaction: &ModuleResolver<'_>,
        mut phantom_acc: Option<&mut PhantomAcc>,
        importing_file: &FileKey,
        import_specifier: &str,
    ) -> Option<Dependency> {
        if !options.node_resolver_allow_root_relative {
            return None;
        }

        let dirnames = &options.node_resolver_root_relative_dirnames;
        let root = options.root.to_string_lossy();

        for (applicable_dirname_opt, dirname) in dirnames.iter() {
            let relative_to_directory = if dirname.is_empty() {
                root.to_string()
            } else {
                files::normalize_path(&root, dirname)
            };

            let applicable = applicable_dirname_opt
                .as_ref()
                .map(|prefix| files::is_file_key_prefix(prefix, importing_file.suffix()))
                .unwrap_or(true);

            if applicable {
                if let Some(result) = resolve_relative(
                    options,
                    transaction,
                    phantom_acc.as_deref_mut(),
                    importing_file,
                    &relative_to_directory,
                    None,
                    import_specifier,
                ) {
                    return Some(result);
                }
            }
        }

        None
    }

    fn resolve_import(
        options: &Options,
        transaction: &ModuleResolver<'_>,
        node_modules_containers: &BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
        importing_file: &FileKey,
        mut phantom_acc: Option<&mut PhantomAcc>,
        import_specifier: &str,
    ) -> Option<Dependency> {
        let file = importing_file.to_absolute();
        let importing_file_dir = Path::new(&file).parent()?.to_str()?;

        if super::is_relative_or_absolute(import_specifier) {
            resolve_relative(
                options,
                transaction,
                phantom_acc.as_deref_mut(),
                importing_file,
                importing_file_dir,
                None,
                import_specifier,
            )
        } else {
            resolve_root_relative(
                options,
                transaction,
                phantom_acc.as_deref_mut(),
                importing_file,
                import_specifier,
            )
            .or_else(|| {
                flow_typed_module(
                    options,
                    transaction,
                    phantom_acc.as_deref_mut(),
                    importing_file,
                    import_specifier,
                )
            })
            .or_else(|| {
                node_module_or_at_types(
                    options,
                    transaction,
                    node_modules_containers,
                    phantom_acc,
                    importing_file,
                    importing_file_dir,
                    import_specifier,
                )
            })
        }
    }

    fn imported_module_impl(
        options: &Options,
        transaction: &ModuleResolver<'_>,
        node_modules_containers: &BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
        importing_file: &FileKey,
        mut phantom_acc: Option<&mut PhantomAcc>,
        import_specifier: &FlowImportSpecifier,
    ) -> Result<Dependency, Option<FlowImportSpecifier>> {
        match import_specifier {
            FlowImportSpecifier::Userland(userland) => {
                let candidates = super::module_name_candidates(options, userland.as_str());

                for candidate in candidates.as_slice() {
                    if let Some(m) = resolve_import(
                        options,
                        transaction,
                        node_modules_containers,
                        importing_file,
                        phantom_acc.as_deref_mut(),
                        candidate,
                    ) {
                        return Ok(m);
                    }
                }

                Err(None)
            }
        }
    }

    fn choose_provider_impl(
        options: &Options,
        m: &str,
        files: Vec<FileKey>,
        errmap: &mut BTreeMap<FlowSmolStr, (FileKey, Vec1<FileKey>)>,
    ) -> Option<FileKey> {
        let m = FlowSmolStr::new(m);
        let fallback = || None;
        super::choose_provider_and_warn_about_duplicates(options, &m, errmap, files, fallback)
    }
}

mod haste {
    use std::path::Path;

    use flow_common::files;
    use flow_common::flow_import_specifier::FlowImportSpecifier;
    use flow_common::options::Options;
    use flow_common_modulename::HasteModuleInfo;
    use flow_common_modulename::Modulename;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_parser::file_key::FileKey;
    use flow_parser::file_key::FileKeyInner;
    use vec1::Vec1;

    use super::*;

    pub struct Haste;

    impl super::ModuleSystemSig for Haste {
        fn exported_module(
            options: &Options,
            file: &FileKey,
            package_info: &PackageInfo,
        ) -> Option<HasteModuleInfo> {
            exported_module_impl(options, file, package_info)
        }

        fn imported_module(
            options: &Options,
            transaction: &ModuleResolver<'_>,
            node_modules_containers: &BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
            importing_file: &FileKey,
            phantom_acc: Option<&mut PhantomAcc>,
            import_specifier: &FlowImportSpecifier,
        ) -> Result<Dependency, Option<FlowImportSpecifier>> {
            imported_module_impl(
                options,
                transaction,
                node_modules_containers,
                importing_file,
                phantom_acc,
                import_specifier,
            )
        }

        fn choose_provider(
            options: &Options,
            m: &str,
            errmap: &mut BTreeMap<FlowSmolStr, (FileKey, Vec1<FileKey>)>,
            files: Vec<FileKey>,
            _fallback: impl FnOnce() -> Option<FileKey>,
        ) -> Option<FileKey> {
            choose_provider_impl(options, m, files, errmap)
        }
    }

    fn short_module_name_of(file: &FileKey) -> String {
        let path = file.as_str();
        let basename = Path::new(path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(path);

        Path::new(basename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(basename)
            .to_string()
    }

    fn is_mock(file: &FileKey) -> bool {
        let path = file.as_str();
        path.replace('\\', "/").contains("/__mocks__/")
    }

    fn is_within_node_modules(options: &Options, path: &str) -> bool {
        let root = &options.root;
        let file_options = &options.file_options;
        files::is_within_node_modules(root.as_path(), file_options, path)
    }

    fn exported_module_impl(
        options: &Options,
        file: &FileKey,
        package_info: &PackageInfo,
    ) -> Option<HasteModuleInfo> {
        let abs_path = file.to_absolute();
        match file.inner() {
            FileKeyInner::SourceFile(_) => {
                if is_mock(file) {
                    Some(HasteModuleInfo::mk(short_module_name_of(file).into()))
                } else {
                    files::haste_name_opt(&options.file_options, file)
                        .map(|name| HasteModuleInfo::mk(name.into()))
                }
            }
            FileKeyInner::JsonFile(_) => {
                if let PackageInfo(Some(pkg)) = package_info {
                    if pkg.haste_commonjs() || !is_within_node_modules(options, &abs_path) {
                        pkg.name().map(|name| HasteModuleInfo::mk(name.dupe()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn package_dir_opt(transaction: &ModuleResolver<'_>, file: &FileKey) -> Option<String> {
        if transaction.is_package_file(file) {
            use std::path::Path;
            let abs = file.to_absolute();
            Path::new(&abs)
                .parent()
                .and_then(|p| p.to_str())
                .map(|s| s.to_string())
        } else {
            None
        }
    }

    fn resolve_haste_module(
        options: &Options,
        transaction: &ModuleResolver<'_>,
        mut phantom_acc: Option<&mut PhantomAcc>,
        importing_file: &FileKey,
        importing_file_dir: &str,
        import_specifier: &str,
    ) -> Option<Dependency> {
        let (name, subpath) = parse_haste_name(import_specifier);

        let mut resolve = || -> Option<Dependency> {
            let haste_info = HasteModuleInfo::mk(name.clone().into());
            let mname = Modulename::Haste(haste_info.clone());
            let dependency = transaction.get_dependency(&mname);

            if let Some(provider_file) = dependency
                .as_ref()
                .and_then(|dep| transaction.get_provider(dep))
            {
                match (
                    package_dir_opt(transaction, &provider_file),
                    subpath.as_slice(),
                ) {
                    (Some(package_dir), []) => super::node::resolve_package(
                        options,
                        transaction,
                        phantom_acc.as_deref_mut(),
                        None,
                        &package_dir,
                    ),
                    (Some(package_dir), _) => {
                        record_phantom_dependency(
                            mname.clone(),
                            dependency.clone(),
                            phantom_acc.as_deref_mut(),
                        );

                        let subpath_strs: Vec<&str> = subpath.iter().map(|s| s.as_str()).collect();
                        let path = files::construct_path(&package_dir, &subpath_strs);
                        super::node::resolve_relative(
                            options,
                            transaction,
                            phantom_acc.as_deref_mut(),
                            importing_file,
                            importing_file_dir,
                            None,
                            &path,
                        )
                    }
                    (None, []) => dependency,
                    (None, _) => {
                        record_phantom_dependency(mname, dependency, phantom_acc.as_deref_mut());
                        None
                    }
                }
            } else {
                record_phantom_dependency(mname, dependency, phantom_acc.as_deref_mut());
                None
            }
        };

        resolve()
    }

    fn parse_haste_name(import_specifier: &str) -> (String, Vec<String>) {
        let parts: Vec<&str> = import_specifier.split('/').collect();

        if parts.is_empty() {
            return (import_specifier.to_string(), vec![]);
        }

        if parts[0].starts_with('@') && parts.len() > 1 {
            let name = format!("{}/{}", parts[0], parts[1]);
            let subpath = parts[2..].iter().map(|s| s.to_string()).collect();
            (name, subpath)
        } else {
            let name = parts[0].to_string();
            let subpath = parts[1..].iter().map(|s| s.to_string()).collect();
            (name, subpath)
        }
    }

    fn resolve_haste_module_disallow_platform_specific(
        options: &Options,
        transaction: &ModuleResolver<'_>,
        mut phantom_acc: Option<&mut PhantomAcc>,
        importing_file: &FileKey,
        import_specifier: &str,
    ) -> Option<Dependency> {
        let importing_file_abs = importing_file.to_absolute();
        let importing_file_dir = Path::new(&importing_file_abs)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("");

        let dependency = resolve_haste_module(
            options,
            transaction,
            phantom_acc.as_deref_mut(),
            importing_file,
            importing_file_dir,
            import_specifier,
        );

        let file_options = &options.file_options;
        if file_options.multi_platform {
            if let Some(dep) = &dependency {
                if let Some(haste_module_info) = dep.as_haste_module_info() {
                    let module_name = haste_module_info.module_name();
                    let multi_platform_extensions = &file_options.multi_platform_extensions;

                    let is_platform_specific = multi_platform_extensions
                        .iter()
                        .any(|ext| module_name.ends_with(ext.as_str()));

                    if is_platform_specific {
                        record_phantom_dependency(
                            dep.to_modulename(),
                            dependency.clone(),
                            phantom_acc,
                        );
                        return None;
                    }
                }
            }
        }

        dependency
    }

    fn resolve_import(
        options: &Options,
        transaction: &ModuleResolver<'_>,
        node_modules_containers: &BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
        importing_file: &FileKey,
        mut phantom_acc: Option<&mut PhantomAcc>,
        import_specifier: &str,
    ) -> Option<Dependency> {
        let importing_file_abs = importing_file.to_absolute();
        let importing_file_dir = Path::new(&importing_file_abs)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("");

        if super::is_relative_or_absolute(import_specifier) {
            return super::node::resolve_relative(
                options,
                transaction,
                phantom_acc.as_deref_mut(),
                importing_file,
                importing_file_dir,
                None,
                import_specifier,
            );
        }

        let file_options = &options.file_options;
        let projects_options = &options.projects_options;

        if !file_options.multi_platform {
            resolve_haste_module(
                options,
                transaction,
                phantom_acc.as_deref_mut(),
                importing_file,
                importing_file_dir,
                import_specifier,
            )
            .or_else(|| {
                super::node::resolve_root_relative(
                    options,
                    transaction,
                    phantom_acc.as_deref_mut(),
                    importing_file,
                    import_specifier,
                )
            })
            .or_else(|| {
                super::node::flow_typed_module(
                    options,
                    transaction,
                    phantom_acc.as_deref_mut(),
                    importing_file,
                    import_specifier,
                )
            })
            .or_else(|| {
                super::node::node_module_or_at_types(
                    options,
                    transaction,
                    node_modules_containers,
                    phantom_acc.as_deref_mut(),
                    importing_file,
                    importing_file_dir,
                    import_specifier,
                )
            })
        } else {
            let ordered_platforms = super::node::ordered_allowed_implicit_platform_specific_import(
                file_options,
                projects_options,
                importing_file.as_str(),
                None,
            );

            match ordered_platforms {
                None => resolve_haste_module_disallow_platform_specific(
                    options,
                    transaction,
                    phantom_acc.as_deref_mut(),
                    importing_file,
                    import_specifier,
                )
                .or_else(|| {
                    super::node::resolve_root_relative(
                        options,
                        transaction,
                        phantom_acc.as_deref_mut(),
                        importing_file,
                        import_specifier,
                    )
                })
                .or_else(|| {
                    super::node::flow_typed_module(
                        options,
                        transaction,
                        phantom_acc.as_deref_mut(),
                        importing_file,
                        import_specifier,
                    )
                })
                .or_else(|| {
                    super::node::node_module_or_at_types(
                        options,
                        transaction,
                        node_modules_containers,
                        phantom_acc.as_deref_mut(),
                        importing_file,
                        importing_file_dir,
                        import_specifier,
                    )
                }),
                Some(ordered_platforms) => {
                    for platform in &ordered_platforms {
                        let platform_specifier = format!("{}.{}", import_specifier, platform);
                        if let Some(result) = resolve_haste_module(
                            options,
                            transaction,
                            phantom_acc.as_deref_mut(),
                            importing_file,
                            importing_file_dir,
                            &platform_specifier,
                        ) {
                            return Some(result);
                        }
                    }

                    resolve_haste_module_disallow_platform_specific(
                        options,
                        transaction,
                        phantom_acc.as_deref_mut(),
                        importing_file,
                        import_specifier,
                    )
                    .or_else(|| {
                        super::node::resolve_root_relative(
                            options,
                            transaction,
                            phantom_acc.as_deref_mut(),
                            importing_file,
                            import_specifier,
                        )
                    })
                    .or_else(|| {
                        super::node::flow_typed_module(
                            options,
                            transaction,
                            phantom_acc.as_deref_mut(),
                            importing_file,
                            import_specifier,
                        )
                    })
                    .or_else(|| {
                        super::node::node_module_or_at_types(
                            options,
                            transaction,
                            node_modules_containers,
                            phantom_acc,
                            importing_file,
                            importing_file_dir,
                            import_specifier,
                        )
                    })
                }
            }
        }
    }

    fn imported_module_impl(
        options: &Options,
        transaction: &ModuleResolver<'_>,
        node_modules_containers: &BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
        importing_file: &FileKey,
        phantom_acc: Option<&mut PhantomAcc>,
        import_specifier: &FlowImportSpecifier,
    ) -> Result<Dependency, Option<FlowImportSpecifier>> {
        match import_specifier {
            FlowImportSpecifier::Userland(userland) => {
                let candidates = super::module_name_candidates(options, userland.as_str());
                let first_candidate = candidates.first();

                if let Some(m) = resolve_import(
                    options,
                    transaction,
                    node_modules_containers,
                    importing_file,
                    phantom_acc,
                    first_candidate,
                ) {
                    return Ok(m);
                }

                let mapped_name = if candidates.len() > 1 {
                    Some(FlowImportSpecifier::userland(FlowSmolStr::new(
                        first_candidate,
                    )))
                } else {
                    None
                };

                Err(mapped_name)
            }
        }
    }

    fn choose_provider_impl(
        options: &Options,
        m: &str,
        mut files: Vec<FileKey>,
        errmap: &mut BTreeMap<FlowSmolStr, (FileKey, Vec1<FileKey>)>,
    ) -> Option<FileKey> {
        if files.is_empty() {
            return None;
        }

        if files.len() == 1 {
            return Some(files.remove(0));
        }

        let (mocks, non_mocks): (Vec<_>, Vec<_>) = files.into_iter().partition(is_mock);

        let fallback = || mocks.first().map(|k| k.dupe());

        choose_provider_and_warn_about_duplicates(
            options,
            &FlowSmolStr::new(m),
            errmap,
            non_mocks,
            fallback,
        )
    }
}

pub type ErrorMap = BTreeMap<FlowSmolStr, (FileKey, Vec1<FileKey>)>;

pub fn exported_module(
    options: &Options,
    file: &FileKey,
    package_info: &PackageInfo,
) -> Option<HasteModuleInfo> {
    match options.module_system {
        ModuleSystem::Node => None,
        ModuleSystem::Haste => haste::Haste::exported_module(options, file, package_info),
    }
}

pub fn imported_module(
    options: &Options,
    transaction: &Transaction,
    node_modules_containers: &BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
    importing_file: &FileKey,
    phantom_acc: Option<&mut PhantomAcc>,
    import_specifier: &FlowImportSpecifier,
) -> Result<Dependency, Option<FlowImportSpecifier>> {
    let resolver = ModuleResolver::new(options, transaction);
    imported_module_with_resolver(
        options,
        &resolver,
        node_modules_containers,
        importing_file,
        phantom_acc,
        import_specifier,
    )
}

fn imported_module_with_resolver(
    options: &Options,
    transaction: &ModuleResolver<'_>,
    node_modules_containers: &BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
    importing_file: &FileKey,
    phantom_acc: Option<&mut PhantomAcc>,
    import_specifier: &FlowImportSpecifier,
) -> Result<Dependency, Option<FlowImportSpecifier>> {
    // `react` is always provided by Flow's builtin libdefs, even if a project installs it.
    if matches!(
        import_specifier,
        FlowImportSpecifier::Userland(userland) if userland.as_str() == "react"
    ) {
        return Err(None);
    }

    match options.module_system {
        ModuleSystem::Node => node::Node::imported_module(
            options,
            transaction,
            node_modules_containers,
            importing_file,
            phantom_acc,
            import_specifier,
        ),
        ModuleSystem::Haste => haste::Haste::imported_module(
            options,
            transaction,
            node_modules_containers,
            importing_file,
            phantom_acc,
            import_specifier,
        ),
    }
}

pub fn choose_provider(
    options: &Options,
    m: &str,
    files: Vec<FileKey>,
    errmap: &mut ErrorMap,
) -> Option<FileKey> {
    match options.module_system {
        ModuleSystem::Node => node::Node::choose_provider(options, m, errmap, files, || None),
        ModuleSystem::Haste => haste::Haste::choose_provider(options, m, errmap, files, || None),
    }
}

pub fn add_parsed_resolved_requires(
    options: &Options,
    transaction: &Transaction,
    node_modules_containers: &BTreeMap<FlowSmolStr, BTreeSet<FlowSmolStr>>,
    file: &FileKey,
) -> Result<(), String> {
    use flow_heap::resolved_requires::Dependency;
    use flow_heap::resolved_requires::ResolvedModule;
    use flow_heap::resolved_requires::ResolvedRequires;

    let resolver = ModuleResolver::new(options, transaction);
    let requires = resolver.get_requires_unsafe(file);

    let phantom_acc_map = std::collections::BTreeMap::new();
    let mut phantom_acc = PhantomAcc(phantom_acc_map);

    let resolved_modules: Vec<ResolvedModule> = requires
        .iter()
        .map(|import_specifier| {
            match imported_module_with_resolver(
                options,
                &resolver,
                node_modules_containers,
                file,
                Some(&mut phantom_acc),
                import_specifier,
            ) {
                Ok(dependency) => transaction.resolved_module_for_dependency(&dependency),
                Err(None) => ResolvedModule::null(),
                Err(Some(spec)) => ResolvedModule::from_result(Err(Some(spec))),
            }
        })
        .collect();

    let phantom_dependencies: Vec<Dependency> = phantom_acc
        .0
        .into_iter()
        .map(|(mname, dep_opt)| match dep_opt {
            Some(dep) => dep,
            None => resolver.intern_dependency_from_modulename(mname),
        })
        .collect();

    let resolved_requires = ResolvedRequires::new(resolved_modules, phantom_dependencies);

    transaction.set_resolved_requires(file, resolved_requires);

    Ok(())
}

pub fn commit_modules(
    pool: &flow_utils_concurrency::thread_pool::ThreadPool,
    options: &Options,
    transaction: &Transaction,
    dirty_modules: flow_common_modulename::ModulenameSet,
) -> (
    flow_common_modulename::ModulenameSet,
    BTreeMap<FlowSmolStr, (FileKey, Vec1<FileKey>)>,
) {
    use flow_common_modulename::HasteModuleInfo;
    use flow_common_modulename::ModulenameSet;

    let debug = options.debug;
    if debug {
        flow_hh_logger::info!("Committing modules");
    }

    fn commit_haste(
        options: &Options,
        transaction: &Transaction,
        reader: &HeapReader<'_>,
        debug: bool,
        unchanged: &mut ModulenameSet,
        errmap: &mut BTreeMap<FlowSmolStr, (FileKey, Vec1<FileKey>)>,
        mname: &Modulename,
        haste_info: &HasteModuleInfo,
    ) {
        let name = haste_info.module_name();
        let haste_module = reader
            .get_haste_module(haste_info)
            .unwrap_or_else(|| panic!("Haste module not found: {:?}", haste_info));

        let all_providers = reader
            .haste_provider_candidates(haste_info)
            .into_iter()
            .collect();

        let old_provider = haste_module.get_provider();

        let new_provider = choose_provider(options, name, all_providers, errmap);

        match (&old_provider, &new_provider) {
            (_, None) => {
                if debug {
                    eprintln!("no remaining providers: {}", name);
                }
                transaction.set_haste_module_provider(haste_info, None);
            }
            (None, Some(p)) => {
                transaction.set_haste_module_provider(haste_info, Some(p.dupe()));
            }
            (Some(old_p), Some(new_p)) => {
                if old_p == new_p {
                    if debug {
                        eprintln!("unchanged provider: {} -> {}", name, new_p.as_str());
                    }
                    let old_typed =
                        reader
                            .get_parse_committed(new_p)
                            .and_then(|parse| match parse {
                                Parse::Typed(typed) => Some(typed),
                                _ => None,
                            });
                    let new_typed = reader.get_parse(new_p).and_then(|parse| match parse {
                        Parse::Typed(typed) => Some(typed),
                        _ => None,
                    });

                    match (old_typed, new_typed) {
                        (Some(_), None) | (None, Some(_)) => {}
                        _ => {
                            unchanged.insert(mname.clone());
                        }
                    }
                } else {
                    if debug {
                        eprintln!(
                            "new provider: {} -> {} replaces {}",
                            name,
                            new_p.as_str(),
                            old_p.as_str()
                        );
                    }
                    transaction.set_haste_module_provider(haste_info, Some(new_p.dupe()));
                }
            }
        }
    }

    fn commit_file(
        reader: &HeapReader<'_>,
        unchanged: &mut ModulenameSet,
        mname: &Modulename,
        file_key: &FileKey,
    ) {
        let get_parses = |key: &FileKey| -> (Option<Parse>, Option<Parse>) {
            (reader.get_parse_committed(key), reader.get_parse(key))
        };

        let decl_key = file_key.with_suffix(files::FLOW_EXT);
        let (old_decl_parse, new_decl_parse) = get_parses(&decl_key);
        let (old_impl_parse, new_impl_parse) = get_parses(file_key);

        match (
            old_decl_parse,
            new_decl_parse,
            old_impl_parse,
            new_impl_parse,
        ) {
            (None, None, None, None) => {
                unchanged.insert(mname.clone());
            }
            (None, Some(_), _, _)
            | (Some(_), None, _, _)
            | (None, None, Some(_), None)
            | (None, None, None, Some(_)) => {}
            (Some(old_parse), Some(new_parse), _, _)
            | (None, None, Some(old_parse), Some(new_parse)) => {
                let old_typed = matches!(old_parse, Parse::Typed(_));
                let new_typed = matches!(new_parse, Parse::Typed(_));

                match (old_typed, new_typed) {
                    (true, false) | (false, true) => {}
                    _ if reader.get_package_info_committed(file_key)
                        != reader.get_package_info(file_key) => {}
                    _ => {
                        unchanged.insert(mname.clone());
                    }
                }
            }
        }
    }

    const BATCH_SIZE: usize = 500;
    let modules: Vec<&Modulename> = dirty_modules.iter().collect();
    let work_items: Vec<Vec<&Modulename>> = modules
        .chunks(BATCH_SIZE)
        .map(|batch| batch.to_vec())
        .collect();

    let (unchanged, errmap) = flow_utils_concurrency::map_reduce::fold(
        pool,
        work_items,
        |acc: &mut (
            ModulenameSet,
            BTreeMap<FlowSmolStr, (FileKey, Vec1<FileKey>)>,
        ),
         batch| {
            let (unchanged, errmap) = acc;
            let access = transaction.latest_heap_reader();
            let reader = access.reader();
            for &mname in batch {
                match mname {
                    Modulename::Haste(haste_info) => {
                        commit_haste(
                            options,
                            transaction,
                            &reader,
                            debug,
                            unchanged,
                            errmap,
                            mname,
                            haste_info,
                        );
                    }
                    Modulename::Filename(file_key) => {
                        commit_file(&reader, unchanged, mname, file_key);
                    }
                }
            }
        },
        |acc: &mut (
            ModulenameSet,
            BTreeMap<FlowSmolStr, (FileKey, Vec1<FileKey>)>,
        ),
         other: (
            ModulenameSet,
            BTreeMap<FlowSmolStr, (FileKey, Vec1<FileKey>)>,
        )| {
            let (unchanged, errmap) = acc;
            let (other_unchanged, other_errmap) = other;
            unchanged.extend(other_unchanged);
            errmap.extend(other_errmap);
        },
    );

    let changed_modules: ModulenameSet = dirty_modules
        .into_iter()
        .filter(|m| !unchanged.contains(m))
        .collect();

    if debug {
        flow_hh_logger::info!("Committing modules Done");
    }

    (changed_modules, errmap)
}
