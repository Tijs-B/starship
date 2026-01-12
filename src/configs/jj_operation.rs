use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[cfg_attr(
    feature = "config-schema",
    derive(schemars::JsonSchema),
    schemars(deny_unknown_fields)
)]
#[serde(default)]
pub struct JJOperationConfig<'a> {
    pub format: &'a str,
    pub style: &'a str,
    pub operation_length: usize,
}

impl Default for JJOperationConfig<'_> {
    fn default() -> Self {
        Self {
            format: "[$operation]($style) ",
            style: "blue",
            operation_length: 12,
        }
    }
}
