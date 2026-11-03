/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

import React, {useState, useEffect, useRef, type MixedElement} from 'react';
import {useBaseUrlUtils} from '@docusaurus/useBaseUrl';
import clsx from 'clsx';
import Editor from '@monaco-editor/react';
import * as LZString from 'lz-string';
import styles from './TryFlow.module.css';
import TryFlowConfigEditor from './TryFlowConfigEditor';
import TryFlowResults from './TryFlowResults';
import {
  monaco,
  setAutoCompleteFunction,
  setGetDefFunction,
  setTypeAtPosFunction,
  setSignatureHelpFunction,
} from './configured-monaco';
import FlowJsServices from './flow-services';
import copyText from './clipboard';
import {formatEditor, preloadFormatter} from './formatEditor';
import createTokensProvider from './tokens-theme-provider';
import flowLanguageConfiguration from './flow-configuration.json';

const TRY_FLOW_LAST_CONTENT_STORAGE_KEY = 'tryFlowLastContent';
const DEFAULT_FLOW_PROGRAM = `type User = {
  readonly name: string,
  readonly age: number,
};

function get<K extends keyof User>(user: User, key: K): User[K] {
  return user[key];
}

declare const user: User;
const age: number = get(user, 'name'); // Fix - update to: get(user, 'age')
`;

type InitialStateFromHash = {
  code: string,
  config: ?{[string]: unknown},
  version: ?string,
};

function getHashedValue(hash: ?string): ?InitialStateFromHash {
  if (hash == null) return null;
  if (hash[0] !== '#' || hash.length < 2) return null;
  const version = hash.slice(1, 2);
  const encoded = hash.slice(2);
  if (version === '0' && encoded.match(/^[a-zA-Z0-9+/=_-]+$/)) {
    return {
      code: LZString.decompressFromEncodedURIComponent(encoded),
      config: null,
      version: null,
    };
  }
  if (version === '1' && encoded.match(/^[a-zA-Z0-9+/=_-]+$/)) {
    try {
      const {code, config, version} = JSON.parse(
        LZString.decompressFromEncodedURIComponent(encoded),
      );
      if (
        typeof code === 'string' &&
        (config == null || typeof config === 'object') &&
        (version == null || typeof version === 'string')
      ) {
        return {code, config, version};
      }
    } catch {}
  }
  return null;
}

function setHashedValue(
  flowService: ?FlowJsServices,
  version: string,
  code: string,
) {
  const compressed = LZString.compressToEncodedURIComponent(
    JSON.stringify({config: flowService?.config, code, version}),
  );
  window.location.hash = `1${compressed}`;
  localStorage.setItem(TRY_FLOW_LAST_CONTENT_STORAGE_KEY, location.hash);
}

// The saved-state recover prompt is only worth offering when there's actual
// content to recover. An empty or whitespace-only program isn't, so treat it as
// if nothing were saved.
function getRecoverableStateFromStorage(): ?InitialStateFromHash {
  const state = getHashedValue(
    localStorage.getItem(TRY_FLOW_LAST_CONTENT_STORAGE_KEY),
  );
  if (state == null || (state.code ?? '').replace(/\s/g, '') === '') {
    return null;
  }
  return state;
}

const initialStateFromURI = getHashedValue(location.hash);
const initialState: InitialStateFromHash = initialStateFromURI || {
  // Only default to an example if we haven't used Try Flow before
  code:
    localStorage.getItem(TRY_FLOW_LAST_CONTENT_STORAGE_KEY) != null
      ? ''
      : DEFAULT_FLOW_PROGRAM,
  version: null,
  config: null,
};

const REFINED_VALUE_DECORATION_OPTIONS = {
  inlineClassName: styles.refinedValueDecoration,
};

// Minimum widths (px) each pane keeps while dragging the splitter in the
// side-by-side layout. The stacked-layout breakpoint (TryFlow.module.css) sits
// above their sum, so there's always room for both before we stack.
const MIN_EDITOR_WIDTH = 400;
const MIN_RESULTS_WIDTH = 450;

// Clamp a raw split ratio so neither pane drops below its minimum width at the
// current container width.
function clampSplitRatio(ratio: number, containerWidth: number): number {
  if (containerWidth <= 0) return ratio;
  const min = MIN_EDITOR_WIDTH / containerWidth;
  const max = 1 - MIN_RESULTS_WIDTH / containerWidth;
  if (min >= max) return min;
  return Math.min(max, Math.max(min, ratio));
}

component CodeIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true">
      <polyline points="16 18 22 12 16 6" />
      <polyline points="8 6 2 12 8 18" />
    </svg>
  );
}

component ConfigIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true">
      <line x1="21" x2="14" y1="4" y2="4" />
      <line x1="10" x2="3" y1="4" y2="4" />
      <line x1="21" x2="12" y1="12" y2="12" />
      <line x1="8" x2="3" y1="12" y2="12" />
      <line x1="21" x2="16" y1="20" y2="20" />
      <line x1="12" x2="3" y1="20" y2="20" />
      <line x1="14" x2="14" y1="2" y2="6" />
      <line x1="8" x2="8" y1="10" y2="14" />
      <line x1="16" x2="16" y1="18" y2="22" />
    </svg>
  );
}

component CopyLinkIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true">
      <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
      <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
    </svg>
  );
}

component CopyIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true">
      <rect width="14" height="14" x="8" y="8" rx="2" ry="2" />
      <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
    </svg>
  );
}

component FormatIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true">
      <path d="M4 6h16M8 10h12M8 14h12M4 18h16" />
      <path d="m2 10 2 2-2 2" />
    </svg>
  );
}

component ClearIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true">
      <polyline points="3 6 5 6 21 6" />
      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
      <line x1="10" x2="10" y1="11" y2="17" />
      <line x1="14" x2="14" y1="11" y2="17" />
    </svg>
  );
}

component RestoreIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true">
      <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
      <path d="M3 3v5h5" />
    </svg>
  );
}

component DismissIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true">
      <path d="M18 6 6 18" />
      <path d="m6 6 12 12" />
    </svg>
  );
}

export default component TryFlow(
  defaultFlowVersion: string,
  flowVersions: ReadonlyArray<string>,
) {
  const {withBaseUrl} = useBaseUrlUtils();
  const [initialStateFromStorage, setInitialStateFromStorage] = useState(
    initialStateFromURI == null ? getRecoverableStateFromStorage() : null,
  );
  const [flowVersion, setFlowVersion] = useState(
    initialState.version || defaultFlowVersion,
  );
  const editorRef = useRef<any>(null);
  const tryEditorRef = useRef<HTMLDivElement | null>(null);
  const [splitRatio, setSplitRatio] = useState(0.5);
  const previousDecorationsRef = useRef(null);
  const [errors, setErrors] = useState<ReadonlyArray<FlowJsError>>([]);
  const [internalError, setInternalError] = useState('');
  const [cursorPosition, setCursorPosition] =
    useState<?{lineNumber: number, column: number}>(null);
  const [ast, setAST] = useState<interface {} | string>({});
  const [loading, setLoading] = useState(true);
  const [flowService, setFlowService] = useState(null as ?FlowJsServices);
  const [activeToolbarTab, setActiveToolbarTab] = useState(
    'code' as 'code' | 'config',
  );
  const [copied, setCopied] = useState(null as ?('link' | 'code'));
  const [editorReady, setEditorReady] = useState(false);
  const [hasCode, setHasCode] = useState(
    (initialState.code ?? '').trim() !== '',
  );
  const [formatting, setFormatting] = useState(false);
  const [formatLabel, setFormatLabel] = useState('Format');
  const [formatNotice, setFormatNotice] = useState('');
  const formattingRef = useRef(false);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    FlowJsServices.init(withBaseUrl, flowVersion)
      .then(f => {
        if (cancelled) {
          return;
        }
        setFlowService(existing =>
          existing == null
            ? // Only the initial init will use the config encoded in the starting URI
              f.withUpdatedConfig(initialState.config)
            : f,
        );
        setLoading(false);
        setInternalError('');
      })
      .catch(e => {
        if (cancelled) {
          return;
        }
        setLoading(false);
        setInternalError(e instanceof Error ? e.message : JSON.stringify(e));
      });
    return () => {
      cancelled = true;
    };
  }, [flowVersion]);

  useEffect(() => {
    if (
      formatNotice !== 'Code formatted.' &&
      formatNotice !== 'Code is already formatted.'
    ) {
      return;
    }
    const timer = setTimeout(() => {
      setFormatLabel('Format');
      setFormatNotice('');
    }, 2000);
    return () => clearTimeout(timer);
  }, [formatNotice]);

  useEffect(() => {
    forceRecheck();
  }, [flowService]);

  function resetFromStorage() {
    const model = monaco.editor.getModels()[0];
    if (initialStateFromStorage != null && model != null) {
      setInitialStateFromStorage(null);
      setFlowVersion(initialStateFromStorage.version || defaultFlowVersion);
      model.setValue(initialStateFromStorage.code);
      setFlowService(
        existing =>
          existing?.withUpdatedConfig(initialStateFromStorage.config) ??
          existing,
      );
    }
  }

  // Clear the editor, stashing the current program as the recoverable state so
  // the recover/ignore prompt appears (the same prompt as on reload). Whitespace-
  // only content isn't worth offering to recover, so in that case we just empty
  // the editor without arming the prompt.
  function clearEditor() {
    const model = monaco.editor.getModels()[0];
    if (model == null) return;
    const code = model.getValue();
    if (code.replace(/\s/g, '') !== '') {
      setInitialStateFromStorage({
        code,
        config: flowService?.config ?? null,
        version: flowVersion,
      });
    }
    model.setValue('');
  }

  async function formatCurrentCode() {
    if (formattingRef.current || activeToolbarTab !== 'code') return;
    const editor = editorRef.current;
    if (editor == null || editor.getModel() == null) return;
    const isCurrent = () => mountedRef.current && editorRef.current === editor;
    formattingRef.current = true;
    setFormatting(true);
    setFormatLabel('Loading…');
    setFormatNotice('Loading formatter…');
    try {
      const result = await formatEditor(editor, isCurrent, undefined, () => {
        if (!isCurrent()) return;
        setFormatLabel('Formatting…');
        setFormatNotice('Formatting code…');
      });
      if (!isCurrent()) return;
      setFormatLabel(
        result === 'stale'
          ? 'Retry'
          : result === 'formatted'
            ? 'Formatted'
            : 'No change',
      );
      setFormatNotice(
        result === 'stale'
          ? 'Code changed; press Format again.'
          : result === 'formatted'
            ? 'Code formatted.'
            : 'Code is already formatted.',
      );
    } catch (error) {
      if (isCurrent()) {
        const message = error instanceof Error ? error.message : String(error);
        setFormatLabel('Failed');
        setFormatNotice(`Could not format code: ${message}`);
      }
    } finally {
      formattingRef.current = false;
      if (isCurrent()) setFormatting(false);
    }
  }

  function onCodeChange(value: ?string) {
    setHasCode((value ?? '').trim() !== '');
    if (!formattingRef.current) {
      setFormatLabel('Format');
      setFormatNotice('');
    }
    forceRecheck();
  }

  function copyToClipboard(kind: 'link' | 'code', text: string) {
    // Only show the "Copied!" confirmation if the write actually succeeds.
    // copyText falls back to execCommand where the async Clipboard API is
    // unavailable or blocked (insecure context, unfocused doc, embedded iframe).
    copyText(text).then(ok => {
      if (!ok) return;
      setCopied(kind);
      setTimeout(() => setCopied(null), 1500);
    });
  }

  function onSplitterDown(e: SyntheticMouseEvent<>) {
    e.preventDefault();
    const container = tryEditorRef.current;
    if (container == null) return;
    const body = document.body;
    const onMove = (ev: MouseEvent) => {
      const rect = container.getBoundingClientRect();
      const ratio = (ev.clientX - rect.left) / rect.width;
      setSplitRatio(clampSplitRatio(ratio, rect.width));
      // Keep the editor sized to its (now-resized) pane during the drag. rAF so
      // it reads the post-render width rather than the pre-update one.
      requestAnimationFrame(() => editorRef.current?.layout());
    };
    const onUp = () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      if (body != null) {
        body.style.cursor = '';
        body.style.userSelect = '';
      }
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    if (body != null) {
      body.style.cursor = 'col-resize';
      body.style.userSelect = 'none';
    }
  }

  // Keyboard control for the splitter: arrows nudge the ratio, Home/End jump to
  // the min/max, matching the per-pane minimum-width clamp used by the drag.
  function onSplitterKey(e: SyntheticKeyboardEvent<>) {
    const width = tryEditorRef.current?.getBoundingClientRect().width ?? 0;
    const min = width > 0 ? MIN_EDITOR_WIDTH / width : 0.2;
    const max = width > 0 ? 1 - MIN_RESULTS_WIDTH / width : 0.8;
    const step = 0.02;
    let next;
    switch (e.key) {
      case 'ArrowLeft':
        next = splitRatio - step;
        break;
      case 'ArrowRight':
        next = splitRatio + step;
        break;
      case 'Home':
        next = min;
        break;
      case 'End':
        next = max;
        break;
      default:
        return;
    }
    e.preventDefault();
    setSplitRatio(clampSplitRatio(next, width));
  }

  function revealError(line: number, column: number) {
    const editor = editorRef.current;
    if (editor == null) return;
    editor.revealLineInCenter(line);
    editor.setPosition({lineNumber: line, column});
    editor.focus();
  }

  function semanticDecorations(flowService: FlowJsServices, model: any) {
    let decorations: ReadonlyArray<{kind: 'refined-value', range: any}> = [];
    try {
      decorations =
        flowService.semanticDecorations?.('-', model.getValue())?.decorations ||
        [];
    } catch {
      // Refined value decorations are best-effort UI hints. They should not
      // interrupt typechecking or surface as Try Flow internal errors.
    }
    const refinedValueDecorations = [];
    for (const decoration of decorations) {
      switch (decoration.kind) {
        case 'refined-value':
          refinedValueDecorations.push({
            range: decoration.range,
            options: REFINED_VALUE_DECORATION_OPTIONS,
          });
          break;
      }
    }

    const editor = editorRef.current;
    if (editor == null) return;
    previousDecorationsRef.current?.clear();
    previousDecorationsRef.current = editor.createDecorationsCollection(
      refinedValueDecorations,
    );
  }

  function forceRecheck() {
    setAutoCompleteFunction(flowService);
    setGetDefFunction(flowService);
    setTypeAtPosFunction(flowService);
    setSignatureHelpFunction(flowService);

    const model = monaco.editor.getModels()[0];
    if (model == null || flowService == null) return;
    const value = model.getValue();

    // typecheck on edit
    try {
      const errors = flowService.checkContent('-', model.getValue());
      const markers = errors.map(err => {
        const messages = err.message;
        const firstLoc = messages[0].loc;
        const message = messages.map(msg => msg.descr).join('\n');
        let severity;
        switch (err.level) {
          case 'error':
            severity = monaco.MarkerSeverity.Error;
            break;
          case 'warning':
            severity = monaco.MarkerSeverity.Warning;
            break;
          default:
            severity = monaco.MarkerSeverity.Hint;
            break;
        }
        return {
          // the code is also in the message, so don't also include it here.
          // but if we fixed the message, we'd do this:
          // code: Array.isArray(err.error_codes) ? err.error_codes[0] : undefined,
          severity,
          message: message,
          source: firstLoc.source,
          startLineNumber: firstLoc.start.line,
          startColumn: firstLoc.start.column,
          endLineNumber: firstLoc.end.line,
          endColumn: firstLoc.end.column + 1,
          // TODO: show references
          // relatedInformation: ...
        };
      });
      monaco.editor.setModelMarkers(model, 'default', markers);
      semanticDecorations(flowService, model);
      setInternalError('');
      setErrors(errors);
      if (flowService?.supportsParse) {
        setAST(flowService.parseAstToJsonString(value));
      }
    } catch (e) {
      console.error(e);
      setInternalError(e instanceof Error ? e.message : JSON.stringify(e));
      setErrors([]);
      setAST({});
    }

    // update the URL
    setHashedValue(flowService, flowVersion, value);

    // Whenever the recover prompt is showing, the editor starts empty (both on
    // load from storage and right after Clear). Once the user types real content
    // into it, dismiss the prompt — they're working on something new now.
    if (initialStateFromStorage != null && value.replace(/\s/g, '') !== '') {
      setInitialStateFromStorage(null);
    }
  }

  function onMount(editor: any) {
    forceRecheck();

    editor.onDidChangeCursorPosition(e => {
      setCursorPosition(e.position);
    });
    editorRef.current = editor;
    setEditorReady(true);

    // The editor fills its pane via flexbox (height: 100%), but Monaco only
    // re-renders its canvas to a new size when layout() is called. We call it on
    // the discrete events that actually change the pane size, rather than via a
    // ResizeObserver: an observer that relayouts inside its own callback (or
    // Monaco's automaticLayout, which does the same) is what produces the
    // "ResizeObserver loop completed with undelivered notifications" warning. The
    // editor therefore snaps to its new size when the results-pane collapse
    // animation finishes, instead of animating along with it.
    const relayout = () => editorRef.current?.layout();

    // Results-pane collapse/expand animates the panes' flex sizes; the
    // transition bubbles up to the .tryEditor container when it finishes.
    tryEditorRef.current?.addEventListener('transitionend', e => {
      if (e.propertyName === 'flex-grow' || e.propertyName === 'flex-basis') {
        relayout();
      }
    });

    // Window resizes, including the responsive desktop<->stacked switch.
    // Coalesced to one relayout per frame.
    let resizeFrame: ?number = null;
    window.addEventListener('resize', () => {
      if (resizeFrame != null) cancelAnimationFrame(resizeFrame);
      resizeFrame = requestAnimationFrame(relayout);
    });

    // Re-measure once the code font has loaded so glyph widths are correct.
    // (document.fonts isn't in Flow's DOM lib defs yet.)
    (document as $FlowFixMe).fonts?.ready?.then(relayout);
  }

  return (
    <div
      className={styles.tryEditor}
      ref={tryEditorRef}
      style={{'--tf-split': splitRatio} as $FlowFixMe}>
      <div className={styles.code}>
        <div className={styles.editorContainer}>
          <div className={styles.toolbar}>
            <div className={styles.tabs}>
              <button
                type="button"
                className={clsx(
                  styles.tab,
                  activeToolbarTab === 'code' && styles.selectedTab,
                )}
                aria-pressed={activeToolbarTab === 'code'}
                onClick={() => setActiveToolbarTab('code')}>
                <CodeIcon />
                Code
              </button>
              <button
                type="button"
                className={clsx(
                  styles.tab,
                  activeToolbarTab === 'config' && styles.selectedTab,
                )}
                aria-pressed={activeToolbarTab === 'config'}
                onClick={() => setActiveToolbarTab('config')}>
                <ConfigIcon />
                Config
              </button>
            </div>
            <div className={styles.toolbarActions}>
              <button
                type="button"
                className={clsx(styles.toolbarButton, styles.iconOnly)}
                onClick={formatCurrentCode}
                onMouseEnter={() => preloadFormatter()}
                onFocus={() => preloadFormatter()}
                disabled={
                  !editorReady ||
                  !hasCode ||
                  formatting ||
                  activeToolbarTab !== 'code'
                }
                aria-label="Format code with Prettier"
                aria-describedby={
                  formatNotice ? 'try-flow-format-status' : undefined
                }
                aria-busy={formatting}
                title={formatNotice || 'Format code with Prettier'}>
                {formatting ? (
                  <span className={styles.formatSpinner} aria-hidden="true" />
                ) : formatLabel === 'Formatted' ||
                  formatLabel === 'No change' ? (
                  <span className={styles.formatStateIcon} aria-hidden="true">
                    ✓
                  </span>
                ) : formatLabel === 'Failed' || formatLabel === 'Retry' ? (
                  <span className={styles.formatStateIcon} aria-hidden="true">
                    !
                  </span>
                ) : (
                  <FormatIcon />
                )}
                <span className={styles.toolbarButtonLabel}>{formatLabel}</span>
                <span
                  id="try-flow-format-status"
                  className={styles.formatStatus}
                  role="status"
                  aria-live="polite">
                  {formatNotice}
                </span>
              </button>
              <div className={styles.swapSlot}>
                <div
                  className={clsx(
                    styles.resetBanner,
                    initialStateFromStorage
                      ? styles.swapActive
                      : styles.swapInactive,
                  )}>
                  <button
                    className={clsx(styles.toolbarButton, styles.iconOnly)}
                    onClick={resetFromStorage}
                    title="Recover last saved">
                    <RestoreIcon />
                    <span className={styles.toolbarButtonLabel}>
                      Recover
                      <span className={styles.labelExtra}> last saved</span>
                    </span>
                  </button>
                  {/* Ignore is hidden on phone — typing into the editor dismisses
                      the prompt anyway, so it's redundant where space is tight. */}
                  <button
                    className={clsx(styles.toolbarButton, styles.phoneHidden)}
                    onClick={() => setInitialStateFromStorage(null)}
                    title="Ignore saved">
                    <DismissIcon />
                    <span className={styles.toolbarButtonLabel}>
                      Ignore saved
                    </span>
                  </button>
                </div>
                <button
                  className={clsx(
                    styles.toolbarButton,
                    styles.iconOnly,
                    initialStateFromStorage
                      ? styles.swapInactive
                      : styles.swapActive,
                  )}
                  onClick={clearEditor}
                  disabled={!editorReady || !hasCode}
                  title="Clear the editor">
                  <ClearIcon />
                  <span className={styles.toolbarButtonLabel}>Clear</span>
                </button>
              </div>
              <button
                className={clsx(
                  styles.toolbarButton,
                  styles.iconOnly,
                  styles.copyCode,
                )}
                onClick={() =>
                  copyToClipboard(
                    'code',
                    monaco.editor.getModels()[0]?.getValue() ?? '',
                  )
                }
                disabled={!editorReady || !hasCode}
                title="Copy the code">
                <CopyIcon />
                <span className={styles.toolbarButtonLabel}>
                  {copied === 'code' ? 'Copied!' : 'Copy code'}
                </span>
              </button>
              <button
                className={styles.toolbarButton}
                onClick={() => copyToClipboard('link', window.location.href)}
                title="Copy a shareable link to this code">
                <CopyLinkIcon />
                <span className={styles.toolbarButtonLabel}>
                  {copied === 'link' ? 'Copied!' : 'Copy link'}
                </span>
              </button>
            </div>
          </div>
          <div
            style={{display: activeToolbarTab === 'config' ? 'block' : 'none'}}
            className={styles.tryEditorConfig}>
            <TryFlowConfigEditor
              flowService={flowService}
              setConfig={config =>
                setFlowService(flowService?.withUpdatedConfig(config))
              }
            />
          </div>
          <div className={styles.editorBody}>
            <Editor
              defaultValue={initialState.code}
              defaultLanguage="flow"
              theme="vs-light"
              height="100%"
              onChange={onCodeChange}
              onMount={onMount}
              options={{
                minimap: {enabled: false},
                hover: {enabled: true, above: false},
                scrollBeyondLastLine: false,
                overviewRulerBorder: false,
                // Match the site's code font (Monaco uses its own setting, not the
                // CSS --ifm-font-family-monospace var). Keep this stack identical
                // to that var in custom.css so the editor falls back the same way
                // the rest of the site's code does if Geist Mono fails to load.
                fontFamily:
                  "'Geist Mono Variable', ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace",
                // Breathing room between the tab bar and the first line of code.
                padding: {top: 12},
              }}
            />
          </div>
        </div>
      </div>
      <div
        className={styles.splitter}
        role="separator"
        tabIndex={0}
        aria-label="Resize editor and results panes"
        aria-orientation="vertical"
        aria-valuemin={20}
        aria-valuemax={80}
        aria-valuenow={Math.round(splitRatio * 100)}
        onMouseDown={onSplitterDown}
        onKeyDown={onSplitterKey}
      />
      <TryFlowResults
        flowVersion={flowVersion}
        flowVersions={flowVersions}
        changeFlowVersion={setFlowVersion}
        loading={loading}
        errors={errors}
        internalError={internalError}
        onErrorSelect={revealError}
        cursorPosition={
          cursorPosition != null
            ? {
                line: cursorPosition.lineNumber,
                column: cursorPosition.column - 1,
              }
            : null
        }
        ast={
          flowService?.supportsParse
            ? ast
            : 'AST output is not supported in this version of Flow.'
        }
      />
    </div>
  );
}
