/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

import type {CollapsedFieldProps} from 'react-json-view';

import * as React from 'react';
import {useMemo, useState, type MixedElement} from 'react';
import ReactJson from 'react-json-view';
import clsx from 'clsx';
import styles from './TryFlow.module.css';
import copyText from './clipboard';
import formatAstForDisplay from './formatAstForDisplay';

const ERROR_FORMATS: ReadonlyArray<{
  value: 'default' | 'json',
  label: string,
}> = [
  {value: 'default', label: 'Rich'},
  {value: 'json', label: 'JSON'},
];

component CheckIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true">
      <polyline points="20 6 9 17 4 12" />
    </svg>
  );
}

component AstIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true">
      <circle cx="12" cy="5" r="2" />
      <circle cx="6" cy="19" r="2" />
      <circle cx="18" cy="19" r="2" />
      <path d="M12 7v3M12 10H6v7M12 10h6v7" />
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

// Chevron points down when the pane is open; CSS rotates it 180° (to point up)
// when collapsed.
component ChevronIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true">
      <polyline points="6 9 12 15 18 9" />
    </svg>
  );
}

// Plain-text rendering of all errors, for the "Copy all" button.
function errorsToText(errors: ReadonlyArray<FlowJsError>): string {
  return errors
    .map(error => {
      const loc = error.message[0]?.loc;
      const prefix =
        loc != null ? `${loc.start.line}:${loc.start.column} ` : '';
      return prefix + error.message.map(m => m.descr).join(' ');
    })
    .join('\n\n');
}

component ErrorMessage(msg: FlowJsErrorMessage) {
  if (msg.loc && msg.context != null) {
    const basename = msg.loc.source.replace(/.*\//, '');
    const filename = basename !== '-' ? `${msg.loc.source}:` : '';
    const prefix = `${filename}${msg.loc.start.line}: `;

    const before = msg.context.slice(0, msg.loc.start.column - 1);
    const highlight =
      msg.loc.start.line === msg.loc.end.line
        ? msg.context.slice(msg.loc.start.column - 1, msg.loc.end.column)
        : msg.context.slice(msg.loc.start.column - 1);
    const after =
      msg.loc.start.line === msg.loc.end.line
        ? msg.context.slice(msg.loc.end.column)
        : '';

    const offset = msg.loc.start.column + prefix.length - 1;
    const arrow = `${(prefix + before).replace(/[^ ]/g, ' ')}^ `;

    return (
      <>
        <div>
          {prefix + before}
          <strong className={styles.msgHighlight}>{highlight}</strong>
          {after}
        </div>
        {arrow}
        <span className={styles.msgType}>{msg.descr}</span>
      </>
    );
    // $FlowFixMe[invalid-compare]
  } else if (msg.type === 'Comment') {
    return `. ${msg.descr}\n`;
  } else {
    return `${msg.descr}\n`;
  }
}

component ErrorMessageExtra(info: FlowJsErrorMessageInformation) {
  return (
    <ul>
      <li>
        {info.message &&
          info.message.map((msg, i) => <ErrorMessage key={i} msg={msg} />)}
      </li>
      <li>
        {info.children &&
          info.children.map((info, i) => (
            <ErrorMessageExtra key={i} info={info} />
          ))}
      </li>
    </ul>
  );
}

type Position = {line: number, column: number};

function comparePosition(a: Position, b: Position): number {
  if (a.line > b.line) {
    return 1;
  } else if (a.line < b.line) {
    return -1;
  } else if (a.column > b.column) {
    return 1;
  } else if (a.column < b.column) {
    return -1;
  } else {
    return 0;
  }
}

function shouldCollapse(cursorPosition: Position, json: unknown): boolean {
  if (
    typeof json === 'object' &&
    json != null &&
    typeof json.loc === 'object'
  ) {
    const loc: {start: Position, end: Position} = json.loc as any;
    if (
      comparePosition(loc.start, cursorPosition) <= 0 &&
      comparePosition(cursorPosition, loc.end) <= 0
    ) {
      return false;
    }
  }
  if (Array.isArray(json)) {
    return json.every(child => shouldCollapse(cursorPosition, child));
  }
  return true;
}

export component AstResult(
  ast: interface {} | string,
  format: 'tree' | 'json',
  hideLocations: boolean,
  hideEmptyFields: boolean,
  cursorPosition: ?Position,
) {
  const displayAst = useMemo(
    () => formatAstForDisplay(ast, hideLocations, hideEmptyFields),
    [ast, hideLocations, hideEmptyFields],
  );
  if (typeof ast === 'string') {
    return <pre className={styles.resultBody}>{ast}</pre>;
  }
  if (Object.keys(ast).length === 0) {
    return <div className={styles.emptyState}>No AST to show.</div>;
  }
  if (format === 'json') {
    return (
      <pre className={styles.resultBody}>
        {JSON.stringify(displayAst, null, 2)}
      </pre>
    );
  }
  return (
    <div className={styles.astBody}>
      <ReactJson
        src={displayAst}
        indentWidth={2}
        quotesOnKeys={false}
        displayDataTypes={false}
        enableClipboard={false}
        shouldCollapse={field => {
          if (cursorPosition == null) {
            return false;
          }
          const original = field.namespace
            .slice(1)
            .reduce<unknown>(
              (value, key) =>
                value != null && typeof value === 'object' && key != null
                  ? value[key]
                  : null,
              ast,
            );
          return shouldCollapse(cursorPosition, original);
        }}
      />
    </div>
  );
}

export default component TryFlowResults(
  flowVersion: string,
  flowVersions: ReadonlyArray<string>,
  changeFlowVersion: (version: string) => void,
  loading: boolean,
  errors: ReadonlyArray<FlowJsError>,
  internalError: string,
  cursorPosition: ?Position,
  ast: interface {} | string,
  onErrorSelect: (line: number, column: number) => void,
) {
  const [activeToolbarTab, setActiveToolbarTab] = useState('errors');
  const [errorFormat, setErrorFormat] = useState<'default' | 'json'>('default');
  const [astFormat, setAstFormat] = useState<'tree' | 'json'>('tree');
  const [hideLocations, setHideLocations] = useState(true);
  const [hideEmptyFields, setHideEmptyFields] = useState(true);
  // On narrow screens the panes stack vertically and the results pane can be
  // collapsed down to just its tab bar, giving the editor more room. Clicking
  // the active tab collapses it; clicking anywhere on the tab bar expands it.
  // Collapsing is a no-op in the side-by-side desktop layout.
  const [collapsed, setCollapsed] = useState(false);
  const [copiedAll, setCopiedAll] = useState(false);
  // Index of the error card whose "Copy" button is currently showing "Copied!".
  const [copiedError, setCopiedError] = useState(null as ?number);
  const [copiedAst, setCopiedAst] = useState(false);
  function copyAst() {
    const text = JSON.stringify(
      formatAstForDisplay(ast, hideLocations, hideEmptyFields),
      null,
      2,
    );
    if (text == null) return;
    copyText(text).then(ok => {
      if (!ok) return;
      setCopiedAst(true);
      setTimeout(() => setCopiedAst(false), 1500);
    });
  }
  function copyAll() {
    // Only show the "Copied!" confirmation if the write actually succeeds.
    // copyText falls back to execCommand where the async Clipboard API is
    // unavailable or blocked (insecure context, unfocused doc, embedded iframe).
    const text =
      errorFormat === 'json'
        ? JSON.stringify(errors, null, 2)
        : errorsToText(errors);
    copyText(text).then(ok => {
      if (!ok) return;
      setCopiedAll(true);
      setTimeout(() => setCopiedAll(false), 1500);
    });
  }
  function copyError(index: number, text: string) {
    copyText(text).then(ok => {
      if (!ok) return;
      setCopiedError(index);
      setTimeout(
        () => setCopiedError(current => (current === index ? null : current)),
        1500,
      );
    });
  }
  // The collapse interaction only applies to the stacked (narrow-screen) layout.
  function isStacked(): boolean {
    // Keep this breakpoint in sync with the stacking @media query in
    // TryFlow.module.css.
    return (
      typeof window !== 'undefined' &&
      window.matchMedia('(max-width: 850px)').matches
    );
  }
  // Tab click handler. Callers stopPropagation so a click here doesn't also hit
  // the tab bar's expand handler below.
  function selectTab(tab: string) {
    if (collapsed) {
      // Collapsed: any tab click both selects that tab and re-expands.
      setActiveToolbarTab(tab);
      setCollapsed(false);
    } else if (activeToolbarTab === tab) {
      // Clicking the already-active tab collapses (stacked layout only).
      if (isStacked()) {
        setCollapsed(true);
      }
    } else {
      setActiveToolbarTab(tab);
    }
  }

  const latestReleaseVersion = flowVersions.find(
    version => version !== 'master',
  );
  const isOldFlowVersion =
    flowVersion !== 'master' && flowVersion !== latestReleaseVersion;

  return (
    <div className={clsx(styles.results, collapsed && styles.collapsed)}>
      {/* Clicking the tab bar (anywhere not handled by a tab button) re-expands a
          collapsed results pane. */}
      <div
        className={clsx(
          styles.toolbar,
          activeToolbarTab === 'errors' && styles.errorsToolbar,
          activeToolbarTab === 'ast' && styles.astToolbar,
        )}
        onClick={() => {
          if (collapsed) {
            setCollapsed(false);
          }
        }}>
        <div className={styles.tabs}>
          <button
            type="button"
            className={clsx(
              styles.tab,
              activeToolbarTab === 'errors' && styles.selectedTab,
            )}
            aria-pressed={activeToolbarTab === 'errors'}
            onClick={e => {
              e.stopPropagation();
              selectTab('errors');
            }}>
            <span className={styles.errorIndicator}>
              {errors.length === 0 ? (
                <CheckIcon />
              ) : (
                <span className={styles.tabBadge}>{errors.length}</span>
              )}
            </span>
            Errors
          </button>
          <button
            type="button"
            className={clsx(
              styles.tab,
              activeToolbarTab === 'ast' && styles.selectedTab,
            )}
            aria-pressed={activeToolbarTab === 'ast'}
            onClick={e => {
              e.stopPropagation();
              selectTab('ast');
            }}>
            <AstIcon />
            AST
          </button>
        </div>
        <div className={styles.resultsActions}>
          {activeToolbarTab === 'errors' && errors.length > 0 && (
            <button
              className={clsx(
                styles.toolbarButton,
                styles.iconOnly,
                styles.copyErrors,
              )}
              onClick={copyAll}
              title="Copy all errors">
              <CopyIcon />
              <span className={styles.toolbarButtonLabel}>
                {copiedAll ? 'Copied!' : 'Copy errors'}
              </span>
            </button>
          )}
          {activeToolbarTab === 'errors' && (
            <div
              role="group"
              aria-label="Error format"
              className={styles.formatPills}>
              {ERROR_FORMATS.map(({value, label}) => (
                <button
                  key={value}
                  type="button"
                  className={clsx(
                    styles.formatPill,
                    errorFormat === value && styles.selectedTab,
                  )}
                  aria-pressed={errorFormat === value}
                  onClick={() => {
                    setErrorFormat(value);
                    setCopiedAll(false);
                    setCollapsed(false);
                  }}>
                  {label}
                </button>
              ))}
            </div>
          )}
          {activeToolbarTab === 'ast' && astFormat === 'json' && (
            <button
              type="button"
              className={clsx(styles.toolbarButton, styles.iconOnly)}
              onClick={copyAst}
              disabled={
                loading ||
                typeof ast === 'string' ||
                Object.keys(ast).length === 0
              }
              title="Copy JSON">
              <CopyIcon />
              <span className={styles.toolbarButtonLabel}>
                {copiedAst ? 'Copied!' : 'Copy JSON'}
              </span>
            </button>
          )}
          {activeToolbarTab === 'ast' && (
            <div className={styles.astOptions}>
              <label className={styles.astOption}>
                <input
                  type="checkbox"
                  aria-label="Hide locations"
                  checked={hideLocations}
                  onChange={event =>
                    setHideLocations(event.currentTarget.checked)
                  }
                />
                <span>
                  Hide loc<span className={styles.labelExtra}>ations</span>
                </span>
              </label>
              <label className={styles.astOption}>
                <input
                  type="checkbox"
                  aria-label="Hide empty fields"
                  checked={hideEmptyFields}
                  onChange={event =>
                    setHideEmptyFields(event.currentTarget.checked)
                  }
                />
                <span>
                  Hide empty<span className={styles.labelExtra}> fields</span>
                </span>
              </label>
            </div>
          )}
          {activeToolbarTab === 'ast' && (
            <div
              role="group"
              aria-label="AST format"
              className={styles.formatPills}>
              {(['tree', 'json'] as const).map(value => (
                <button
                  key={value}
                  type="button"
                  className={clsx(
                    styles.formatPill,
                    astFormat === value && styles.selectedTab,
                  )}
                  aria-pressed={astFormat === value}
                  onClick={() => {
                    setAstFormat(value);
                    setCollapsed(false);
                  }}>
                  {value === 'tree' ? 'Tree' : 'JSON'}
                </button>
              ))}
            </div>
          )}
          {isOldFlowVersion ? (
            <span className={styles.versionWarning}>old version selected</span>
          ) : null}
          <select
            className={styles.flowVersion}
            aria-label="Flow version"
            value={flowVersion}
            onChange={event => changeFlowVersion(event.currentTarget.value)}>
            {flowVersions.map(version => (
              <option key={version} value={version}>
                {version.replace('master', 'main')}
              </option>
            ))}
          </select>
          {/* Collapse toggle, shown only in the stacked layout (CSS). Makes the
              tap-to-collapse gesture discoverable: chevron points down when the
              pane is open, up when collapsed. */}
          <button
            type="button"
            className={styles.collapseChevron}
            aria-label={collapsed ? 'Expand results' : 'Collapse results'}
            aria-expanded={!collapsed}
            onClick={e => {
              e.stopPropagation();
              setCollapsed(c => !c);
            }}>
            <ChevronIcon />
          </button>
        </div>
      </div>
      {loading && (
        <div className={styles.loader}>
          <div className={styles.spinner} />
        </div>
      )}
      {!loading &&
        activeToolbarTab === 'errors' &&
        (internalError ? (
          <pre className={clsx(styles.resultBody, styles.errors)}>
            <ul>
              <li>TryFlow encountered an internal error: {internalError}</li>
            </ul>
          </pre>
        ) : errorFormat === 'json' ? (
          errors.length === 0 ? (
            <div className={styles.emptyState}>No errors to show as JSON.</div>
          ) : (
            <pre className={styles.resultBody}>
              {JSON.stringify(errors, null, 2)}
            </pre>
          )
        ) : errors.length === 0 ? (
          <div className={styles.noErrors}>
            <span className={styles.noErrorsCheck}>
              <CheckIcon />
            </span>
            No errors
          </div>
        ) : (
          <pre className={clsx(styles.resultBody, styles.errors)}>
            <ul>
              {errors.map((error, i) => {
                const code = Array.isArray(error.error_codes)
                  ? error.error_codes[0]
                  : null;
                const loc = error.message[0]?.loc;
                const text = error.message.map(m => m.descr).join(' ');
                return (
                  <li key={i}>
                    {/* A stretched overlay button turns the whole card into one
                        keyboard-focusable jump target. Keeping it a sibling of
                        the copy button (rather than wrapping the card) avoids
                        nesting interactive elements. */}
                    {loc != null && (
                      <button
                        type="button"
                        className={styles.errorCardJump}
                        aria-label={`Go to error at line ${loc.start.line}, column ${loc.start.column}`}
                        onClick={() =>
                          onErrorSelect(loc.start.line, loc.start.column)
                        }
                      />
                    )}
                    <div className={styles.errorCardHeader}>
                      {code != null && (
                        <span className={styles.errorCode}>{code}</span>
                      )}
                      <button
                        type="button"
                        className={clsx(
                          styles.errorCopy,
                          copiedError === i && styles.errorCopied,
                        )}
                        title="Copy this error"
                        onClick={() => copyError(i, text)}>
                        {copiedError === i ? 'Copied!' : 'Copy'}
                      </button>
                    </div>
                    {error.message.map((msg, j) => (
                      <ErrorMessage key={j} msg={msg} />
                    ))}
                    {error.extra &&
                      error.extra.map((info, j) => (
                        <ErrorMessageExtra key={j} info={info} />
                      ))}
                  </li>
                );
              })}
            </ul>
          </pre>
        ))}
      {!loading && activeToolbarTab === 'ast' && (
        <AstResult
          ast={ast}
          format={astFormat}
          hideLocations={hideLocations}
          hideEmptyFields={hideEmptyFields}
          cursorPosition={cursorPosition}
        />
      )}
    </div>
  );
}
