use crate::search::PackageSearchValue;

pub struct PackageConstraints {
    package_search: PackageSearchValue,
    nixpkgs_version: String,
}

pub struct Environment {
    packages: Vec<PackageConstraints>,
}

impl Environment {
    pub fn insert(&mut self, package_search: PackageSearchValue, nixpkgs_version: String) {
        self.packages.push(PackageConstraints {
            package_search,
            nixpkgs_version,
        })
    }
}
