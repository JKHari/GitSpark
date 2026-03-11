use std::path::Path;

use once_cell::sync::Lazy;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

pub static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
pub static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

const DIFF_THEME_NAME: &str = "base16-ocean.dark";

pub fn syntax_theme() -> &'static Theme {
    THEME_SET
        .themes
        .get(DIFF_THEME_NAME)
        .expect("missing diff theme")
}

pub fn get_syntax(file_path: &str, first_line: Option<&str>) -> &'static SyntaxReference {
    let path = Path::new(file_path);

    if let Some(file_name) = path.file_name().and_then(|name| name.to_str()) {
        if let Some(syntax) = match file_name {
            "Dockerfile" => SYNTAX_SET.find_syntax_by_name("Dockerfile"),
            "Cargo.lock" | "Cargo.toml" => SYNTAX_SET.find_syntax_by_extension("toml"),
            "Makefile" => SYNTAX_SET.find_syntax_by_name("Makefile"),
            _ => None,
        } {
            return syntax;
        }
    }

    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        if let Some(syntax) = SYNTAX_SET.find_syntax_by_extension(extension) {
            return syntax;
        }
    }

    if let Some(line) = first_line {
        if let Some(syntax) = SYNTAX_SET.find_syntax_by_first_line(line) {
            return syntax;
        }
    }

    SYNTAX_SET.find_syntax_plain_text()
}
