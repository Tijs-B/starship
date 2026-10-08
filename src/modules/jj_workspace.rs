use jj_lib::ref_name::WorkspaceName;

use crate::config::ModuleConfig as _;
use crate::configs::jj_workspace::JJWorkspaceConfig;
use crate::context::Context;
use crate::formatter::StringFormatter;
use crate::module::Module;

pub fn module<'a>(context: &'a Context) -> Option<Module<'a>> {
    let mod_name = "jj_workspace";
    let mut module = context.new_module(mod_name);
    let config = JJWorkspaceConfig::try_load(module.config);

    if config.disabled {
        return None;
    }

    let repo = context.get_jj_lib_repo()?;
    let workspace = repo.workspace_name.as_str();

    if !config.show_default && repo.workspace_name == WorkspaceName::DEFAULT {
        return None;
    }

    let parsed = StringFormatter::new(config.format).and_then(|formatter| {
        formatter
            .map_style(|variable| match variable {
                "style" => Some(Ok(config.style)),
                _ => None,
            })
            .map(|variable| match variable {
                "workspace" => Some(Ok(workspace)),
                _ => None,
            })
            .parse(None, Some(context))
    });

    module.set_segments(match parsed {
        Ok(segments) => segments,
        Err(error) => {
            log::warn!("Error in module `{mod_name}`:\n{error}");
            return None;
        }
    });

    Some(module)
}
