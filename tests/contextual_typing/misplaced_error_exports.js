export type CommitTemplate<TCommitState> = {
  title: string | ((state: TCommitState) => string),
  summary: string | ((state: TCommitState) => string),
  tags?: ?ReadonlyArray<string> | ((state: TCommitState) => ReadonlyArray<string>),
};

export function wrapCommitTemplateWithState<
  TCommitStateEntry,
  TCommitStateResult,
>(
  commitTemplate: CommitTemplate<{...TCommitStateResult, ...TCommitStateEntry}>,
  commitState: TCommitStateEntry,
): CommitTemplate<TCommitStateResult> {throw ''}
