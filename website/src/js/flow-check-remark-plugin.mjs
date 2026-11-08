/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

import getFlowMeta from './initialized-flow-provider.mjs';

/*:: type UnwrapPromise<T> = T extends Promise<infer V> ? V : T */

const parseOptions = (rest /*: string */) /*: {[key: string]: boolean} */ => {
  const options /*: {[key: string]: boolean} */ = {};
  for (const tok of rest.split(/\s+/).filter(Boolean)) {
    if (tok === 'first-error') options.firstErrorOnly = true;
  }
  return options;
};

const matchNode = (node /*: any */) /*: {[key: string]: boolean} | null */ => {
  if (node.type !== 'code') return null;
  const meta = node.meta || '';
  if (meta === 'flow-check') return {};
  const m = /^flow-check\s+(.+)$/.exec(meta);
  return m == null ? null : parseOptions(m[1]);
};

const transformNode = async (
  node /*: any */,
  options /*: {[key: string]: boolean} */,
) => [
  {
    type: 'code',
    lang: 'flow',
    meta: await getFlowMeta(node.value, options),
    value: node.value,
  },
];

export default () /*: any */ => {
  const transformer = async (
    node /*: any */,
  ) /*: Promise<UnwrapPromise<ReturnType<typeof transformNode>> | null> */ => {
    const options = matchNode(node);
    if (options != null) {
      return transformNode(node, options);
    }
    if (Array.isArray(node.children)) {
      let index = 0;
      while (index < node.children.length) {
        const result = await transformer(node.children[index]);
        if (result) {
          node.children.splice(index, 1, ...result);
          index += result.length;
        } else {
          index += 1;
        }
      }
    }
    return null;
  };
  return transformer;
};
