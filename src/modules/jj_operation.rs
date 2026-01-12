use crate::config::ModuleConfig as _;
use crate::configs::jj_operation::JJOperationConfig;
use crate::context::Context;
use crate::formatter::StringFormatter;
use crate::module::Module;

pub fn module<'a>(context: &'a Context) -> Option<Module<'a>> {
    let mod_name = "jj_operation";
    let mut module = context.new_module(mod_name);
    let config = JJOperationConfig::try_load(module.config);

    let repo = context.get_jj_repo()?;

    let mut op_id = repo.repo.op_id().to_string();
    op_id.truncate(config.operation_length);

    let parsed = StringFormatter::new(config.format).and_then(|formatter| {
        formatter
            .map_style(|variable| match variable {
                "style" => Some(Ok(config.style)),
                _ => None,
            })
            .map(|variable| match variable {
                "operation" => Some(Ok(&op_id)),
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
