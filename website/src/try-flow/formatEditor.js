/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

import type {Formatter} from './formatCode';

export function createFormatterLoader(
  load: () => Promise<{readonly formatCode: Formatter}> = () =>
    import('./formatCode'),
): () => Promise<Formatter> {
  let pending: ?Promise<Formatter> = null;
  return () => {
    if (pending == null) {
      pending = load()
        .then(module => module.formatCode)
        .catch(error => {
          pending = null;
          throw error;
        });
    }
    return pending;
  };
}

const loadFormatter = createFormatterLoader();

export async function preloadFormatter(
  load: () => Promise<Formatter> = loadFormatter,
): Promise<void> {
  try {
    await load();
  } catch {
    // Preloading is best-effort; a click retries and reports any load failure.
  }
}

export async function formatEditor(
  editor: any,
  isCurrent: () => boolean,
  load: () => Promise<Formatter> = loadFormatter,
  onReady: () => void = () => {},
): Promise<'formatted' | 'unchanged' | 'stale'> {
  const model = editor.getModel();
  if (model == null || model.getValue().trim() === '') return 'unchanged';
  const version = model.getVersionId();
  const code = model.getValue();
  const position = editor.getPosition();
  const cursorOffset = position == null ? 0 : model.getOffsetAt(position);
  const scroll = {
    scrollTop: editor.getScrollTop(),
    scrollLeft: editor.getScrollLeft(),
  };
  const format = await load();
  onReady();
  const result = await format(code, cursorOffset);
  if (
    !isCurrent() ||
    model.isDisposed() ||
    editor.getModel() !== model ||
    model.getVersionId() !== version
  ) {
    return 'stale';
  }
  if (result.formatted === code) return 'unchanged';

  editor.pushUndoStop();
  if (
    !editor.executeEdits('prettier', [
      {range: model.getFullModelRange(), text: result.formatted},
    ])
  ) {
    throw new Error('Unable to apply formatted code.');
  }
  editor.setPosition(model.getPositionAt(result.cursorOffset));
  editor.pushUndoStop();
  editor.setScrollPosition(scroll);
  editor.focus();
  return 'formatted';
}
