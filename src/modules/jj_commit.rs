use jj_lib::backend::ChangeId;
use jj_lib::id_prefix::{IdPrefixContext, IdPrefixIndex};
use jj_lib::index::IndexResult;
use jj_lib::repo::ReadonlyRepo;

use crate::config::ModuleConfig as _;
use crate::configs::jj_commit::JJCommitConfig;
use crate::context::Context;
use crate::context::jj::{OrLog as _, get_working_copy};
use crate::formatter::StringFormatter;
use crate::module::Module;

pub fn module<'a>(context: &'a Context) -> Option<Module<'a>> {
    let mod_name = "jj_commit";
    let mut module = context.new_module(mod_name);
    let config = JJCommitConfig::try_load(module.config);

    let repo = context.get_jj_repo()?;
    let wc = get_working_copy(repo, mod_name)?;

    let ctx = IdPrefixContext::new(Default::default());
    let index = ctx.populate(repo.repo.as_ref()).or_log(mod_name)?;

    let (prefix, rest) =
        shortest(repo.repo.as_ref(), &index, wc.change_id(), 8).or_log(mod_name)?;

    let desc = wc.description().lines().next();
    let (desc, desc_style) = desc.filter(|d| !d.trim().is_empty()).map_or(
        (
            config.description_empty.to_string(),
            config.style_description_empty,
        ),
        |d| {
            (
                truncate_description(d.trim(), config.description_length),
                config.style_description,
            )
        },
    );

    let conflicted = if wc.has_conflict() {
        config.conflicted_string
    } else {
        ""
    };

    let empty = if wc.is_empty(repo.repo.as_ref()).ok()? {
        config.empty_string
    } else {
        ""
    };

    let mut op_id = repo.repo.op_id().to_string();
    op_id.truncate(4);

    let parsed = StringFormatter::new(config.format).and_then(|formatter| {
        formatter
            .map_style(|variable| match variable {
                "style_prefix" => Some(Ok(config.style_prefix)),
                "style_rest" => Some(Ok(config.style_rest)),
                "style_description" => Some(Ok(desc_style)),
                "style_conflicted" => Some(Ok(config.style_conflicted)),
                "style_empty" => Some(Ok(config.style_empty)),
                _ => None,
            })
            .map(|variable| match variable {
                "prefix" => Some(Ok(prefix.as_str())),
                "rest" => Some(Ok(rest.as_str())),
                "description" => Some(Ok(&desc)),
                "conflicted" => Some(Ok(conflicted)),
                "empty" => Some(Ok(empty)),
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

fn truncate_description(desc: &str, length: usize) -> String {
    if desc.len() > length {
        let truncate_length = length - 1;
        let mut truncated = desc[..truncate_length].to_string();
        truncated.push('…');
        truncated
    } else {
        desc.to_string()
    }
}

fn shortest(
    repo: &ReadonlyRepo,
    index: &IdPrefixIndex,
    id: &ChangeId,
    total_len: usize,
) -> IndexResult<(String, String)> {
    let prefix_len = index.shortest_change_prefix_len(repo, id)?;
    let mut hex = id.reverse_hex();
    hex.truncate(total_len);
    let rest = hex.split_off(prefix_len);
    Ok((hex, rest))
}
