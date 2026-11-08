/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

import * as React from 'react';
import Link from '@docusaurus/Link';

import type FlowJsServices from './flow-services';
import styles from './TryFlow.module.css';

component DocsLink(kind: 'option' | 'lint', id: string) {
  let href;
  if (kind === 'lint') {
    href = `/en/docs/linting/rule-reference/#toc-${id}`;
  } else {
    const fragment = id.toLowerCase().replace(/[^a-z]/g, '-');
    href = `/en/docs/config/options/#toc-${fragment}`;
  }
  return <Link to={href}>[docs]</Link>;
}

component TryFlowConfigRows(
  name: string,
  schema: FlowJsConfigSchema,
  config: {[key: string]: unknown},
  setConfig: (v: {[key: string]: unknown}) => void,
) {
  return (
    <>
      {schema.length > 0 ? (
        <tr>
          <td colSpan={2}>
            <h3>{name}</h3>
          </td>
        </tr>
      ) : null}
      {schema.map(item => (
        <tr key={item.key}>
          <td className={styles.tryEditorConfigInputCell}>
            {item.type === 'enum' ? (
              <select
                // $FlowFixMe[incompatible-type]
                value={config[item.key]}
                // $FlowFixMe[incompatible-type]
                onChange={(event: SyntheticInputEvent<>) => {
                  setConfig({
                    ...config,
                    [item.key]: event.target.value,
                  });
                }}>
                {item.choices.map(choice => (
                  <option key={choice} value={choice}>
                    {choice}
                  </option>
                ))}
              </select>
            ) : (
              <input
                type="checkbox"
                id={item.key}
                // $FlowFixMe[incompatible-type]
                checked={config[item.key]}
                // $FlowFixMe[incompatible-type]
                onChange={(event: SyntheticInputEvent<>) => {
                  setConfig({
                    ...config,
                    [item.key]: event.target.checked,
                  });
                }}
              />
            )}
          </td>
          <td className={styles.tryEditorConfigLabelCell}>
            <label htmlFor={item.key}>
              <div className={styles.tryEditorConfigLabel}>{item.key}</div>
              {item.desc != null ? (
                <div>
                  {item.desc}
                  <DocsLink id={item.key} kind={item.kind} />
                </div>
              ) : null}
            </label>
          </td>
        </tr>
      ))}
    </>
  );
}

export default component TryFlowConfigEditor(
  flowService: ?FlowJsServices,
  setConfig: (v: {[key: string]: unknown}) => void,
) {
  if (flowService == null) {
    return 'Loading Flow configuration schema';
  }
  if (flowService?.schema == null) {
    return 'Configuration is not supported on this version of Flow.';
  }
  const [optionsSchema, lintsSchema] = flowService.schema.reduce<
    [FlowJsConfigSchema, FlowJsConfigSchema],
  >(
    ([optionsSchema, lintsSchema], config) => {
      if (config.kind === 'lint') {
        return [optionsSchema, lintsSchema.concat(config)];
      } else {
        return [optionsSchema.concat(config), lintsSchema];
      }
    },
    [[], []],
  );
  return (
    <table>
      <tbody>
        <TryFlowConfigRows
          name="Options"
          schema={optionsSchema}
          config={flowService.config}
          setConfig={setConfig}
        />
        <TryFlowConfigRows
          name="Lints"
          schema={lintsSchema}
          config={flowService.config}
          setConfig={setConfig}
        />
      </tbody>
    </table>
  );
}
