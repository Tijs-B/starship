use crate::config::ModuleConfig as _;
use crate::configs::jj_stale::JJStaleConfig;
use crate::context::Context;
use crate::formatter::StringFormatter;
use crate::module::Module;

pub fn module<'a>(context: &'a Context) -> Option<Module<'a>> {
    let mod_name = "jj_stale";
    let mut module = context.new_module(mod_name);
    let config = JJStaleConfig::try_load(module.config);

    if config.disabled {
        return None;
    }

    if !context.get_jj_lib_repo()?.stale {
        return None;
    }

    let parsed = StringFormatter::new(config.format).and_then(|formatter| {
        formatter
            .map_meta(|variable, _| match variable {
                "symbol" => Some(config.symbol),
                _ => None,
            })
            .map_style(|variable| match variable {
                "style" => Some(Ok(config.style)),
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
