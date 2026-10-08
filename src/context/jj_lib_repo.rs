use std::path::{Path, PathBuf};
use std::sync::Arc;

use jj_lib::commit::Commit;
use jj_lib::config::StackedConfig;
use jj_lib::default_backend_factories::default_backend_factories;
use jj_lib::default_backend_factories::default_working_copy_factories;
use jj_lib::op_walk::closest_common_ancestors;
use jj_lib::ref_name::WorkspaceNameBuf;
use jj_lib::repo::{ReadonlyRepo, Repo as _};
use jj_lib::settings::UserSettings;
use jj_lib::workspace::Workspace;

use pollster::FutureExt as _;

pub struct Repo {
    pub workdir: PathBuf,
    pub workspace_name: WorkspaceNameBuf,
    pub repo: Arc<ReadonlyRepo>,
    /// Whether the working copy is behind the repo and needs `jj workspace update-stale`.
    pub stale: bool,
}

pub fn init_repo(cwd: &Path) -> Option<Repo> {
    fn ok<T, E: std::fmt::Display>(r: Result<T, E>) -> Option<T> {
        r.inspect_err(|e| log::warn!("while loading jj repo: {e}"))
            .ok()
    }

    let workspace_dir = cwd.ancestors().find(|path| path.join(".jj").is_dir())?;

    let settings = ok(UserSettings::from_config_and_home_dir(
        StackedConfig::with_defaults(),
        dirs::home_dir(),
    ))?;
    let store_factories = default_backend_factories();
    let working_copy_factories = default_working_copy_factories();
    let workspace = ok(Workspace::load(
        &settings,
        workspace_dir,
        &store_factories,
        &working_copy_factories,
    ))?;
    let repo = ok(workspace.repo_loader().load_at_head().block_on())?;

    let stale = is_working_copy_stale(&workspace, &repo).unwrap_or(false);

    Some(Repo {
        workdir: workspace_dir.into(),
        repo,
        workspace_name: workspace.workspace_name().into(),
        stale,
    })
}

/// Read-only equivalent of `WorkingCopyFreshness::check_stale`.
///
/// `check_stale` needs a `LockedWorkingCopy`, which takes the working-copy lock. A prompt must
/// never wait on or hold that lock, so this compares the same data without locking.
fn is_working_copy_stale(workspace: &Workspace, repo: &Arc<ReadonlyRepo>) -> Option<bool> {
    let working_copy = workspace.working_copy();
    if working_copy.operation_id() == repo.op_id() {
        return Some(false);
    }

    let wc_operation = repo
        .loader()
        .load_operation(working_copy.operation_id())
        .block_on()
        .or_log("jj_lib_repo")?;
    let repo_operation = repo.operation();
    let ancestor = closest_common_ancestors([wc_operation.clone()], [repo_operation.clone()])
        .block_on()
        .or_log("jj_lib_repo")?
        .into_iter()
        .next()?;

    // The working copy is only stale when the repo moved on without it. A working copy that is
    // ahead of the repo, or on a sibling operation, is a different situation.
    if ancestor.id() != wc_operation.id() || ancestor.id() == repo_operation.id() {
        return Some(false);
    }

    let wc_commit = repo
        .store()
        .get_commit(repo.view().get_wc_commit_id(workspace.workspace_name())?)
        .or_log("jj_lib_repo")?;
    let wc_tree = working_copy.tree().or_log("jj_lib_repo")?;
    Some(wc_tree.tree_ids_and_labels() != wc_commit.tree().tree_ids_and_labels())
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
