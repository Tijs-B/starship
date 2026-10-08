use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[cfg_attr(
    feature = "config-schema",
    derive(schemars::JsonSchema),
    schemars(deny_unknown_fields)
)]
#[serde(default)]
pub struct JJWorkspaceConfig<'a> {
    pub format: &'a str,
    pub style: &'a str,
    pub show_default: bool,
    pub disabled: bool,
}

impl Default for JJWorkspaceConfig<'_> {
    fn default() -> Self {
        Self {
            format: "[$workspace]($style) ",
            style: "bold cyan",
            show_default: false,
            disabled: false,
        }
    }
}
