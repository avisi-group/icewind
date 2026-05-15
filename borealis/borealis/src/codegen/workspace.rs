use {
    cargo_util_schemas::manifest::{
        InheritableDependency, InheritableField, PackageName, TomlDependency,
        TomlDetailedDependency, TomlManifest, TomlPackage,
    },
    common::{intern::InternedString, HashMap, HashSet},
    semver::{BuildMetadata, Prerelease, Version},
    std::{
        collections::{BTreeMap, BTreeSet},
        fs::{self, read_to_string},
        path::{Path, PathBuf},
    },
    walkdir::WalkDir,
};

pub fn write_workspace(target_fs: (HashMap<PathBuf, String>, HashSet<PathBuf>), path: PathBuf) {
    log::debug!("reading current workspace");
    let current_fs = read_fs(&path);

    log::info!(
        "writing workspace: {} files, {} folders",
        target_fs.0.len(),
        target_fs.1.len()
    );

    write_difference(target_fs, current_fs, &path);

    log::debug!("done writing workspace");
}

/// Reads files, their contents, and directories at the supplied path'
fn read_fs(path: &Path) -> (HashMap<PathBuf, String>, HashSet<PathBuf>) {
    if !path.exists() {
        return Default::default();
    }

    let files = WalkDir::new(path)
        .into_iter()
        .filter(|entry| entry.as_ref().unwrap().file_type().is_file())
        .map(|entry| {
            let entry = entry.unwrap();
            let adjusted_path = entry.path().strip_prefix(path).unwrap().to_owned();
            (
                adjusted_path,
                read_to_string(entry.path()).unwrap_or_default(),
            )
        })
        .collect();

    let dirs = WalkDir::new(path)
        .into_iter()
        .filter(|entry| entry.as_ref().unwrap().file_type().is_dir())
        .map(|entry| entry.unwrap().path().strip_prefix(path).unwrap().to_owned())
        .filter(|path| path.ends_with("src"))
        .collect();

    (files, dirs)
}

fn write_difference(
    (target_files, target_dirs): (HashMap<PathBuf, String>, HashSet<PathBuf>),
    (mut current_files, mut current_dirs): (HashMap<PathBuf, String>, HashSet<PathBuf>),
    workspace_path: &Path,
) {
    {
        for path in target_dirs {
            if !current_dirs.remove(&path) {
                log::info!("new dir @ {path:?}");
                fs::create_dir_all(workspace_path.join(path)).unwrap();
            }
        }

        for path in current_dirs {
            log::info!("delete dir @ {path:?}");
            fs::remove_dir_all(workspace_path.join(path)).unwrap();
        }
    }

    {
        for (path, target_contents) in target_files {
            if let Some(contents) = current_files.remove(&path) {
                if *contents != target_contents {
                    log::info!("diff @ {path:?}");
                    fs::write(workspace_path.join(path), target_contents).unwrap();
                }
            } else {
                log::info!("new @ {path:?}");
                fs::write(workspace_path.join(path), target_contents).unwrap();
            }
        }

        for (path, _) in current_files {
            log::info!("delete @ {path:?}");
            // hide errors if parent dir was already deleted
            fs::remove_file(workspace_path.join(path)).ok();
        }
    }
}

pub fn create_manifest(
    name: InternedString,
    dependencies: &HashSet<InternedString>,
) -> TomlManifest {
    TomlManifest {
        cargo_features: None,
        package: Some(Box::new(TomlPackage {
            edition: Some(InheritableField::Value("2024".to_owned())),
            rust_version: None,
            name: PackageName::new(name.as_ref().to_owned()).unwrap(),
            version: Some(InheritableField::Value(Version {
                major: 0,
                minor: 0,
                patch: 0,
                pre: Prerelease::EMPTY,
                build: BuildMetadata::EMPTY,
            })),
            authors: None,
            build: None,
            metabuild: None,
            default_target: None,
            forced_target: None,
            links: None,
            exclude: None,
            include: None,
            publish: None,
            workspace: None,
            im_a_teapot: None,
            autobins: None,
            autoexamples: None,
            autotests: None,
            autobenches: None,
            default_run: None,
            description: None,
            homepage: None,
            documentation: None,
            readme: None,
            keywords: None,
            categories: None,
            license: None,
            license_file: None,
            repository: None,
            resolver: None,
            metadata: None,
            _invalid_cargo_features: None,
        })),
        project: None,
        profile: None,
        lib: None,
        bin: None,
        example: None,
        test: None,
        bench: None,
        dependencies: Some(
            dependencies
                .iter()
                .map(|n| (n, PackageName::new(n.to_string()).unwrap()))
                .map(|(n, package_name)| {
                    (
                        package_name,
                        InheritableDependency::Value(TomlDependency::Detailed(
                            TomlDetailedDependency {
                                version: None,
                                registry: None,
                                registry_index: None,
                                path: Some(format!("../{n}")),
                                git: None,
                                branch: None,
                                tag: None,
                                rev: None,
                                features: None,
                                optional: None,
                                default_features: None,
                                default_features2: None,
                                package: None,
                                public: None,
                                artifact: None,
                                lib: None,
                                target: None,
                                _unused_keys: BTreeMap::new(),
                            },
                        )),
                    )
                })
                .chain([
                    (
                        PackageName::new("log".to_owned()).unwrap(),
                        InheritableDependency::Value(TomlDependency::Simple("0.4.29".to_owned())),
                    ),
                    (
                        PackageName::new("num-rational".to_owned()).unwrap(),
                        InheritableDependency::Value(TomlDependency::Detailed(
                            TomlDetailedDependency {
                                version: Some("0.4.2".to_owned()),
                                registry: None,
                                registry_index: None,
                                path: None,
                                git: None,
                                branch: None,
                                tag: None,
                                rev: None,
                                features: None,
                                optional: None,
                                default_features: Some(false),
                                default_features2: None,
                                package: None,
                                public: None,
                                artifact: None,
                                lib: None,
                                target: None,
                                _unused_keys: BTreeMap::new(),
                            },
                        )),
                    ),
                    (
                        PackageName::new("plugins_api".to_owned()).unwrap(),
                        InheritableDependency::Value(TomlDependency::Detailed(
                            TomlDetailedDependency {
                                version: None,
                                registry: None,
                                registry_index: None,
                                path: Some("../../brig/brig/plugins_api".into()),
                                git: None,
                                branch: None,
                                tag: None,
                                rev: None,
                                features: None,
                                optional: None,
                                default_features: Some(false),
                                default_features2: None,
                                package: None,
                                public: None,
                                artifact: None,
                                lib: None,
                                target: None,
                                _unused_keys: BTreeMap::new(),
                            },
                        )),
                    ),
                ])
                .collect(),
        ),
        dev_dependencies: None,
        dev_dependencies2: None,
        build_dependencies: None,
        build_dependencies2: None,
        features: None,
        target: None,
        replace: None,
        patch: None,
        workspace: None,
        badges: None,
        lints: None,
        _unused_keys: BTreeSet::new(),
    }
}
