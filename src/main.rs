#![allow(dead_code)]
use std::{collections::HashMap, process::Stdio};

use floem::cosmic_text::Weight;

use floem::event::Event;
use floem::keyboard::{Key, KeyCode, KeyEvent, NamedKey};
use floem::peniko::Color;
use floem::reactive::{create_memo, create_rw_signal, ReadSignal, RwSignal};
use floem::style::StyleValue;
use floem::views::{container_box, drag_window_area, list, text_input};
use floem::{quit_app, views};
// use floem::widgets::{button, text_input};
use floem::view::View;
use floem::window::WindowConfig;

#[allow(unused_imports)]
use tap::{Conv, Pipe};

use floem::{
    event::EventListener,
    style::Position,
    views::{
        container, h_stack, label, scroll, v_stack, Decorators, VirtualListDirection,
        VirtualListItemSize,
    },
};

use bstr::ByteSlice;
use serde::{Deserialize, Serialize};
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

use snafu::prelude::*;

const DARK_BG: Color = Color::rgb8(28, 30, 31);
const DARK_BD: Color = Color::rgb8(28, 30 - 5, 31 - 5);
const DARK_TEXT: Color = Color::rgb8(222, 222, 222);
const TRANSPARENT_LOW: Color = Color::rgba8(255, 255, 255, 10);
const ACCENT_COLOR: Color = Color::rgba8(0, 122 - 15, 204 - 15, 255);
const ACCENT_COLOR_SEMI: Color = Color::rgba8(0, 122 - 15, 204 - 15, 155);

use im::Vector;

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
        "Error in external process.
            While attempting {goal}, by using {command}, we 
            encountered the following erorr: 

                {exit_code:?}
            hint: as of right now exit code must be zero to succeed
        "
    ))]
    BadExitCode {
        goal: String,
        command: String,
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

    cmd.args(["--json", "--refresh"]);

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

fn flake_list(
    sidebar_width: f64,
    flake_sources: RwSignal<Vector<String>>,
    selection_state: RwSignal<SelectedFlakeOption>,
    templates: RwSignal<Vec<NixTemplates>>,
) -> impl View {
    views::virtual_list(
        VirtualListDirection::Vertical,
        VirtualListItemSize::Fixed(Box::new(|| 24.0)),
        move || {
            flake_sources
                .get()
                .into_iter()
                .enumerate()
                .collect::<Vector<_>>()
        },
        move |item| item.clone(),
        move |(idx, item)| {
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
                        .width(sidebar_width - 15.0)
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
                    dbg!(selection_state.get());
                }),
                list(
                    move || {
                        // todo
                        let selection_state = selection_state.get();
                        if selection_state.is_flake_source(idx) {
                            return templates.get()[idx]
                                .templates
                                .clone()
                                .into_iter()
                                .enumerate()
                                .collect();
                        }
                        vec![]
                    },
                    |item| item.1.name.clone(),
                    move |(idx, view)| {
                        label(move || view.name.clone())
                            .style(move |s| {
                                s.padding(15.0)
                                    .padding_top(4.0)
                                    .padding_bottom(4.0)
                                    .width_full()
                                    .items_start()
                                    .font_weight(StyleValue::Val(Weight::MEDIUM))
                                    .border_radius(5.0)
                                    .apply_if(selection_state.get().is_template(idx), |s| {
                                        s.background(ACCENT_COLOR)
                                    })
                                    .hover(|s| {
                                        s.apply_if(!selection_state.get().is_template(idx), |s| {
                                            s.background(TRANSPARENT_LOW)
                                        })
                                    })
                            })
                            .on_click_stop(move |_| {
                                selection_state.update(|state| *state = state.select_template(idx))
                            })
                            .pipe(container)
                            .style(|s| s.padding_left(10.0).margin_top(4.0))
                    },
                )
                .style(|s| s.flex_col()),
            ))
        },
    )
    .style(|s| s.flex_col().gap(0.0, 3.0))
}
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

    let description = create_memo(move |_| {
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

    // let (options, set_options) =
    //     create_signal(vec![Tabs::People, Tabs::Companies, Tabs::Jobs].conv::<Vector<_>>());

    // let (active_option, set_active_option) = create_signal(Tabs::People);
    let search_text = create_rw_signal("".to_owned());
    let searcher = text_input(search_text)
        .placeholder("Search")
        .style(|s| {
            s.width_full()
                .background(TRANSPARENT_LOW)
                .color(DARK_TEXT)
                .cursor_color(DARK_TEXT)
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

    let top_bar = drag_window_area(
        h_stack((
            views::empty(),
            container(label(|| "Create Project").pipe(container).style(|s| {
                s.padding_vert(0.0)
                    .height_full()
                    .padding_horiz(15.0)
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .background(ACCENT_COLOR_SEMI)
                    .hover(|s| s.background(ACCENT_COLOR))
                    .border_radius(5.0)
            }))
            .style(|s| s.padding_vert(5.0).padding_horiz(7.0)),
        ))
        .style(|s| s.flex_row().justify_between().width_full()),
    )
    .style(|s| s.width_full().height(TOPBAR_HEIGHT).justify_between());

    let side_bar = scroll({
        container(
            views::virtual_list(
                VirtualListDirection::Vertical,
                VirtualListItemSize::Fixed(Box::new(|| 24.0)),
                move || {
                    flake_sources
                        .get()
                        .into_iter()
                        .enumerate()
                        .collect::<Vector<_>>()
                },
                move |item| item.clone(),
                move |(idx, item)| {
                    v_stack((
                        h_stack((
                            views::svg(|| {
                                include_str!("../assets/github-mark-white.svg").to_owned()
                            })
                            .style(|s| s.height(12.0).aspect_ratio(Some(1.0))),
                            label(move || item.clone()),
                        ))
                        .style(move |s| {
                            s.padding(8.0)
                                .gap(10.0, 0.0)
                                .padding_top(8.0)
                                .padding_bottom(8.0)
                                .width(SIDEBAR_WIDTH - 15.0)
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
                            dbg!(selection_state.get());
                        }),
                        list(
                            move || {
                                // todo
                                let selection_state = selection_state.get();
                                if selection_state.is_flake_source(idx) {
                                    return templates.get()[idx]
                                        .templates
                                        .clone()
                                        .into_iter()
                                        .enumerate()
                                        .collect();
                                }
                                vec![]
                            },
                            |item| item.1.name.clone(),
                            move |(idx, view)| {
                                label(move || view.name.clone())
                                    .style(move |s| {
                                        s.padding(15.0)
                                            .padding_top(4.0)
                                            .padding_bottom(4.0)
                                            .width_full()
                                            .items_start()
                                            .font_weight(StyleValue::Val(Weight::MEDIUM))
                                            .border_radius(5.0)
                                            .apply_if(selection_state.get().is_template(idx), |s| {
                                                s.background(ACCENT_COLOR)
                                            })
                                            .hover(|s| {
                                                s.apply_if(
                                                    !selection_state.get().is_template(idx),
                                                    |s| s.background(TRANSPARENT_LOW),
                                                )
                                            })
                                    })
                                    .on_click_stop(move |_| {
                                        selection_state
                                            .update(|state| *state = state.select_template(idx))
                                    })
                                    .pipe(container)
                                    .style(|s| s.padding_left(10.0).margin_top(4.0))
                            },
                        )
                        .style(|s| s.flex_col()),
                    ))
                },
            )
            .style(|s| s.flex_col().gap(0.0, 3.0)),
        )
    })
    .hide_bar(|| true)
    .style(|s| {
        s.width(SIDEBAR_WIDTH)
            .border_right(1.0)
            .color(DARK_TEXT)
            .border_top(1.0)
            .padding_left(5.0)
            .background(DARK_BG)
            .padding_right(5.0)
            .border_color(DARK_BD)
    });

    let main_window = scroll(
        v_stack((
            label(move || String::from("Description")).style(|s| {
                s.padding(10.0)
                    .font_size(18.0)
                    .font_weight(Weight::SEMIBOLD)
            }),
            label(move || description.get()).style(move |s| s.max_width_full()),
        ))
        .style(|s| {
            s.flex_col()
                .border_horiz(1.0)
                .padding_horiz(15.0)
                .border_color(DARK_BD)
                .min_height_full()
                .padding_top(15.0)
                .items_start()
                .width_full()
                .padding_bottom(10.0)
                .max_width(420.0)
        }),
    )
    .style(|s| {
        s.flex_col()
            .flex_basis(0)
            .min_width(0)
            .flex_grow(1.0)
            .border_top(1.0)
            .items_center()
            .border_color(DARK_BD)
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

fn main() -> Result<(), ProgramError> {
    let sources = vec![
        nix_templates("github:andystopia/nix-templates")?,
        nix_templates("github:NixOS/templates")?,
        nix_templates("github:akirak/flake-templates")?,
    ];

    // dbg!(nix_flake_init(
    //     "github:andystopia/nix-templates",
    //     "typst-math",
    //     "testing"
    // )?);

    floem::Application::new()
        .window(
            move |_| {
                container_box(app_view(sources))
                    .style(|s| s.background(DARK_BG).color(DARK_TEXT).width_full())
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
