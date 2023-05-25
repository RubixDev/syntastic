use std::{
    collections::{BTreeMap, HashMap},
    env, fs,
};

use anyhow::Result;
use syntastica::config::ThemeValue;

mod parsers_gitdep;
mod queries;
mod theme_gruvbox;
mod theme_one;

#[derive(Clone, Debug)]
enum RawThemeValue {
    Link(String),
    Styles(HashMap<String, String>),
    Ignore,
}

fn is_arg(test: &str) -> bool {
    env::args().nth(2).map_or(true, |arg| arg == test)
}

pub fn run() -> Result<()> {
    if is_arg("queries") {
        let mut queries_lib_rs = r###"
//! This crate defines constants for three types of tree-sitter queries for lots of parsers.
//! It is intended to be used via [syntastica](https://crates.io/crates/syntastica).
//!
//! The three types of queries are:
//!
//! 1. `highlights`: defining the highlight captures for nodes
//! 2. `injections`: defining where other languages are injected for highlighting
//! 3. `locals`: keeping track of scopes, variables, parameters, etc. to have occurrences of those
//!    be highlighted the same everywhere
//!
//! The constants are defined as `<language_name>_<kind>` where `<kind>` is one of `HIGHLIGHTS`,
//! `INJECTIONS`, or `LOCALS`. The `INJECTIONS` and `LOCALS` may be empty for some languages.
//!
//! The crate source is automatically generated with `cargo xtask codegen` inside the
//! syntastica workspace.
#![cfg_attr(all(doc, CHANNEL_NIGHTLY), feature(doc_auto_cfg))]
#![cfg_attr(rustfmt, rustfmt_skip)]
"###
        .trim_start()
        .to_owned();

        let queries_dir = crate::WORKSPACE_DIR.join("syntastica-queries/generated_queries");
        let _ = fs::remove_dir_all(&queries_dir);
        fs::create_dir_all(&queries_dir)?;
        fs::write(
            queries_dir.join("README.md"),
            include_str!("./codegen/generated_queries_readme.md"),
        )?;

        for (name, [highlights, injections, locals]) in queries::make_queries()? {
            let lang_dir = queries_dir.join(name);
            fs::create_dir(&lang_dir)?;

            fs::write(lang_dir.join("highlights.scm"), highlights)?;
            fs::write(lang_dir.join("injections.scm"), injections)?;
            fs::write(lang_dir.join("locals.scm"), locals)?;

            queries_lib_rs += &format!(
                r###"
pub const {lang}_HIGHLIGHTS: &str = include_str!("../generated_queries/{name}/highlights.scm");
pub const {lang}_INJECTIONS: &str = include_str!("../generated_queries/{name}/injections.scm");
pub const {lang}_LOCALS: &str = include_str!("../generated_queries/{name}/locals.scm");
"###,
                lang = name.to_uppercase()
            )
        }
        fs::write(
            crate::WORKSPACE_DIR.join("syntastica-queries/src/lib.rs"),
            queries_lib_rs,
        )?;
    }

    if is_arg("parsers-gitdep") {
        parsers_gitdep::write()?;
    }

    if is_arg("themes") {
        fs::write(
            crate::WORKSPACE_DIR.join("syntastica-themes/src/gruvbox.rs"),
            theme_gruvbox::make_theme()?,
        )?;
        fs::write(
            crate::WORKSPACE_DIR.join("syntastica-themes/src/one.rs"),
            theme_one::make_theme()?,
        )?;
    }

    Ok(())
}

const INDENT: &str = "    ";
type Theme = BTreeMap<String, ThemeValue>;
fn to_theme_macro_call(map: &Theme) -> String {
    let mut out = "theme! {\n".to_owned();
    for (key, value) in map {
        let value = match value {
            ThemeValue::Simple(str) => format!("\"{str}\""),
            ThemeValue::Extended {
                color,
                underline,
                strikethrough,
                italic,
                bold,
                link,
            } => {
                let mut value = "{\n".to_owned();
                value += &format!(
                    "{}color: {},\n",
                    INDENT.repeat(3),
                    color
                        .as_ref()
                        .map(|s| format!("{s:?}"))
                        .unwrap_or("None".to_owned()),
                );
                value += &format!("{}underline: {},\n", INDENT.repeat(3), underline);
                value += &format!("{}strikethrough: {},\n", INDENT.repeat(3), strikethrough);
                value += &format!("{}italic: {},\n", INDENT.repeat(3), italic);
                value += &format!("{}bold: {},\n", INDENT.repeat(3), bold);
                value += &format!(
                    "{}link: {},\n",
                    INDENT.repeat(3),
                    link.as_ref()
                        .map(|s| format!("{s:?}"))
                        .unwrap_or("None".to_owned()),
                );
                value += &format!("{}}}", INDENT.repeat(2));
                value
            }
        };
        out += &format!("{}\"{key}\": {value},\n", INDENT.repeat(2));
    }
    out + INDENT + "}"
}

fn resolve_links(raw_theme: &mut BTreeMap<String, RawThemeValue>) {
    let mut links_left = true;
    while links_left {
        links_left = false;
        let raw_theme_copy = raw_theme.clone();
        for (key, value) in &mut *raw_theme {
            if !key.starts_with('@') {
                continue;
            }
            if let RawThemeValue::Link(link) = value {
                links_left = true;
                match raw_theme_copy.get(link) {
                    Some(new_value) => *value = new_value.clone(),
                    None => {
                        eprintln!("warning: ignoring key {key} because of invalid link to {link}");
                        *value = RawThemeValue::Ignore;
                    }
                }
            }
        }
    }
}

fn make_theme_file(
    name: &str,
    url: &str,
    palettes: BTreeMap<&&str, Theme>,
    theme: Theme,
) -> String {
    let mut out = format!("//! The {name} themes in this module were extracted from <{url}>");
    out += r###"
//!
//! The module source is automatically generated with `cargo xtask codegen` inside the
//! syntastica workspace.

use std::collections::BTreeMap;

use syntastica::{
    config::{Config, ThemeValue},
    theme,
};
"###;

    for (variant, palette) in palettes {
        out += &format!(
            r###"
pub fn {variant}() -> Config {{
    let mut palette = {theme}
    .into_inner();
    palette.append(&mut theme());
    palette.into()
}}
"###,
            theme = to_theme_macro_call(&palette),
        )
    }

    out += &format!(
        r###"
fn theme() -> BTreeMap<String, ThemeValue> {{
    {theme}
    .into_inner()
}}
"###,
        theme = to_theme_macro_call(&theme)
    );

    out
}
