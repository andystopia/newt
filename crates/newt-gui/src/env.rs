// use crate::search::PackageSearchValue;

// pub struct PackageConstraints {
//     package_search: PackageSearchValue,
//     nixpkgs_version: String,
// }

// pub struct Environment {
//     packages: Vec<PackageConstraints>,
// }

// impl Environment {
//     pub fn insert(&mut self, package_search: PackageSearchValue, nixpkgs_version: String) {
//         self.packages.push(PackageConstraints {
//             package_search,
//             nixpkgs_version,
//         })
//     }
// }

use floem::{
    cosmic_text::Weight,
    reactive::{create_memo, RwSignal},
    style::FontWeight,
    unit::Pct,
    view::View,
    views::{self, container, dyn_stack, empty, label, v_stack, Container, Decorators},
};

use crate::{instr, theme};

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug, Hash)]
pub enum EnvEntryKind {
    Simple { attr_name: String },
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug, Hash)]
pub struct EnvEntry {
    kind: EnvEntryKind,
    children: EnvironmentEntries,
}

impl EnvEntry {
    pub fn view(&self) -> impl View {
        match &self.kind {
            EnvEntryKind::Simple { attr_name } => {
                let attr_name = attr_name.to_owned();
                label(move || attr_name.to_owned())
                    .style(|s| s.color(theme().fg).font_weight(Weight::SEMIBOLD))
            }
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug, Default, Hash)]
pub struct EnvironmentEntries {
    entries: Vec<EnvEntry>,
}

impl EnvironmentEntries {
    pub fn push_simple(&mut self, simple: &str) {
        self.entries.push(EnvEntry {
            kind: EnvEntryKind::Simple {
                attr_name: simple.to_owned(),
            },
            children: Default::default(),
        })
    }
}

pub fn with_border(view: impl View + 'static, last: bool) -> impl View {
    let src = if last {
        views::svg(|| instr!("../../../assets/final-corner.svg").to_owned())
            .style(|s| s.height_full().aspect_ratio(1.0))
    } else {
        views::svg(|| instr!("../../../assets/inline-element.svg").to_owned())
            .style(|s| s.height_full().aspect_ratio(1.0))
    };
    floem::views::h_stack((
        src.style(|s| s.color(theme().fg.with_alpha_factor(0.2))),
        view,
    ))
    .style(|s| s.gap(0.0, 0.0).items_start().min_height(0))
}
impl EnvironmentEntries {
    pub fn view(this: RwSignal<Self>) -> impl View {
        let len = create_memo(move |_| this.get().entries.len());
        dyn_stack(
            move || this.get().entries.into_iter().enumerate(),
            ToOwned::to_owned,
            move |(idx, t)| {
                let top = empty().style(|s| s.height(3.0));
                let middle = t.view();
                let bottom = empty().style(|s| s.height(3.0));
                with_border(v_stack((top, middle, bottom)), idx == len.get() - 1)
            },
        )
        .style(|s| s.flex().flex_col().padding_horiz(10.0))
    }
}
