# elb-dl

[![Crates.io Version](https://img.shields.io/crates/v/elb-dl)](https://crates.io/crates/elb-dl)
[![Docs](https://docs.rs/elb-dl/badge.svg)](https://docs.rs/elb-dl)
[![dependency status](https://deps.rs/repo/github/igankevich/elb-dl/status.svg)](https://deps.rs/repo/github/igankevich/elb-dl)

A library that resolves ELF dependencies without loading and executing them.

Based on [`elb`](https://docs.rs/elb) crate.


## Examples

### Resolve immediate dependencies

```rust
use elb_dl::{DependencyTree, DynamicLoader, Error, glibc};

fn resolve_immediate() -> Result<(), Error> {
    let loader = DynamicLoader::options()
        .search_dirs(glibc::get_search_dirs("/")?)
        .new_loader();
    let mut tree = DependencyTree::new();
    let deps = loader.resolve_dependencies("/bin/sh", &mut tree)?;
    for path in deps.iter() {
        eprintln!("{:?}", path);
    }
    Ok(())
}
```


### Resolve dependencies recursively

```rust
use elb_dl::{DependencyTree, DynamicLoader, Error, glibc};
use std::collections::{BTreeSet, VecDeque};
use std::path::Path;

fn resolve_all() -> Result<(), Error> {
    let loader = DynamicLoader::options()
        .search_dirs(glibc::get_search_dirs("/")?)
        .new_loader();
    let mut tree = DependencyTree::new();
    let mut queue = VecDeque::new();
    queue.push_back(Path::new("/bin/sh").to_path_buf());
    while let Some(path) = queue.pop_front() {
        let deps = loader.resolve_dependencies(&path, &mut tree)?;
        queue.extend(deps);
    }
    // Print dependency table.
    for (dependent, dependencies) in tree.iter() {
        eprintln!("{dependent:?} => {dependencies:?}");
    }
    Ok(())
}
```
