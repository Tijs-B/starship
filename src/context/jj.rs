use std::path::{Path, PathBuf};
use std::sync::Arc;

use jj_lib::commit::Commit;
use jj_lib::config::StackedConfig;
use jj_lib::ref_name::WorkspaceNameBuf;
use jj_lib::repo::{ReadonlyRepo, Repo as _};
use jj_lib::settings::UserSettings;
use jj_lib::workspace::Workspace;

pub struct Repo {
    pub workdir: PathBuf,
    pub workspace_name: WorkspaceNameBuf,
    pub repo: Arc<ReadonlyRepo>,
}

pub fn init_repo(cwd: &Path) -> Option<Repo> {
    fn ok<T, E: std::fmt::Display>(r: Result<T, E>) -> Option<T> {
        r.inspect_err(|e| log::warn!("while loading jj repo: {e}"))
            .ok()
    }

    let workspace_dir = cwd.ancestors().find(|path| path.join(".jj").is_dir())?;

    let settings = ok(UserSettings::from_config(StackedConfig::with_defaults()))?;
    let store_factories = Default::default();
    let working_copy_factories = jj_lib::workspace::default_working_copy_factories();
    let workspace = ok(Workspace::load(
        &settings,
        workspace_dir,
        &store_factories,
        &working_copy_factories,
    ))?;
    let repo = ok(workspace.repo_loader().load_at_head())?;

    Some(Repo {
        workdir: workspace_dir.into(),
        repo,
        workspace_name: workspace.workspace_name().into(),
    })
}

pub trait OrLog {
    type Output;
    fn or_log(self, module: &str) -> Self::Output;
}

impl<T, E: std::fmt::Display> OrLog for Result<T, E> {
    type Output = Option<T>;

    fn or_log(self, module: &str) -> Self::Output {
        self.inspect_err(|e| log::warn!("in {module}: {e}")).ok()
    }
}

pub fn get_working_copy(repo: &Repo, mod_name: &str) -> Option<Commit> {
    repo.repo
        .store()
        .get_commit(repo.repo.view().get_wc_commit_id(&repo.workspace_name)?)
        .or_log(mod_name)
}
