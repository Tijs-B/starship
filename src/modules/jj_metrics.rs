// Did not want to depend on all of jj-cli. Many of these are inlined from:
// https://github.com/jj-vcs/jj/blob/6c14ccd89df3f4445ba0e362c17cdd56a13127af/cli/src/diff_util.rs
//
// The original code had the following license notification:
//
// Copyright 2020-2022 The Jujutsu Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::io;

use futures::executor::{block_on, block_on_stream};
use jj_lib::commit::Commit;
use jj_lib::conflict_labels::ConflictLabels;
use jj_lib::conflicts::{
    ConflictMarkerStyle, ConflictMaterializeOptions, MaterializedFileConflictValue,
    MaterializedFileValue, MaterializedTreeDiffEntry, MaterializedTreeValue,
    materialize_merge_result_to_bytes, materialized_diff_stream,
};
use jj_lib::copies::CopyRecords;
use jj_lib::diff::{CompareBytesExactly, ContentDiff, DiffHunkKind, find_line_ranges};
use jj_lib::matchers::EverythingMatcher;
use jj_lib::merge::Diff;
use jj_lib::repo::Repo as _;
use jj_lib::repo_path::RepoPath;
use jj_lib::rewrite::merge_commit_trees;
use jj_lib::tree_merge::MergeOptions;
use tokio::io::AsyncReadExt;

use crate::config::ModuleConfig as _;
use crate::configs::jj_metrics::JJMetricsConfig;
use crate::context::Context;
use crate::context::jj::{OrLog as _, Repo, get_working_copy};
use crate::formatter::StringFormatter;
use crate::module::Module;

pub fn module<'a>(context: &'a Context) -> Option<Module<'a>> {
    let mod_name = "jj_metrics";
    let mut module = context.new_module(mod_name);
    let config = JJMetricsConfig::try_load(module.config);

    let repo = context.get_jj_repo()?;
    let wc = get_working_copy(repo, mod_name)?;

    let parents = wc
        .parents()
        .collect::<Result<Vec<_>, _>>()
        .or_log(mod_name)?;
    let from_tree = block_on(merge_commit_trees(repo.repo.as_ref(), &parents)).or_log(mod_name)?;
    let to_tree = wc.tree();

    let mut copy_records = CopyRecords::default();
    for p in &parents {
        get_copy_records(mod_name, repo, &wc, &mut copy_records, p)?;
    }

    let diff = from_tree.diff_stream_with_copies(&to_tree, &EverythingMatcher, &copy_records);
    let unlabeled = ConflictLabels::unlabeled();
    let conflict_labels = jj_lib::merge::Diff {
        before: &unlabeled,
        after: &unlabeled,
    };
    let diff = materialized_diff_stream(repo.repo.store(), diff, conflict_labels);

    let (added, deleted) = run_diff(block_on_stream(diff)).or_log(mod_name)?;
    let added = if config.only_nonzero_diffs && added == 0 {
        None
    } else {
        Some(added)
    };
    let deleted = if config.only_nonzero_diffs && deleted == 0 {
        None
    } else {
        Some(deleted)
    };

    let parsed = StringFormatter::new(config.format).and_then(|formatter| {
        formatter
            .map_style(|variable| match variable {
                "added_style" => Some(Ok(config.added_style)),
                "deleted_style" => Some(Ok(config.deleted_style)),
                _ => None,
            })
            .map(|variable| match variable {
                "added" => added.map(|v| Ok(format!("{v}"))),
                "deleted" => deleted.map(|v| Ok(format!("{v}"))),
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

// from show_diff_stat() in jj_cli/src/diff_util.rs
pub fn run_diff(
    mut diff_tree: impl Iterator<Item = MaterializedTreeDiffEntry>,
) -> Result<(usize, usize), Box<dyn std::error::Error>> {
    diff_tree.try_fold(
        (0, 0),
        |mut sums, MaterializedTreeDiffEntry { path, values }| {
            let Diff { before, after } = values?;
            let content_before = diff_content(path.source(), before)?;
            let content_after = diff_content(path.target(), after)?;

            let (added, deleted) = get_diff_stat(&content_before, &content_after);
            sums.0 += added;
            sums.1 += deleted;

            Ok::<_, Box<dyn std::error::Error>>(sums)
        },
    )
}

fn get_diff_stat(left: &[u8], right: &[u8]) -> (usize, usize) {
    let diff = ContentDiff::for_tokenizer([left, right], find_line_ranges, CompareBytesExactly);
    let mut added = 0;
    let mut removed = 0;
    for hunk in diff.hunks() {
        match hunk.kind {
            DiffHunkKind::Matching => {}
            DiffHunkKind::Different => {
                let [left, right] = hunk.contents[..].try_into().unwrap();
                removed += left.split_inclusive(|b| *b == b'\n').count();
                added += right.split_inclusive(|b| *b == b'\n').count();
            }
        }
    }
    (added, removed)
}

fn diff_content(path: &RepoPath, value: MaterializedTreeValue) -> io::Result<Vec<u8>> {
    match value {
        MaterializedTreeValue::Absent => Ok(Vec::new()),
        MaterializedTreeValue::AccessDenied(err) => {
            Ok(format!("Access denied: {err}").into_bytes())
        }
        MaterializedTreeValue::File(MaterializedFileValue { mut reader, .. }) => {
            let mut buf = Vec::new();
            block_on(reader.read_to_end(&mut buf))?;
            Ok(buf)
        }
        MaterializedTreeValue::Symlink { id: _, target } => Ok(target.into_bytes()),
        MaterializedTreeValue::GitSubmodule(id) => {
            Ok(format!("Git submodule checked out at {id}").into_bytes())
        }
        MaterializedTreeValue::FileConflict(MaterializedFileConflictValue {
            contents,
            labels,
            ..
        }) => {
            let opts = ConflictMaterializeOptions {
                marker_style: ConflictMarkerStyle::Git,
                marker_len: None,
                merge: MergeOptions {
                    hunk_level: jj_lib::files::FileMergeHunkLevel::Line,
                    same_change: jj_lib::merge::SameChange::Accept,
                },
            };
            Ok(materialize_merge_result_to_bytes(&contents, &labels, &opts).into())
        }
        MaterializedTreeValue::OtherConflict { id, labels } => {
            Ok(id.describe(&labels).into_bytes())
        }
        MaterializedTreeValue::Tree(id) => {
            panic!("Unexpected tree with id {id:?} in diff at path {path:?}");
        }
    }
}

pub fn get_copy_records(
    mod_name: &str,
    repo: &Repo,
    wc: &Commit,
    copy_records: &mut CopyRecords,
    p: &Commit,
) -> Option<()> {
    let records = repo
        .repo
        .store()
        .get_copy_records(None, p.id(), wc.id())
        .or_log(mod_name)?;
    copy_records
        .add_records(block_on_stream(records))
        .or_log(mod_name)
}
