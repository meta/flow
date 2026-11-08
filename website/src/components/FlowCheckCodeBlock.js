/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

import * as React from 'react';
import {
  useState,
  useEffect,
  useRef,
  useContext,
  useMemo,
  type MixedElement,
} from 'react';
import clsx from 'clsx';
import {Highlight} from 'prism-react-renderer';
import copy from 'copy-text-to-clipboard';
import * as LZString from 'lz-string';
import {useBaseUrlUtils} from '@docusaurus/useBaseUrl';
import {usePrismTheme} from '@docusaurus/theme-common';
import Translate, {translate} from '@docusaurus/Translate';
import IconCopy from '@theme/Icon/Copy';
import IconSuccess from '@theme/Icon/Success';
import styles from './FlowCheckCodeBlock.module.css';
import {useThemeConfig} from '@docusaurus/theme-common';
import {CodeThemeContext} from './landing/CodeThemeContext';
import {CodeErrorPlacementContext} from './landing/CodeErrorPlacementContext';

function IconExternalLink({
  className,
}: {
  className?: string,
}): React.MixedElement {
  return (
    <svg viewBox="0 0 24 24" className={className} aria-hidden="true">
      <path
        fill="currentColor"
        d="M14,3V5H17.59L7.76,14.83L9.17,16.24L19,6.41V10H21V3M19,19H5V5H12V3H5C3.89,3 3,3.9 3,5V19A2,2 0 0,0 5,21H19A2,2 0 0,0 21,19V12H19V19Z"
      />
    </svg>
  );
}

type FlowErrorMessage = {
  startLine: number,
  startColumn: number,
  endLine: number,
  endColumn: number,
  description: string,
};

type FlowReferenceLocation = {
  line: number,
  column: number,
};

type FlowError = {
  messages: Array<FlowErrorMessage>,
  referenceLocations: {[key: string]: FlowReferenceLocation},
  fullDescription: string,
  errorCode: string | null,
};

type HighlightRange = {
  start: number,
  end: number,
};

function formatErrorMessage(
  text: string,
  errorCode: string | null,
  referenceLocations: {[key: string]: FlowReferenceLocation},
): React.MixedElement {
  // Extract and strip URLs from the message
  const cleanText = text
    .replace(/\.\s*See \d+\.\s*in\s*https?:\/\/[^\s,)]+\.?/g, '.')
    .replace(/\.\s*See\s+https?:\/\/[^\s,)]+\.?/g, '.')
    .replace(/\s*https?:\/\/[^\s,)]+/g, '')
    .trim();

  // Split by backtick-quoted segments and [N] reference markers
  const parts = cleanText.split(/(`[^`]+`|\[\d+\])/g);

  const badge = errorCode ? (
    <span className={styles.errorCodeBadge}>{errorCode}</span>
  ) : null;

  return (
    <>
      {badge}
      <span className={styles.errorMessageText}>
        {parts.map((part, i) => {
          if (part.startsWith('`') && part.endsWith('`')) {
            return (
              <code key={i} className={styles.errorHighlightedCode}>
                {part.slice(1, -1)}
              </code>
            );
          }
          if (/^\[\d+\]$/.test(part)) {
            const referenceLocation = referenceLocations[part.slice(1, -1)];
            const title =
              referenceLocation == null
                ? undefined
                : referenceLocation.column === 0
                  ? `Line ${referenceLocation.line}`
                  : `Line ${referenceLocation.line}, column ${referenceLocation.column}`;
            return (
              <sup key={i} className={styles.errorRefMarker} title={title}>
                {part}
              </sup>
            );
          }
          return part;
        })}
      </span>
    </>
  );
}

function mergeRanges(ranges: Array<HighlightRange>): Array<HighlightRange> {
  if (ranges.length === 0) return [];
  const sorted = [...ranges].sort((a, b) => a.start - b.start);
  const merged: Array<HighlightRange> = [sorted[0]];
  for (let i = 1; i < sorted.length; i++) {
    const last = merged[merged.length - 1];
    if (sorted[i].start <= last.end) {
      last.end = Math.max(last.end, sorted[i].end);
    } else {
      merged.push(sorted[i]);
    }
  }
  return merged;
}

function getHighlightRangesOnLine(
  errors: Array<FlowError>,
  line: number,
  lineWidth: number,
): Array<HighlightRange> {
  const ranges: Array<HighlightRange> = [];
  for (const error of errors) {
    for (const msg of error.messages) {
      if (!(msg.startLine <= line && line <= msg.endLine)) continue;
      const start = msg.startLine < line ? 0 : msg.startColumn - 1;
      const end = line < msg.endLine ? lineWidth : msg.endColumn;
      ranges.push({start, end});
    }
  }
  return mergeRanges(ranges);
}

type FlowMeta = {
  errors: Array<FlowError>,
  options?: {firstErrorOnly?: boolean},
};

export default component FlowCheckCodeBlock(
  children: string,
  metastring: string,
) {
  const parsedMeta: FlowMeta = JSON.parse(
    metastring || '{"errors":[],"options":{}}',
  );
  const allFlowErrors: Array<FlowError> = parsedMeta.errors;
  const flowErrors: Array<FlowError> =
    parsedMeta.options?.firstErrorOnly === true
      ? allFlowErrors.slice(0, 1)
      : allFlowErrors;
  const {prism} = useThemeConfig();
  const [showCopied, setShowCopied] = useState(false);
  const [mounted, setMounted] = useState(false);
  // The Prism theme on SSR is always the default theme but the site theme
  // can be in a different mode. React hydration doesn't update DOM styles
  // that come from SSR. Hence force a re-render after mounting to apply the
  // current relevant styles. There will be a flash seen of the original
  // styles seen using this current approach but that's probably ok. Fixing
  // the flash will require changing the theming approach and is not worth it
  // at this point.

  useEffect(() => {
    setMounted(true);
  }, []);

  const button = useRef<React.ElementRef<'button'> | null>(null);
  const {withBaseUrl} = useBaseUrlUtils();
  const defaultPrismTheme = usePrismTheme();
  const overrideTheme = useContext(CodeThemeContext);
  const prismTheme = overrideTheme ?? defaultPrismTheme;
  const errorPlacement = useContext(CodeErrorPlacementContext);
  const showInlineErrors = errorPlacement !== 'end';
  // In case interleaved Markdown (e.g. when using CodeBlock as standalone component).

  const content = Array.isArray(children) ? children.join('') : children;

  const language = 'tsx';
  const code = content.replace(/\n$/, '');
  const lineWidths = code.split('\n').map(line => line.length);

  // Compute which lines have error indicators (based on primary error location)
  const errorsByPrimaryLine: Map<
    number,
    Array<{error: FlowError, errorIndex: number}>,
  > = new Map();
  flowErrors.forEach((error, errorIndex) => {
    const primaryMsg = error.messages[0];
    if (primaryMsg) {
      const line = primaryMsg.startLine;
      if (!errorsByPrimaryLine.has(line)) {
        errorsByPrimaryLine.set(line, []);
      }
      errorsByPrimaryLine.get(line)?.push({error, errorIndex});
    }
  });

  const handleCopyCode = () => {
    copy(code);
    setShowCopied(true);
    setTimeout(() => setShowCopied(false), 2000);
  };

  // Load the example into Try Flow via its URL hash. Format `0` carries the
  // code alone (see getHashedValue in try-flow/TryFlow.js); flow-check blocks
  // have no per-example config or version, so code-only round-trips exactly.
  // compressToEncodedURIComponent('') is '', which getHashedValue rejects, so
  // an empty block has no valid Try URL and we hide the button below.
  const tryFlowUrl = useMemo(
    () =>
      code === ''
        ? null
        : withBaseUrl('/try/') +
          '#0' +
          LZString.compressToEncodedURIComponent(code),
    [code, withBaseUrl],
  );

  return (
    <Highlight
      key={String(mounted)}
      theme={prismTheme}
      code={code}
      language={language}>
      {({className, style, tokens, getLineProps, getTokenProps}) => (
        <div className={styles.codeBlockContainer}>
          <div className={clsx(styles.codeBlockContent, language)}>
            <pre
              /* eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex */
              tabIndex={0}
              className={clsx(className, styles.codeBlock, 'thin-scrollbar')}
              style={style}>
              <code className={styles.codeBlockLines}>
                {tokens.map((line, i) => {
                  if (line.length === 1 && line[0].content === '') {
                    line[0].content = '\n'; // eslint-disable-line no-param-reassign
                  }

                  const {key: _lineKey, ...lineProps} = getLineProps({
                    line,
                  });

                  lineProps.className += ' ' + styles.codeBlockLine;

                  const lineNum = i + 1;
                  const lineErrors = errorsByPrimaryLine.get(lineNum);
                  const hasError = lineErrors != null && lineErrors.length > 0;

                  return (
                    <React.Fragment key={i}>
                      <span {...lineProps}>
                        <span className={styles.codeBlockLineNumber}>
                          {hasError && <span className={styles.gutterMarker} />}
                          <span className={styles.lineNumberText}>
                            {lineNum}
                          </span>
                        </span>
                        <span className={styles.codeBlockLineContent}>
                          {line.map((token, tokenIdx) => {
                            const {key: _tokenKey, ...tokenProps} =
                              getTokenProps({token});
                            return <span key={tokenIdx} {...tokenProps} />;
                          })}
                          {getHighlightRangesOnLine(
                            flowErrors,
                            lineNum,
                            lineWidths[i],
                          ).map((range, key) => (
                            <span
                              key={key}
                              className={styles.flowErrorHighlight}
                              style={{
                                left: `${range.start}ch`,
                                width: `${range.end - range.start}ch`,
                              }}
                            />
                          ))}
                        </span>
                      </span>
                      {hasError && showInlineErrors && (
                        <>
                          <span className={styles.inlineErrorSpacer} />
                          {/* $FlowFixMe[incompatible-use] lineErrors may be undefined */}
                          {lineErrors.map(({error}, errorIdx) => {
                            // Show only the primary message (first line)
                            const primaryMessage =
                              error.fullDescription.split('\n')[0];
                            return (
                              <React.Fragment key={`err-${errorIdx}`}>
                                {errorIdx > 0 && (
                                  <span className={styles.inlineErrorSpacer} />
                                )}
                                <span className={styles.inlineErrorRow}>
                                  <span className={styles.inlineErrorGutter} />
                                  <span className={styles.inlineErrorContent}>
                                    {formatErrorMessage(
                                      primaryMessage,
                                      error.errorCode,
                                      error.referenceLocations,
                                    )}
                                  </span>
                                </span>
                              </React.Fragment>
                            );
                          })}
                          <span className={styles.inlineErrorSpacer} />
                        </>
                      )}
                    </React.Fragment>
                  );
                })}
              </code>
            </pre>

            {!showInlineErrors &&
              (() => {
                // Mirror the inline path's filter: only render errors that
                // actually have a primary location.
                const footerErrors = flowErrors
                  .map(error => ({error, primaryMsg: error.messages[0]}))
                  .filter(({primaryMsg}) => primaryMsg != null);
                if (footerErrors.length === 0) return null;
                return (
                  <div className={styles.errorFooter}>
                    {footerErrors.map(({error}, errorIdx) => {
                      const primaryMessage =
                        error.fullDescription.split('\n')[0];
                      return (
                        <div key={errorIdx} className={styles.footerError}>
                          <span className={styles.footerErrorBody}>
                            {formatErrorMessage(
                              primaryMessage,
                              null,
                              error.referenceLocations,
                            )}
                          </span>
                        </div>
                      );
                    })}
                  </div>
                );
              })()}

            <div className={styles.buttonGroup}>
              {tryFlowUrl != null && (
                <a
                  className={clsx(styles.tryButton, 'clean-btn')}
                  href={tryFlowUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  aria-label={translate({
                    id: 'theme.CodeBlock.tryButtonAriaLabel',
                    message:
                      'Open this example in Try Flow (opens in a new tab)',
                    description:
                      'The ARIA label for the Try button on flow-check code blocks',
                  })}>
                  <IconExternalLink className={styles.buttonIcon} />
                  <Translate
                    id="theme.CodeBlock.try"
                    description="The try button label on flow-check code blocks">
                    Try
                  </Translate>
                </a>
              )}

              <button
                ref={button}
                type="button"
                aria-label={translate({
                  id: 'theme.CodeBlock.copyButtonAriaLabel',
                  message: '',
                  description: 'The ARIA label for copy code blocks button',
                })}
                className={clsx(styles.copyButton, 'clean-btn')}
                onClick={handleCopyCode}>
                {showCopied ? (
                  <>
                    <IconSuccess
                      className={styles.buttonIcon}
                      aria-hidden="true"
                    />
                    <Translate
                      id="theme.CodeBlock.copied"
                      description="The copied button label on code blocks">
                      Copied
                    </Translate>
                  </>
                ) : (
                  <>
                    <IconCopy
                      className={styles.buttonIcon}
                      aria-hidden="true"
                    />
                    <Translate
                      id="theme.CodeBlock.copy"
                      description="The copy button label on code blocks">
                      Copy
                    </Translate>
                  </>
                )}
              </button>
            </div>
          </div>
        </div>
      )}
    </Highlight>
  );
}
