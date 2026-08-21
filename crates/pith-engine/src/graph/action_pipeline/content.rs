use pith_core::{ActionSpec, Content};
use pith_diag::PithResult;
use pith_ids::ContentId;
use pith_store::{Tree, TreeEntry, TreeEntryContent};

use crate::action::{
    ActionExecution, ActionInvocation, CapturedFileContent, CapturedOutputContent,
    CapturedTreeEntryContent, ExecutionReport, MaterializedActionInput, MaterializedBlob,
    MaterializedContent, MaterializedFileContent, MaterializedTree, MaterializedTreeEntryContent,
    ProducedOutput,
};
use crate::graph::Engine;
use crate::graph::diagnostics::{
    InternalInvariant, content_unavailable_diag, internal_diag, store_error_diag,
};

impl Engine {
    pub(super) fn materialize_action(&self, spec: &ActionSpec) -> PithResult<ActionInvocation> {
        let mut inputs = Vec::with_capacity(spec.inputs.len());
        for input in &spec.inputs {
            let content = match &input.content {
                Content::Blob(id) => self.materialize_blob(*id)?,
                Content::Tree(id) => MaterializedContent::Tree(self.materialize_tree(*id)?),
            };
            inputs.push(MaterializedActionInput {
                path: input.path.clone(),
                content,
            });
        }
        let program = match spec.executable.content() {
            Some(id) => {
                let MaterializedContent::Blob(blob) = self.materialize_blob(id)? else {
                    return Err(internal_diag(InternalInvariant::TreeFileMaterializedAsTree));
                };
                Some(blob)
            }
            None => None,
        };
        Ok(ActionInvocation {
            spec: spec.clone(),
            inputs: inputs.into_boxed_slice(),
            program,
            deadline: None,
        })
    }

    fn materialize_blob(&self, id: ContentId) -> PithResult<MaterializedContent> {
        match self.store.get_blob(id).map_err(store_error_diag)? {
            Some(blob) => Ok(MaterializedContent::Blob(MaterializedBlob {
                id,
                bytes: blob.as_bytes().to_vec().into_boxed_slice(),
            })),
            None => Err(content_unavailable_diag(id)),
        }
    }

    fn materialize_tree(&self, id: ContentId) -> PithResult<MaterializedTree> {
        let tree = match self.store.get_tree(id).map_err(store_error_diag)? {
            Some(tree) => tree,
            None => return Err(content_unavailable_diag(id)),
        };
        let mut entries = Vec::with_capacity(tree.entries().len());
        for entry in tree.entries() {
            let content = match entry.content() {
                TreeEntryContent::File(pith_store::FileContent {
                    content,
                    executable,
                }) => {
                    let MaterializedContent::Blob(materialized) =
                        self.materialize_blob(*content)?
                    else {
                        return Err(internal_diag(InternalInvariant::TreeFileMaterializedAsTree));
                    };
                    MaterializedTreeEntryContent::File(MaterializedFileContent {
                        content: *content,
                        executable: *executable,
                        bytes: materialized.bytes,
                    })
                }
                TreeEntryContent::Tree(child) => {
                    MaterializedTreeEntryContent::Tree(self.materialize_tree(*child)?)
                }
                TreeEntryContent::Symlink { target } => MaterializedTreeEntryContent::Symlink {
                    target: target.clone(),
                },
            };
            entries.push(
                TreeEntry::new(entry.name(), content)
                    .map_err(|_| internal_diag(InternalInvariant::TreeFileMaterializedAsTree))?,
            );
        }
        Ok(MaterializedTree {
            id,
            entries: entries.into_boxed_slice(),
        })
    }

    pub(super) fn import_execution(
        &mut self,
        report: &crate::CapturedExecutionReport,
        exit: Option<crate::ActionExit>,
    ) -> PithResult<ActionExecution> {
        let mut outputs = Vec::with_capacity(report.outputs.len());
        for output in &report.outputs {
            let content = self.import_output(&output.content)?;
            outputs.push(ProducedOutput {
                path: output.path.clone(),
                content,
            });
        }
        Ok(ActionExecution {
            report: ExecutionReport {
                executor: report.executor.clone(),
                platform: report.platform.clone(),
                access: report.access,
                outputs: outputs.into_boxed_slice(),
                capabilities_used: report.capabilities_used.clone(),
            },
            exit,
        })
    }

    fn import_output(
        &mut self,
        content: &CapturedOutputContent,
    ) -> PithResult<Content<ContentId, ContentId>> {
        match content {
            Content::Blob(bytes) => Ok(Content::Blob(
                self.store.put_blob(bytes).map_err(store_error_diag)?,
            )),
            Content::Tree(tree) => Ok(Content::Tree(self.import_tree(tree)?)),
        }
    }

    fn import_tree(&mut self, tree: &crate::action::CapturedTree) -> PithResult<ContentId> {
        let mut entries = Vec::with_capacity(tree.entries.len());
        for entry in tree.entries.iter() {
            let content = match entry.content() {
                CapturedTreeEntryContent::File(CapturedFileContent { bytes, executable }) => {
                    let content = self.store.put_blob(bytes).map_err(store_error_diag)?;
                    TreeEntryContent::File(pith_store::FileContent {
                        content,
                        executable: *executable,
                    })
                }
                CapturedTreeEntryContent::Tree(tree) => {
                    TreeEntryContent::Tree(self.import_tree(tree)?)
                }
                CapturedTreeEntryContent::Symlink { target } => TreeEntryContent::Symlink {
                    target: target.clone(),
                },
            };
            entries.push(
                TreeEntry::new(entry.name(), content)
                    .map_err(|_| internal_diag(InternalInvariant::TreeFileMaterializedAsTree))?,
            );
        }
        let tree = Tree::new(entries).map_err(store_error_diag)?;
        self.store.put_tree(tree).map_err(store_error_diag)
    }
}
