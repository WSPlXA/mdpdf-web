pub struct EmbeddedTheme {
    pub template: &'static str,
    pub style: &'static str,
    pub print: &'static str,
    pub config: &'static str,
}

const MODERN_TECH: EmbeddedTheme = EmbeddedTheme {
    template: include_str!("../themes/modern-tech/template.html"),
    style: include_str!("../themes/modern-tech/style.css"),
    print: include_str!("../themes/modern-tech/print.css"),
    config: include_str!("../themes/modern-tech/theme.yaml"),
};

const JP_STANDARD: EmbeddedTheme = EmbeddedTheme {
    template: include_str!("../themes/jp-standard/template.html"),
    style: include_str!("../themes/jp-standard/style.css"),
    print: include_str!("../themes/jp-standard/print.css"),
    config: include_str!("../themes/jp-standard/theme.yaml"),
};

pub const PRISM_CSS: &str = include_str!("../themes/common/prism.min.css");
pub const MERMAID_RUNTIME: &str = include_str!("../themes/common/mermaid.min.js");

pub fn embedded_theme(name: &str) -> Option<&'static EmbeddedTheme> {
    match name {
        "modern-tech" => Some(&MODERN_TECH),
        "jp-standard" => Some(&JP_STANDARD),
        _ => None,
    }
}
