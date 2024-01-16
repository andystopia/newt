#![allow(dead_code, unused_imports, unused_macros)]
mod actor;
mod env;
mod search;
mod tailwind;
mod theme;

use std::borrow::Cow;
use std::sync::Arc;
use std::{collections::HashMap, process::Stdio};

use actor::ActorThread;
use floem::action::exec_after;
use floem::cosmic_text::Weight;

use floem::cosmic_text::Style as TextStyle;
use floem::event::Event;
use floem::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use floem::peniko::Color;
use floem::reactive::{create_effect, create_trigger};
use floem::reactive::{create_memo, create_rw_signal, ReadSignal, RwSignal};
use floem::style::{AlignContent, AlignItems, CursorStyle, FlexWrap, Style};
use floem::style::{Display, StyleValue};
use floem::taffy::prelude::Line;
use floem::taffy::style::{LengthPercentage, LengthPercentageAuto, TrackSizingFunction};
use floem::taffy::style_helpers::{minmax, TaffyAuto};
use floem::unit::{Pct, Px};
use floem::view::View;
use floem::views::{
    container_box, drag_window_area, dyn_container, dyn_stack, h_stack_from_iter, list, text_input,
    tooltip,
};
use floem::views::{static_label, v_stack_from_iter};
use floem::window::WindowConfig;
use floem::{quit_app, views};

use inline_tweak::tweak;
use once_cell::sync::Lazy;
use regex::Regex;
use search::{PackageSearchValue, PackageSupport};
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use tap::Pipe;

use floem::{
    event::EventListener,
    style::Position,
    views::{container, h_stack, label, scroll, v_stack, Decorators},
};

use bstr::ByteSlice;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NixFlakeInfo {
    pub templates: HashMap<String, NixTemplateDescription>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NixTemplateDescription {
    pub description: String,
    #[serde(rename = "type")]
    pub type_field: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NixTemplateInfo {
    name: String,
    description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NixTemplates {
    location: String,
    templates: Vec<NixTemplateInfo>,
}

use theme::theme;

use im::Vector;
use snafu::prelude::*;

use crate::search::{search, search_by_name_metric};

#[derive(Debug, Snafu)]
pub enum ProgramError {
    #[snafu(display(
        "Error in external process.
            While attempting {goal}, by using {command}, we 
            encountered the following erorr: 

                {source}
        "
    ))]
    ProcessError {
        goal: String,
        command: String,
        source: std::io::Error,
    },

    #[snafu(display(
        r"
Error in external process.
While attempting {goal}, by using {command}, we 
encountered a failed exit code: {exit_code}.
hint: as of right now exit code must be zero to succeed

Error Log: 
{stderr}
        "
    ))]
    BadExitCode {
        goal: String,
        command: String,
        stderr: String,
        exit_code: i32,
    },

    #[snafu(display(
        "While attempting {goal}, during deserialization, the following
        error occurred: 

            {source}
        "
    ))]
    DeserializeError {
        goal: String,
        source: serde_json::Error,
    },
}

pub fn nix() -> std::process::Command {
    std::process::Command::new("nix")
}

pub fn nix_flake_show(source: &str) -> Result<NixFlakeInfo, ProgramError> {
    let mut cmd = std::process::Command::new("nix");
    cmd.args(["flake", "show"]);

    cmd.arg(source);

    cmd.arg("--json");
    // cmd.arg("--refresh");

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let exit = cmd
        .spawn()
        .with_context(|_| ProcessSnafu {
            goal: format!("to compute the templates in {source}"),
            command: "nix flake show",
        })?
        .wait_with_output()
        .with_context(|_| ProcessSnafu {
            goal: format!(
                "to compute the template in {source} -- failed to wait for output from command"
            ),
            command: "nix flake show",
        })?;
    if !exit.status.success() {
        return Err(ProgramError::BadExitCode {
            goal: format!("to compute the template in source {source}"),
            command: "nix flake show".to_owned(),
            stderr: exit.stderr.as_bstr().to_string(),
            exit_code: exit.status.code().unwrap(),
        });
    }
    let output: NixFlakeInfo = serde_json::from_str(exit.stdout.as_bstr().to_str_lossy().as_ref())
        .with_context(|_| DeserializeSnafu {
            goal: format!("to get the info of {source}"),
        })?;

    Ok(output)
}

pub fn nix_templates<'rsrc>(source: &'rsrc str) -> Result<NixTemplates, ProgramError> {
    let nfi = nix_flake_show(source)?;

    let template_infos = nfi
        .templates
        .iter()
        .filter(|(_key, value)| value.type_field == "template")
        .map(|(key, value)| NixTemplateInfo {
            name: key.to_owned(),
            description: value.description.to_owned(),
        })
        .collect::<Vec<_>>();
    Ok(NixTemplates {
        location: source.to_owned(),
        templates: template_infos,
    })
}

pub fn nix_flake_init<'rsrc, P: AsRef<std::path::Path>>(
    source: &str,
    template_name: &str,
    path: P,
) -> Result<(), ProgramError> {
    let mut nix = nix();
    nix.current_dir(path);

    // only two hard problems in CS : naming things, and cache invalidation.
    // so just don't cache anything for now, and we might fix it later.
    // I've pulled too much hair myself trying to find the --refresh command
    // since nix is so poorly documented.
    nix.args(["flake", "init", "--refresh", "-t"]);
    nix.arg(format!("{}#{}", source, template_name));

    nix.stdout(Stdio::piped());
    nix.stderr(Stdio::piped());

    let exit = nix
        .spawn()
        .with_context(|_| ProcessSnafu {
            goal: format!("to instantiate template {source}#{template_name}"),
            command: "nix flake init",
        })?
        .wait_with_output()
        .with_context(|_| ProcessSnafu {
            goal: format!(
                "to instantiate template from {source}#{template_name} -- failed to wait for output from command"
            ),
            command: "nix flake init",
        })?;

    if !exit.status.success() {
        return Err(ProgramError::BadExitCode {
            goal: format!("to instantiate template from {source}#{template_name}"),
            command: "nix flake show".to_owned(),
            stderr: exit.stderr.as_bstr().to_string(),
            exit_code: exit.status.code().unwrap(),
        });
    }
    Ok(())
}

/// I posit users don't really care to
/// know that github:username/template repo
/// is really different from username/template.
/// I can always add the github logo in later,
/// if need be, but for now, not having it is just
/// plain ugly.
fn extract_source_name(template: &str) -> &str {
    if template.starts_with("https://") {
        return template.trim_start_matches("https://");
    }
    match template.split_once(":") {
        Some((_prefix, suffix)) => suffix,
        None => template,
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
struct SelectedFlakeOption {
    which_flake_source: Option<usize>,
    which_template: Option<usize>,
}

impl SelectedFlakeOption {
    fn select_flake_source(self, which_flake_source: usize) -> Self {
        if self.is_flake_source(which_flake_source) {
            Self {
                which_flake_source: None,
                which_template: None,
            }
        } else {
            Self {
                which_flake_source: Some(which_flake_source),
                which_template: None,
            }
        }
    }

    fn is_flake_source(self, which_flake_source: usize) -> bool {
        self.which_flake_source
            .filter(|repo| *repo == which_flake_source)
            .is_some()
    }

    pub fn is_template(self, which_template: usize) -> bool {
        self.which_template
            .filter(|&template| template == which_template)
            .is_some()
    }

    pub fn select_template(self, which_template: usize) -> Self {
        let mut s = self;
        s.which_template = Some(which_template);
        s
    }
}

#[test]
fn test_selected_flake() {
    let mut selected_flake = SelectedFlakeOption::default();
    selected_flake = selected_flake.select_flake_source(0);
    dbg!(selected_flake);
    assert!(selected_flake.is_flake_source(0));
    assert!(!selected_flake.is_flake_source(1));
}

fn template_list(
    selection_state: RwSignal<SelectedFlakeOption>,
    flake_idx: usize,
    templates: RwSignal<Vec<NixTemplates>>,
) -> impl View {
    let this_is_selected = create_memo(move |_| selection_state.get().is_flake_source(flake_idx));

    let sources = create_rw_signal(vec![]);
    create_effect(move |_| {
        sources.set(if dbg!(this_is_selected.get()) {
            dbg!(templates.get()[flake_idx].templates.clone())
        } else {
            vec![]
        })
    });

    views::dyn_container(
        move || sources.get(),
        move |sources| {
            sources
                .into_iter()
                .enumerate()
                .map(move |(idx, template)| {
                    label(move || template.name.clone())
                        .style(move |s| {
                            s.padding(15.0)
                                .padding_top(4.0)
                                .padding_bottom(4.0)
                                .width_full()
                                .items_start()
                                .font_weight(StyleValue::Val(Weight::MEDIUM))
                                .border_radius(5.0)
                                .apply_if(selection_state.get().is_template(idx), |s| {
                                    s.background(theme().accent)
                                })
                                .hover(|s| {
                                    s.apply_if(!selection_state.get().is_template(idx), |s| {
                                        s.background(theme().bg_plus)
                                    })
                                })
                        })
                        .on_click_stop(move |_| {
                            selection_state.update(|state| *state = state.select_template(idx))
                        })
                        .pipe(container)
                        .style(|s| s.padding_left(10.0).margin_top(4.0))
                })
                .pipe(v_stack_from_iter)
                .style(|s| s.width_full())
                .pipe(Box::new)
        },
    )
    .style(|s| s.width_full())
}

pub enum Icon {
    Svg(Cow<'static, str>),
    None,
}
pub fn list_selection(
    selected: impl Fn() -> bool + 'static,
    lab: impl Fn() -> String + 'static,
    icon: Icon,
    label_style: impl Fn(Style) -> Style + 'static,
) -> impl View {
    let icon = match icon {
        Icon::Svg(svg) => views::svg(move || svg.clone().into_owned())
            .style(|s| s.height(9.0).aspect_ratio(Some(1.5)))
            .pipe(container_box),
        Icon::None => container_box(views::empty()),
    };
    h_stack((icon, label(lab).style(label_style))).style(move |s| {
        s.padding(8.0)
            .gap(10.0, 0.0)
            .padding_top(8.0)
            .padding_bottom(8.0)
            .min_width(0)
            .items_center()
            .font_weight(StyleValue::Val(Weight::MEDIUM))
            .border_radius(5.0)
            .apply_if(!selected(), |s| s.background(theme().bg_plus))
            .apply_if(selected(), |s| s.background(theme().accent))
    })
}
fn flake_list(
    sidebar_width: f64,
    flake_sources: RwSignal<Vector<String>>,
    selection_state: RwSignal<SelectedFlakeOption>,
    templates: RwSignal<Vec<NixTemplates>>,
) -> impl View {
    let view_iter = flake_sources
        .get()
        .into_iter()
        .enumerate()
        .map(move |(idx, item)| {
            v_stack((
                h_stack((
                    views::svg(|| include_str!("../assets/github-mark-white.svg").to_owned())
                        .style(|s| s.height(12.0).aspect_ratio(Some(1.0))),
                    label(move || item.clone()),
                ))
                .style(move |s| {
                    s.padding(8.0)
                        .gap(10.0, 0.0)
                        .padding_top(8.0)
                        .padding_bottom(8.0)
                        .width(sidebar_width)
                        .items_center()
                        .font_weight(StyleValue::Val(Weight::MEDIUM))
                        .border_radius(5.0)
                        .apply_if(!selection_state.get().is_flake_source(idx), |s| {
                            s.background(Color::rgba8(255, 255, 255, 5))
                        })
                        .apply_if(selection_state.get().is_flake_source(idx), |s| {
                            s.background(Color::rgba8(0, 122 - 15, 204 - 15, 255))
                        })
                })
                .on_click_stop(move |_| {
                    selection_state.update(|state| *state = state.select_flake_source(idx));
                }),
                template_list(selection_state, idx, templates),
            ))
        });
    views::stack_from_iter(view_iter).style(|s| {
        s.flex_col()
            .gap(0.0, 3.0)
            .padding_bottom(10.0)
            .padding_top(4.0)
    })
}

fn custom_button<S: std::fmt::Display + 'static>(lab: impl Fn() -> S + 'static) -> impl View {
    label(lab).style(move |s| {
        s.padding(15.0)
            .padding_top(4.0)
            .padding_bottom(4.0)
            .width_full()
            .items_start()
            .font_weight(StyleValue::Val(Weight::MEDIUM))
            .border_radius(5.0)
    })
}

fn radio_button<T: PartialEq + Copy + 'static>(
    width: f64,
    checked: RwSignal<T>,
    checked_when: T,
) -> impl View {
    views::container(views::empty().style(move |s| {
        s.width(Pct(50.0))
            .height(Pct(50.0))
            .apply_if(checked.get() == checked_when, |s| {
                s.background(Color::WHITE)
            })
            .apply_if(checked.get() != checked_when, |s| {
                s.background(Color::TRANSPARENT)
            })
            .border_radius(Px(9999.0))
    }))
    .style(move |s| {
        s.width(Px(width))
            .height(Px(width))
            .flex_row()
            .items_center()
            .justify_center()
            .border(width / 10.0)
            .border_color(Color::WHITE)
            .border_radius(Px(9999.0))
    })
    .on_click_stop(move |_| checked.set(checked_when))
}

/// stack:
///   style: flex flex-row px-4 py-4 rounded-full bg-blue-500 mx-2 my-4 gap-x-2
///   - label: "hello"

fn app_view(templates: Vec<NixTemplates>) -> impl View {
    const SIDEBAR_WIDTH: f64 = 220.0;
    const TOPBAR_HEIGHT: f64 = 38.0;

    let template_sources = templates
        .iter()
        .map(|template| template.location.as_ref())
        .map(extract_source_name)
        .collect::<Vec<_>>();

    let owned_template_list = template_sources
        .into_iter()
        .map(|it| it.to_string())
        .collect::<Vector<String>>();

    let templates = create_rw_signal(templates);
    let flake_sources = create_rw_signal(owned_template_list);

    let selection_state = create_rw_signal(SelectedFlakeOption::default());

    let _description = create_memo(move |_| {
        let selection = selection_state.get();
        if let Some((which_src, which_template)) =
            selection.which_flake_source.zip(selection.which_template)
        {
            templates.get()[which_src].templates[which_template]
                .description
                .clone()
        } else {
            "Select an option on the left get a description.".to_owned()
        }
    });

    let search_text = create_rw_signal("".to_owned());
    let _searcher = text_input(search_text)
        .placeholder("Search")
        .style(|s| {
            s.width_full()
                .background(theme().bg_plus)
                .color(theme().fg)
                .cursor_color(theme().fg)
                .padding(5.0)
                .padding_horiz(10.0)
                .border_radius(5.0)
        })
        .pipe(container)
        .style(|s| {
            s.padding_horiz(10.0)
                .padding_vert(8.0)
                .width_full()
                .max_width_pct(30.0)
                .max_height_full()
        });

    let top_bar = drag_window_area(views::empty())
        .style(|s| s.width_full().height(TOPBAR_HEIGHT).justify_between());

    let side_bar = v_stack((
        label(|| "Flake Templates").pipe(container).style(|s| {
            s.border_top(1.0)
                .border_bottom(1.0)
                .padding_left(15.0)
                .font_weight(Weight::SEMIBOLD)
                .padding_vert(2.0)
                .border_color(theme().bd)
        }),
        flake_list(SIDEBAR_WIDTH, flake_sources, selection_state, templates)
            .pipe(scroll)
            // .hide_bar(|| true)
            .pipe(container)
            .style(|s| {
                s.flex_grow(1.0)
                    .min_height(0)
                    .flex_basis(0)
                    .padding_horiz(15)
            }),
    ))
    .style(|s| s.flex_col().flex_grow(1.0).min_height(0));

    let side_bar_2 = v_stack((label(|| "Recent Projects").pipe(container).style(|s| {
        s.border_top(1.0)
            .border_bottom(1.0)
            .padding_left(15.0)
            .font_weight(Weight::SEMIBOLD)
            .padding_vert(2.0)
            .border_color(theme().bd)
    }),))
    .style(|s| s.flex_col().flex_grow(1.0).min_height(0));

    let create_project_button = label(|| "Create Project")
        .pipe(container)
        .style(move |s| {
            s.width_full()
                .min_height(0)
                .flex_row()
                .justify_center()
                .padding_horiz(10.0)
                .padding_vert(8.0)
                .font_weight(Weight::SEMIBOLD)
                .background(theme().bg_plus)
                .apply_if(selection_state.get().which_template.is_some(), |s| {
                    s.background(theme().accent_dim)
                        .hover(|s| s.background(theme().accent))
                })
                .border_radius(5.0)
        })
        .pipe(|view| {
            tooltip(view, || {
                label(|| "First, select a template above").style(|s| {
                    s.color(theme().fg)
                        .background(theme().bg_plus)
                        .padding(4.0)
                        .padding_horiz(10.0)
                        .border_radius(2.0)
                })
            })
        })
        .style(|s| s.width_full())
        .pipe(container)
        .style(|s| s.width_full().padding_horiz(10.0).padding_vert(2.0));

    let bars = v_stack((side_bar, create_project_button, side_bar_2)).style(|s| {
        s.height_full()
            .flex_col()
            .border_right(1.0)
            .border_color(theme().bd)
    });
    let side_bar = bars;

    // label(move || String::from("Description")).style(|s| {
    //                 s.padding(10.0)
    //                     .font_size(18.0)
    //                     .font_weight(Weight::SEMIBOLD)
    //             }),
    //             label(move || description.get()).style(move |s| s.max_width_full())

    let project_name = create_rw_signal(Default::default());

    let _main_window = create_project_menu(project_name);

    let main_window = construct_nixpkgs_search();
    let main_window = main_window
        .pipe(scroll)
        .style(|s| {
            s.flex_col()
                .border_horiz(1.0)
                .gap(0.0, 20.0)
                .padding_horiz(15.0)
                .border_color(theme().bd)
                .min_height_full()
                .padding_top(15.0)
                .items_center()
                .width_full()
                .padding_bottom(10.0)
                .max_width(580)
        })
        .pipe(container)
        .style(|s| {
            s.flex_col()
                .flex_basis(0)
                .min_width(0)
                .flex_grow(1.0)
                .border_top(1.0)
                .items_center()
                .border_color(theme().bd)
                .width_full()
        });

    let content = h_stack((side_bar, main_window)).style(|s| {
        s.position(Position::Absolute)
            .inset_top(TOPBAR_HEIGHT)
            .inset_bottom(0.0)
            .width_full()
    });

    let view = v_stack((top_bar, content))
        .style(|s| s.width_full().height_full())
        .window_title(|| "NixOS Brewer".to_owned());

    let id = view.id();
    view.keyboard_navigatable()
        .on_event_stop(EventListener::KeyUp, move |e| {
            if let Event::KeyUp(e) = e {
                let key = e.key.physical_key;
                if key == KeyCode::KeyI && e.modifiers.super_key() {
                    id.inspect();
                }
                if key == KeyCode::KeyW && e.modifiers.super_key() {
                    quit_app();
                }
                // if e.key.logical_key == Key::Named() {
                // id.inspect();
                // }
            }
        })
}

#[derive(Clone)]
pub enum NixEnvPackageKind {
    // idea is that some day, we
    // may support adding python or
    // R or a whole host of other packages,
    // and those are not simple, because they have
    // parenting.
    Simple,
}

#[derive(Clone)]
pub struct NixEnvPackage {
    human_name: String,
    nix_name: String,
    version: String,
    kind: NixEnvPackageKind,
}

impl NixEnvPackage {
    fn simple_from_strs(human_name: &str, nix_name: &str, version: &str) -> Self {
        Self {
            human_name: human_name.to_owned(),
            nix_name: nix_name.to_owned(),
            version: version.to_owned(),
            kind: NixEnvPackageKind::Simple,
        }
    }
}

pub fn nixpkgs_search_window() -> impl View {
    let main_window = construct_nixpkgs_search().pipe(container).style(|s| {
        s.width_full()
            .height_full()
            .flex()
            .flex_row()
            .justify_center()
            .min_height(0)
    });

    let nix_env_packages = create_rw_signal(vec![
        NixEnvPackage::simple_from_strs("cargo", "cargo", "1.74"),
        NixEnvPackage::simple_from_strs("cargo", "cargo", "1.74"),
        NixEnvPackage::simple_from_strs("cargo", "cargo", "1.74"),
        NixEnvPackage::simple_from_strs("cargo", "cargo", "1.74"),
        NixEnvPackage::simple_from_strs("cargo", "cargo", "1.74"),
        NixEnvPackage::simple_from_strs("cargo", "cargo", "1.74"),
        NixEnvPackage::simple_from_strs("cargo", "cargo", "1.74"),
    ]);

    const TOPBAR_HEIGHT: f64 = 38.0;
    let top_bar = drag_window_area(views::empty())
        .style(|s| s.width_full().height(TOPBAR_HEIGHT).justify_between());
    let environment = static_label("Environment".to_owned()).style(|s| s.font_weight(Weight::BOLD));

    let num_packages = label(move || nix_env_packages.get().len())
        .style(|s| s.font_weight(Weight::BOLD).font_size(10.0))
        .pipe(container)
        .style(|s| {
            s.border_radius(9999.9)
                .background(theme().bg_plus2)
                .flex()
                .flex_row()
                .items_center()
                .justify_center()
        });

    let expansion = static_label("^")
        .style(|s| s.font_weight(Weight::BOLD).font_size(10.0))
        .pipe(container)
        .style(|s| {
            s.flex_row()
                .padding_horiz(10.0)
                .border_radius(5)
                .border(1.0)
                .border_color(theme().bd)
                .flex()
                .items_center()
                .justify_center()
                .background(theme().bg_plus2)
                .margin_left(100.0)
        });

    let dep_stack = dyn_stack(
        move || nix_env_packages.get().into_iter().enumerate(),
        move |(i, _)| *i,
        move |(_, v)| {
            v_stack((
                label(move || v.human_name.clone())
                    .style(|s| s.font_weight(Weight::BOLD).font_size(16.0)),
                label(move || v.version.clone()),
            ))
            .style(|s| {
                s.border_radius(7)
                    .padding_vert(4.0)
                    .border(0.5)
                    .border_color(theme().bd)
                    .padding_left(10.0)
                    .background(theme().bg_plus)
            })
        },
    )
    .style(|s| {
        s.background(theme().bg)
            .padding(4.0)
            .flex_col()
            .width_full()
            .gap(0.0, 5.0)
    })
    .pipe(scroll)
    .style(|s| s.max_height(120.0));
    let environment_tab = v_stack((
        h_stack((environment, num_packages, expansion))
            .style(|s| {
                s.width_full()
                    .background(theme().bg_plus)
                    .gap(10.0, 0)
                    .font_size(14.0)
                    .padding_vert(6.0 * 2.0)
                    .padding_horiz(10.0 * 2.0)
            })
            .style(|s| s.border_bottom(1.0).border_color(theme().bd)),
        dep_stack,
    ))
    .pipe(views::clip)
    .style(|s| {
        s.absolute()
            .inset_bottom(10)
            .inset_right(100)
            .border(1.0)
            .border_color(theme().bd)
            .border_radius(7.0)
    });

    let view = v_stack((top_bar, main_window, environment_tab))
        .style(|s| s.width_full().height_full())
        .window_title(|| "NixOS Brewer".to_owned());

    let id = view.id();
    view.keyboard_navigatable()
        .on_event_stop(EventListener::KeyUp, move |e| {
            if let Event::KeyUp(e) = e {
                let key = e.key.physical_key;
                if key == KeyCode::KeyI && e.modifiers.super_key() {
                    id.inspect();
                }
                if key == KeyCode::KeyW && e.modifiers.super_key() {
                    quit_app();
                }
            }
        })
}
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectCreationLocation {
    ExistingDirectory,
    NewDirectory,
}

mod style {
    use floem::{cosmic_text::Weight, peniko::Color, style::Style};

    pub fn h1(style: Style) -> Style {
        style
            .padding(10.0)
            .font_size(24.0)
            .min_width(0)
            .font_weight(Weight::BOLD)
    }

    pub fn h3(style: Style) -> Style {
        style
            .padding(10.0)
            .font_size(16.0)
            .font_weight(Weight::BOLD)
            .min_width(0)
    }
    pub fn text_hint(s: Style) -> Style {
        s.font_weight(Weight::SEMIBOLD)
            .color(Color::rgb8(225, 225, 255))
    }
    use crate::theme::theme;
    pub fn text_input(s: Style) -> Style {
        s.border(1.0)
            .border_color(theme().bd)
            .background(theme().bg_plus)
            .padding_horiz(5.0)
            .cursor_color(Color::WHITE)
            .border_radius(4.0)
            .padding_vert(4.0)
    }
    pub fn button(s: Style) -> Style {
        s.border_radius(10.0)
            .flex()
            .border(1.0)
            .gap(0.0, 5.0)
            .border_color(theme().bd)
            .min_width(0)
            .padding(8.0)
            .background(theme().bg_plus)
    }
}

pub fn package_support(support: PackageSupport) -> impl View {
    dyn_container(
        move || support,
        |sup| match sup {
            PackageSupport::Supported => Box::new(tooltip(
                static_label("✓").style(|s| s.color(tailwind::color("green-500"))),
                || static_label("Supported on this system"),
            )),
            PackageSupport::MostLikelyNot => {
                Box::new(static_label("?").style(|s| s.color(tailwind::color("yellow-500"))))
            }
            PackageSupport::NoneListed => Box::new(static_label("✕").style(|s| {
                s.color(tailwind::color("red-500"))
                    .font_weight(Weight::BOLD)
            })),
        },
    )
}

fn search_result_card(selected: RwSignal<Selectable<PackageSearchValue>>) -> impl View {
    static PYTHON_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"python[0-9_]+Packages\.").unwrap());
    dyn_stack(
        move || selected.get().into_iter(),
        |key| key.2.clone(),
        move |(_sel, idx, each)| {
            let version = each.package_pversion.clone();
            let support = each.available_on_this_system();
            let card_name = each.package_attr_name;

            let card_name_view = |card_name| {
                static_label(card_name).style(|s| {
                    style::h3(s)
                        .min_width(0)
                        .flex_grow(1.0)
                        .max_width_full()
                        .font_size(14.0)
                })
            };

            let text_node = if PYTHON_REGEX.is_match(&card_name) {
                h_stack((
                    views::svg(|| include_str!("../assets/python-logo-only.svg").to_owned())
                        .style(|s| s.height(14.0).width(14.0)),
                    card_name_view(card_name.split_once(".").unwrap().1.to_owned()),
                ))
                .style(|s| s.align_items(AlignItems::Center))
                .pipe(container_box)
            } else if card_name.starts_with("rPackages.") {
                h_stack((
                    views::svg(|| include_str!("../assets/r-logo.svg").to_owned())
                        .style(|s| s.height(18.0).width(18.0)),
                    card_name_view(card_name.split_once(".").unwrap().1.to_owned()),
                ))
                .style(|s| s.align_items(AlignItems::Center))
                .pipe(container_box)
            } else if card_name.starts_with("haskellPackages.") {
                h_stack((
                    views::svg(|| include_str!("../assets/haskell-logo.svg").to_owned())
                        .style(|s| s.height(18.0).width(18.0)),
                    card_name_view(card_name.split_once(".").unwrap().1.to_owned()),
                ))
                .style(|s| s.align_items(AlignItems::Center))
                .pipe(container_box)
            } else if card_name.starts_with("linuxKernel.") {
                h_stack((
                    views::svg(|| include_str!("../assets/tux.svg").to_owned())
                        .style(|s| s.height(18.0).width(18.0)),
                    card_name_view(card_name.split_once(".").unwrap().1.to_owned()),
                ))
                .style(|s| s.align_items(AlignItems::Center))
                .pipe(container_box)
            } else if card_name.starts_with("vscode-extensions.") {
                h_stack((
                    views::svg(|| include_str!("../assets/vscode-alt.svg").to_owned())
                        .style(|s| s.height(14.0).width(14.0)),
                    card_name_view(card_name.split_once(".").unwrap().1.to_owned()),
                ))
                .style(|s| s.align_items(AlignItems::Center))
                .pipe(container_box)
            } else {
                card_name_view(card_name).pipe(container_box)
            };

            let description = each.package_description;

            let version_line = (
                views::svg(|| include_str!("../assets/home.svg").to_owned())
                    .style(|s| s.width(10.).height(10.))
                    .pipe(container)
                    .style(|s| {
                        s.border_radius(999)
                            .cursor(CursorStyle::Pointer)
                            .background(theme().bg_plus)
                            .border(0.5)
                            .border_color(theme().bd)
                            .padding(5.0)
                    })
                    .on_click_stop(move |_| {
                        each.package_homepage.get(0).map(|t| open::that(t));
                    }),
                static_label(version).style(style::text_hint),
                package_support(support),
            )
                .pipe(h_stack)
                .style(|s| s.flex_row().gap(10.0, 0.0).align_items(AlignItems::Center));

            let top_line = (text_node, version_line)
                .pipe(h_stack)
                .style(|s| s.flex_row().justify_between().flex_grow(1.0).items_center());

            let description = static_label(description);

            // let versions_label = static_label("Versions").style(|s| {
            //     s.font_weight(Weight::BOLD)
            //         .padding_top(10.0)
            //         .padding_bottom(6.0)
            // });

            // let versions_button = static_label("More Versions...")
            //     .pipe(container)
            //     .style(|s| {
            //         s.font_style(floem::cosmic_text::Style::Italic)
            //             .cursor(CursorStyle::Pointer)
            //     })
            //     .on_click_stop(move |_| selected.update(|s| s.els[idx].compute_versions()));

            // let versions = v_stack_from_iter(
            //     each.versions
            //         .into_iter()
            //         .map(|i| i.into_iter())
            //         .flatten()
            //         .map(|ver| {
            //             h_stack((static_label(ver.version), static_label(ver.date))).style(|s| {
            //                 s.flex_row()
            //                     .justify_content(AlignContent::SpaceAround)
            //                     .padding_vert(7.0)
            //             })
            //         }),
            // );

            // let versions = v_stack((
            //     h_stack((static_label("Version"), static_label("Date"))).style(|s| {
            //         s.background(theme().bg)
            //             .font_weight(Weight::BOLD)
            //             .padding_vert(10.0)
            //             .justify_content(AlignContent::SpaceAround)
            //     }),
            //     versions,
            // ))
            // .style(|s| {
            //     s.background(theme().bg)
            //         .border(1.0)
            //         .border_color(theme().bd)
            // });

            // let versions_section = v_stack((versions_label, versions, versions_button));
            let number_of_binaries = each.package_programs.len();
            let programs_provided = h_stack_from_iter(each.package_programs.into_iter().map(|i| {
                static_label(i).style(|s| {
                    s.padding_horiz(10)
                        .padding_vert(4.0)
                        .background(Color::BLACK.with_alpha_factor(0.1))
                        .border(1.0)
                        .border_radius(Pct(25.0))
                        .border_color(theme().bd)
                })
            }))
            .style(|s| s.flex_wrap(FlexWrap::Wrap).width_full());

            let program_section = v_stack((
                static_label(if number_of_binaries == 0 {
                    "No Binaries Provided"
                } else {
                    "Binaries"
                })
                .style(|s| {
                    s.font_weight(Weight::BOLD)
                        .padding_bottom(5.0)
                        .padding_top(10)
                }),
                programs_provided,
            ));

            let add_package = dyn_container(
                move || selected.get(),
                move |s| {
                    if s.is_selected(idx) {
                        static_label(format!("Install {}", each.package_pversion))
                            .style(|s| {
                                s.padding_vert(4.0)
                                    .padding_horiz(10.0)
                                    .outline(2.0)
                                    .outline_color(tailwind::color("slate-500"))
                                    .background(Color::rgb8(45, 45, 45))
                                    .border_radius(Pct(100.0))
                                    .z_index(40)
                                    .font_size(9.0)
                                    .font_weight(Weight::SEMIBOLD)
                            })
                            .pipe(Box::new)
                    } else {
                        views::empty().pipe(Box::new)
                    }
                },
            )
            .style(|s| s.absolute().inset_right(20.0).inset_top(-10.0));

            (
                add_package,
                top_line,
                description,
                // versions_section,
                program_section,
            )
                .pipe(v_stack)
                .style(move |s| {
                    s.border_radius(10.0)
                        .flex()
                        .border(1.0)
                        .gap(0.0, 15.0)
                        .border_color(theme().bd)
                        .apply_if(selected.get().is_selected(idx), |s| {
                            s.outline(2.0).outline_color(tailwind::color("slate-500"))
                        })
                        .min_width(0)
                        .padding(15.0)
                        .padding_top(4.0)
                        .background(theme().bg_plus)
                })
                .pipe(|b| Box::new(b) as Box<dyn View>)
                .on_click_stop(move |_| selected.update(|s| s.select(idx)))
        },
    )
    .style(|s| s.min_width(0))
}

#[derive(Clone, Debug)]
pub struct Selectable<T> {
    els: Vec<T>,
    selected: Option<usize>,
}

impl<T> Selectable<T> {
    pub fn new_vec(els: Vec<T>) -> Self {
        Self {
            els,
            selected: None,
        }
    }

    pub fn new() -> Self {
        Self {
            els: Vec::new(),
            selected: None,
        }
    }

    pub fn into_iter(self) -> impl Iterator<Item = (bool, usize, T)> {
        self.els
            .into_iter()
            .enumerate()
            .map(move |(idx, val)| (self.selected.filter(|s| *s == idx).is_some(), idx, val))
    }

    pub fn select(&mut self, place: usize) {
        self.selected = Some(place);
    }

    fn is_selected(&self, idx: usize) -> bool {
        self.selected.filter(|s| *s == idx).is_some()
    }
}

impl<A> FromIterator<A> for Selectable<A> {
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        Self::new_vec(iter.into_iter().collect())
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Copy)]
pub enum SearchMode {
    Name,
    Program,
}

#[derive(Clone, Debug, Hash)]
pub struct SearchProperties {
    pub mode: SearchMode,
    pub channel: usize,
}

pub struct Channels {
    opts: Vec<String>,
}

impl Channels {
    pub fn new() -> Self {
        Self {
            opts: ["23.05", "23.11", "unstable"]
                .map(ToOwned::to_owned)
                .to_vec(),
        }
    }
}

pub static THREAD_SEARCHER: Lazy<
    ActorThread<(String, SearchProperties), Result<Selectable<PackageSearchValue>, String>>,
> = Lazy::new(|| {
    ActorThread::new(
        |(search_text, search_props): (String, SearchProperties)| match search::search(
            search_text.as_str(),
            search_props.mode,
            Channels::new().opts[search_props.channel].clone(),
        ) {
            Ok(mut val) => {
                val.sort_by_cached_key(|v| {
                    search_by_name_metric(&search_text.to_owned(), &v.package_attr_name)
                });

                val.reverse();
                let val = val
                    .into_iter()
                    .map(|mut val| {
                        val.package_programs.sort_unstable();
                        val
                    })
                    .collect::<Vec<_>>();
                return Ok(Selectable::new_vec(val));
            }
            Err(e) => {
                return Err(e.to_string());
            }
        },
    )
});

#[derive(Clone, Debug)]
pub enum SearchingState {
    Idle,
    Fetching,
    ResultsAvailable,
    NoResultsAvailable,
    AnErrorOccurred(String),
}

#[derive(Clone, Debug)]
pub struct LoadingWidgetState {
    back_tail: usize,
    front_tail: usize,
    cycles: usize,
}

impl LoadingWidgetState {
    pub fn new() -> Self {
        Self {
            back_tail: 0,
            front_tail: 3,
            cycles: 0,
        }
    }
    pub fn next_state(&mut self, n_seg: usize) {
        self.back_tail += 1;
        self.front_tail += 1;
        self.back_tail %= n_seg;
        self.front_tail %= n_seg;
        self.cycles += 1;
    }
    pub fn style_node(&self, node_num: usize) -> impl Fn(Style) -> Style {
        let high = self.front_tail;
        let low = self.back_tail;

        let count = self.cycles;
        move |s| {
            if high < low {
                if (node_num >= low || node_num < high) && count >= 10 {
                    s
                } else {
                    s.color(Color::rgb8(155, 155, 155).with_alpha_factor(count as f32 / 10.0))
                }
            } else {
                if node_num >= low && node_num < high && count >= 10 {
                    s
                } else {
                    s.color(Color::rgb8(155, 155, 155).with_alpha_factor(count as f32 / 10.0))
                }
            }
        }
    }
}
fn loading_widget() -> impl View {
    const SNOWFLAKE_SIZE: f64 = 12.0;

    let loading_state = create_rw_signal(LoadingWidgetState::new());
    let animation_trigger = create_trigger();

    create_effect(move |_| {
        animation_trigger.track();
        exec_after(std::time::Duration::from_millis(100), move |_| {
            animation_trigger.notify();
            loading_state.update(|l| l.next_state(8));
        });
    });

    h_stack((
        nix_snowflake_svg()
            .style(|s| s.width(SNOWFLAKE_SIZE).height(SNOWFLAKE_SIZE))
            .style(move |s| loading_state.get().style_node(0)(s)),
        nix_snowflake_svg()
            .style(|s| s.width(SNOWFLAKE_SIZE).height(SNOWFLAKE_SIZE))
            .style(move |s| loading_state.get().style_node(1)(s)),
        nix_snowflake_svg()
            .style(|s| s.width(SNOWFLAKE_SIZE).height(SNOWFLAKE_SIZE))
            .style(move |s| loading_state.get().style_node(2)(s)),
        nix_snowflake_svg()
            .style(|s| s.width(SNOWFLAKE_SIZE).height(SNOWFLAKE_SIZE))
            .style(move |s| loading_state.get().style_node(3)(s)),
        nix_snowflake_svg()
            .style(|s| s.width(SNOWFLAKE_SIZE).height(SNOWFLAKE_SIZE))
            .style(move |s| loading_state.get().style_node(4)(s)),
        nix_snowflake_svg()
            .style(|s| s.width(SNOWFLAKE_SIZE).height(SNOWFLAKE_SIZE))
            .style(move |s| loading_state.get().style_node(5)(s)),
        nix_snowflake_svg()
            .style(|s| s.width(SNOWFLAKE_SIZE).height(SNOWFLAKE_SIZE))
            .style(move |s| loading_state.get().style_node(6)(s)),
        nix_snowflake_svg()
            .style(|s| s.width(SNOWFLAKE_SIZE).height(SNOWFLAKE_SIZE))
            .style(move |s| loading_state.get().style_node(7)(s)),
    ))
    .style(|s| s.gap(3.0, 0.0))
}

fn construct_nixpkgs_search() -> impl View {
    let search_text = create_rw_signal(String::new());
    let active_packages = create_rw_signal(Selectable::new());
    let searching_state = create_rw_signal(SearchingState::Idle);

    let search_props = create_rw_signal(SearchProperties {
        mode: SearchMode::Name,
        channel: 1,
    });

    let active_package_receiver = THREAD_SEARCHER.create_channel_from_receiver();
    create_effect(move |_| {
        if let Some(pkg) = active_package_receiver.get() {
            match pkg {
                Ok(pkg) => {
                    let pkg_is_empty = pkg.els.is_empty();
                    active_packages.set(pkg);
                    searching_state.set(if pkg_is_empty {
                        SearchingState::NoResultsAvailable
                    } else {
                        SearchingState::ResultsAvailable
                    });
                }
                Err(err) => searching_state.set(SearchingState::AnErrorOccurred(err)),
            }
        }
    });
    let search_init = create_trigger();

    create_effect(move |t| {
        if search_text.get().is_empty() && t.is_some() {
            searching_state.set(SearchingState::Idle);
            active_packages.set(Selectable::new());
            return;
        }
    });
    create_effect(move |_| {
        // i think early exit screws with it.
        search_props.track();
        search_init.track();

        let search_text = search_text.get_untracked();

        if search_text.is_empty() {
            return;
        }
        active_packages.set(Selectable::new());
        searching_state.set(SearchingState::Fetching);

        THREAD_SEARCHER
            .send((search_text.to_owned(), search_props.get()))
            .unwrap();
    });

    let title = static_label("Nix Package Manager Search")
        .style(style::h1)
        .pipe(container)
        .style(|s| s.justify_center());

    let title = h_stack((
        nix_snowflake_svg().style(|s| s.width(40.0).height(40.0)),
        title.style(|s| s.flex_grow(1.0)),
    ))
    .style(|s| s.min_width(0).min_height(0).flex_row().items_center());
    let search = text_input(search_text)
        .style(|s| {
            style::text_input(s)
                .width_full()
                .padding_vert(7.0)
                .padding_horiz(15.0)
        })
        .placeholder("Search for packages")
        .on_event_stop(EventListener::KeyDown, move |e| {
            if let Event::KeyDown(ev) = e {
                if ev.key.logical_key == Key::Named(NamedKey::Enter) {
                    search_init.notify();
                }
            }
        });

    let style_func = |s: Style| {
        s.font_weight(Weight::BOLD)
            .padding_vert(8.0)
            .padding_horiz(10.0)
            .background(theme().accent)
            .flex()
            .background(theme().bg_plus)
            .flex_shrink(1.0)
            .border_color(theme().bd)
            .border(1.0)
            .items_center()
            .border_radius(4.0)
            .justify_center()
    };
    let choose_mode = views::dyn_container(
        move || search_props.get(),
        move |sp| {
            h_stack((
                static_label("By Name")
                    .pipe(views::container)
                    .style(move |s| {
                        style_func(s).apply_if(sp.mode == SearchMode::Name, |s| {
                            s.background(theme().accent)
                        })
                    })
                    .on_click_stop(move |_| search_props.update(|s| s.mode = SearchMode::Name)),
                static_label("By Program")
                    .pipe(views::container)
                    .style(move |s| {
                        style_func(s).apply_if(sp.mode == SearchMode::Program, |s| {
                            s.background(theme().accent)
                        })
                    })
                    .on_click_stop(move |_e| search_props.update(|s| s.mode = SearchMode::Program)),
                views::empty().style(|s| s.flex_grow(1.0)),
                static_label("23.05")
                    .pipe(views::container)
                    .style(style_func)
                    .style(move |s| s.apply_if(sp.channel == 0, |s| s.background(theme().accent)))
                    .on_click_stop(move |_e| search_props.update(|s| s.channel = 0)),
                static_label("23.11")
                    .pipe(views::container)
                    .style(style_func)
                    .style(move |s| s.apply_if(sp.channel == 1, |s| s.background(theme().accent)))
                    .on_click_stop(move |_e| search_props.update(|s| s.channel = 1)),
                static_label("unstable")
                    .pipe(views::container)
                    .style(style_func)
                    .style(move |s| s.apply_if(sp.channel == 2, |s| s.background(theme().accent)))
                    .on_click_stop(move |_e| search_props.update(|s| s.channel = 2)),
            ))
            .style(|s| s.gap(5.0, 0.0).width_full())
            .pipe(Box::new)
        },
    )
    .style(|s| s.width_full())
    .pipe(container);

    let search_section = (title, search, choose_mode)
        .pipe(v_stack)
        .style(|s| s.gap(0.0, 15.0));

    let results_section = views::dyn_container(
        move || searching_state.get(),
        move |s| match s {
            SearchingState::Idle => {
                let nix_repo_svg =
                    views::svg(|| include_str!("../assets/nix-repro.svg").to_owned())
                        .style(|s| s.width(125).aspect_ratio(1.0).margin_bottom(15.0));
                let label = static_label("Search over 80,000 packages")
                    .style(|s| s.font_weight(Weight::NORMAL).font_size(14.0));
                let label_top = static_label("Reproducible. Declarative. Reliable.")
                    .style(|s| s.font_weight(Weight::BOLD).font_size(18.0));

                (nix_repo_svg, label_top, label)
                    .pipe(v_stack)
                    .style(|s| {
                        s.gap(0, 5.0)
                            .flex_grow(1.0)
                            .items_center()
                            .justify_center()
                            .flex_grow(1.0)
                    })
                    .pipe(Box::new)
            }
            SearchingState::Fetching => (
                static_label("Searching for packages")
                    .style(|s| s.font_weight(Weight::BOLD).font_size(14.0)),
                loading_widget(),
            )
                .pipe(v_stack)
                .style(|s| {
                    s.flex()
                        .flex_col()
                        .gap(0.0, 10.0)
                        .items_center()
                        .justify_center()
                        .width_full()
                        .height_full()
                })
                .pipe(Box::new),
            SearchingState::ResultsAvailable => search_result_card(active_packages)
                .style(|s| s.flex_col().gap(0, 10).min_width(0))
                .pipe(container)
                .style(|s| s.padding(10.0).min_width(0))
                .pipe(scroll)
                .style(|s| s.min_height(0).max_height_full().max_width_full())
                .pipe(Box::new),
            SearchingState::NoResultsAvailable => {
                let nix_repo_svg =
                    views::svg(|| include_str!("../assets/nix-repro.svg").to_owned())
                        .style(|s| s.width(125).aspect_ratio(1.0));
                let label = static_label(
                    "We searched far and wide, but no trace of your query was found :(",
                )
                .style(|s| s.font_weight(Weight::BOLD).font_size(14.0));

                (nix_repo_svg, label)
                    .pipe(v_stack)
                    .style(|s| {
                        s.gap(0, 10.0)
                            .flex_grow(1.0)
                            .items_center()
                            .justify_center()
                            .flex_grow(1.0)
                    })
                    .pipe(Box::new)
            }
            SearchingState::AnErrorOccurred(err) => (
                static_label("Oh no! An Error Occurred")
                    .style(|s| s.font_size(14.0).font_weight(Weight::BOLD)),
                static_label(err).style(|s| s.max_width_full()),
            )
                .pipe(v_stack)
                .style(|s| {
                    s.flex()
                        .flex_col()
                        .gap(0.0, 5.0)
                        .min_width(0)
                        .max_width(0)
                        .max_width_full()
                        .max_height_full()
                        .items_center()
                        .justify_center()
                })
                .pipe(Box::new),
        },
    )
    .style(|s| {
        s.flex()
            .flex_grow(1.0)
            .min_width(0)
            .min_height(0)
            .width(420.0)
    });
    (search_section, results_section).pipe(v_stack).style(|s| {
        s.gap(0.0, 10.0)
            .width(420.0)
            .min_width(0)
            .min_height(0)
            .flex_shrink(0.0)
    })
}

fn nix_snowflake_svg() -> views::Svg {
    views::svg(|| include_str!("../assets/Nix_snowflake.svg").to_owned())
}

fn create_project_menu(project_name: RwSignal<String>) -> impl View {
    let choice = create_rw_signal(ProjectCreationLocation::ExistingDirectory);
    let title = label(move || String::from("Create Project"))
        .style(|s| s.padding(10.0).font_size(24.0).font_weight(Weight::BOLD));

    let existing_or_new_folder_dialog = views::stack((
        radio_button(16.0, choice, ProjectCreationLocation::ExistingDirectory),
        label(|| "Use an existing folder"),
        views::empty(),
        radio_button(16.0, choice, ProjectCreationLocation::NewDirectory),
        label(|| "Create project in a new folder with name: "),
        text_input(project_name)
            .style(|s| {
                s.border(1.0)
                    .border_color(theme().bd)
                    .background(theme().bg_plus)
                    .padding_horiz(5.0)
                    .cursor_color(Color::WHITE)
                    .border_radius(4.0)
                    .padding_vert(4.0)
            })
            .on_event_stop(EventListener::FocusGained, move |_| {
                choice.set(ProjectCreationLocation::NewDirectory)
            }),
    ))
    .style(|s| {
        s.display(Display::Grid)
            .grid_template_columns(vec![
                TrackSizingFunction::AUTO,
                TrackSizingFunction::AUTO,
                TrackSizingFunction::AUTO,
            ])
            .gap(5.0, 5.0)
            .items_center()
            .margin_left(30)
            .margin_top(5.0)
            .justify_center()
    })
    .pipe(container);

    let in_common_directory = label(move || "Project Location").style(|s| {
        s.font_size(18.0)
            .font_weight(Weight::SEMIBOLD)
            .padding_left(20.0)
            .padding_top(10.0)
    });

    let common_hint = views::static_label("Your common folders are listed below");

    let button_stack = views::dyn_stack(
        || {
            vec![
                "junior-fall/sc-482",
                "junior-fall/ma-253",
                "rust-programs",
                "julia-programs",
            ]
        },
        |key| key.to_owned(),
        |item| {
            list_selection(
                || false,
                || item.to_owned(),
                Icon::Svg(Cow::Borrowed(include_str!("../assets/folder-white.svg"))),
                |s| s.padding_horiz(10.0),
            )
            .style(|s| s.border(1.0).border_color(theme().bd))
        },
    )
    .style(|s| {
        s.display(Display::Flex)
            // .grid_template_columns(vec![TrackSizingFunction::Repeat(
            //     floem::taffy::style::GridTrackRepetition::AutoFit,
            //     vec![minmax(
            //         floem::taffy::style::MinTrackSizingFunction::Fixed(
            //             floem::taffy::style::LengthPercentage::Points(100.0),
            //         ),
            //         floem::taffy::style::MaxTrackSizingFunction::Fraction(1.0),
            //     )],
            // )])
            .flex_wrap(FlexWrap::Wrap)
            .gap(10.0, 10.0)
    });

    let choose_my_own = list_selection(
        || false,
        || "Choose a Directory".to_owned(),
        Icon::None,
        |s| s.justify_content(AlignContent::Center).width_full(),
    )
    .style(|s| {
        s.width_full()
            .border(1.0)
            .border_color(theme().bd)
            .font_weight(Weight::BOLD)
    });

    v_stack((
        title,
        existing_or_new_folder_dialog,
        in_common_directory,
        common_hint,
        button_stack,
        views::static_label("I want my project somewhere else"),
        choose_my_own,
    ))
    .style(|s| s.gap(0.0, 10.0))
}
mod tailwindcss {

    use bstr::WordsWithBreaks;
    use crossbeam::channel::{Receiver, Sender};

    use floem::{
        cosmic_text::Weight,
        ext_event::create_signal_from_channel,
        reactive::{ReadSignal, RwSignal},
        style::Style,
    };
    use once_cell::sync::Lazy;

    use crate::tailwind;

    fn tailwind_thread(send: Sender<usize>) {
        let mut count = 0;
        loop {
            send.send(count).unwrap();
            count += 1;
            inline_tweak::watch!();
        }
    }
    pub fn style_effect() -> ReadSignal<Option<usize>> {
        static RECV: Lazy<ReadSignal<Option<usize>>> = Lazy::new(|| {
            let (send, recv) = crossbeam::channel::unbounded();
            std::thread::spawn(|| tailwind_thread(send));
            create_signal_from_channel(recv)
        });
        *RECV
    }

    // this is going to be incredibly stupid
    // implementation. here goes.
    fn read_pixels<F: Fn(Style, f32) -> Style>(section: &str, style: Style, f: F) -> Style {
        if let Some((_, end)) = section.split_once("-") {
            if let Ok(end) = end.parse::<u16>() {
                return f(style.clone(), end as f32);
            }
        }
        return style;
    }
    pub fn parse_tailwind(input: String, style: floem::style::Style) -> floem::style::Style {
        let sections = input.split(" ");
        let mut style = style;
        for section in sections {
            match section {
                "flex" => {
                    style = style.flex();
                }
                "flex-row" => {
                    style = style.flex_row();
                }
                "flex-col" => {
                    style = style.flex_col();
                }
                "semibold" => {
                    style = style.font_weight(Weight::SEMIBOLD);
                }
                "bold" => {
                    style = style.font_weight(Weight::BOLD);
                }
                "extra-bold" => {
                    style = style.font_weight(Weight::EXTRA_BOLD);
                }
                "thin" => {
                    style = style.font_weight(Weight::THIN);
                }
                "medium" => {
                    style = style.font_weight(Weight::MEDIUM);
                }
                "extra-light" => {
                    style = style.font_weight(Weight::LIGHT);
                }
                _ => {}
            };
            let chars = section.chars().collect::<Vec<_>>();
            style = match chars.as_slice() {
                ['p', '-', ..] => read_pixels(section, style, Style::padding),
                ['p', 'x', '-', ..] => read_pixels(section, style, Style::padding_horiz),
                ['p', 'y', '-', ..] => read_pixels(section, style, Style::padding_vert),
                ['p', 't', '-', ..] => read_pixels(section, style, Style::padding_top),
                ['p', 'b', '-', ..] => read_pixels(section, style, Style::padding_bottom),
                ['p', 'l', '-', ..] => read_pixels(section, style, Style::padding_left),
                ['p', 'r', '-', ..] => read_pixels(section, style, Style::padding_right),
                ['m', '-', ..] => read_pixels(section, style, Style::margin_horiz),
                ['m', 'x', '-', ..] => read_pixels(section, style, Style::margin_vert),
                ['m', 'y', '-', ..] => read_pixels(section, style, Style::margin_top),
                ['m', 't', '-', ..] => read_pixels(section, style, Style::margin_bottom),
                ['m', 'b', '-', ..] => read_pixels(section, style, Style::margin_left),
                ['m', 'l', '-', ..] => read_pixels(section, style, Style::margin_right),
                ['m', 'r', '-', ..] => read_pixels(section, style, Style::margin_right),
                ['r', 'o', 'u', 'n', 'd', 'e', 'd', '-', ..] => {
                    read_pixels(section, style, Style::border_radius)
                }
                ['b', 'g', '-', ..] => {
                    let color_only = section.trim_start_matches("bg-");
                    let split = color_only.split_once('/');
                    if let Some((color, end)) = split {
                        if let (Ok(end), Some(color)) =
                            (end.parse::<u16>(), tailwind::COLOR_MAP.get(color))
                        {
                            style.background(color.clone().with_alpha_factor(end as f32 / 100.0))
                        } else {
                            style
                        }
                    } else {
                        match tailwind::COLOR_MAP.get(color_only) {
                            Some(color) => style.background(color.clone()),
                            None => style,
                        }
                    }
                }
                _ => {
                    println!("warning unrecoginzed style: {section}");
                    style
                }
            };
        }
        style
    }

    pub fn tw(
        string: &str,
        signal: ReadSignal<Option<usize>>,
    ) -> impl Fn(floem::style::Style) -> floem::style::Style {
        dbg!(signal.get());
        let owned = string.to_owned();
        move |style| (dbg!(parse_tailwind(owned.clone(), style)))
    }
}

fn main() -> Result<(), ProgramError> {
    println!("{}", search::nix_system());
    let sources = [
        "github:akirak/flake-templates",
        "github:andystopia/nix-templates",
        "github:NixOS/templates",
        "github:NixOS/templates",
        "github:NixOS/templates",
        "github:NixOS/templates",
        "github:NixOS/templates",
        "github:NixOS/templates",
        "github:NixOS/templates",
        "github:NixOS/templates",
        "github:NixOS/templates",
        "github:NixOS/templates",
        "github:NixOS/templates",
        "github:NixOS/templates",
        "github:NixOS/templates",
        "github:NixOS/templates",
        "github:NixOS/templates",
        "github:NixOS/templates",
        "github:NixOS/templates",
    ]
    .map(nix_templates)
    .into_iter()
    .collect::<Result<Vec<NixTemplates>, _>>()?;

    // dbg!(nix_flake_init(
    //     "github:andystopia/nix-templates",
    //     "typst-math",
    //     "testing"
    // )?);

    floem::Application::new()
        .window(
            move |_| {
                container_box(nixpkgs_search_window())
                    .style(|s| s.background(theme().bg).color(theme().fg).width_full())
                    .on_event(EventListener::WindowClosed, |_| {
                        quit_app();
                        floem::EventPropagation::Stop
                    })
            },
            Some(WindowConfig::default().show_titlebar(false).resizable(true)),
        )
        .run();
    Ok(())
}
