/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

// defines window.requirejs
// $FlowFixMe[cannot-resolve-module]
import './require_2_3_3';

declare function requirejs(
  pathList: ReadonlyArray<string>,
  resolve: (v: any) => void,
): void;

const versionCache /*: Map<string, Promise<FlowJs>> */ = new Map();

const MASTER_VERSION = 'master';
const FLOW_RELEASE_VERSION =
  /^\d+\.\d+\.\d+(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$/;

const SPLIT_FLOWLIB_FILES: ReadonlyArray<string> = [
  'lib.decorators.js',
  'lib.decorators.legacy.js',
  'lib.es2015.collection.js',
  'lib.es2015.core.js',
  'lib.es2015.generator.js',
  'lib.es2015.iterable.js',
  'lib.es2015.promise.js',
  'lib.es2015.proxy.js',
  'lib.es2015.reflect.js',
  'lib.es2015.symbol.js',
  'lib.es2015.symbol.wellknown.js',
  'lib.es2016.array.include.js',
  'lib.es2016.intl.js',
  'lib.es2017.arraybuffer.js',
  'lib.es2017.date.js',
  'lib.es2017.intl.js',
  'lib.es2017.object.js',
  'lib.es2017.sharedmemory.js',
  'lib.es2017.string.js',
  'lib.es2017.typedarrays.js',
  'lib.es2018.asyncgenerator.js',
  'lib.es2018.asynciterable.js',
  'lib.es2018.intl.js',
  'lib.es2018.promise.js',
  'lib.es2018.regexp.js',
  'lib.es2019.array.js',
  'lib.es2019.intl.js',
  'lib.es2019.object.js',
  'lib.es2019.string.js',
  'lib.es2019.symbol.js',
  'lib.es2020.bigint.js',
  'lib.es2020.date.js',
  'lib.es2020.intl.js',
  'lib.es2020.number.js',
  'lib.es2020.promise.js',
  'lib.es2020.sharedmemory.js',
  'lib.es2020.string.js',
  'lib.es2020.symbol.wellknown.js',
  'lib.es2021.intl.js',
  'lib.es2021.promise.js',
  'lib.es2021.string.js',
  'lib.es2021.weakref.js',
  'lib.es2022.array.js',
  'lib.es2022.error.js',
  'lib.es2022.intl.js',
  'lib.es2022.object.js',
  'lib.es2022.regexp.js',
  'lib.es2022.string.js',
  'lib.es2023.array.js',
  'lib.es2023.collection.js',
  'lib.es2023.intl.js',
  'lib.es2024.arraybuffer.js',
  'lib.es2024.collection.js',
  'lib.es2024.object.js',
  'lib.es2024.promise.js',
  'lib.es2024.regexp.js',
  'lib.es2024.sharedmemory.js',
  'lib.es2024.string.js',
  'lib.es5.js',
  'lib.esnext.array.js',
  'lib.esnext.collection.js',
  'lib.esnext.decorators.js',
  'lib.esnext.disposable.js',
  'lib.esnext.error.js',
  'lib.esnext.float16.js',
  'lib.esnext.intl.js',
  'lib.esnext.iterator.js',
  'lib.esnext.promise.js',
  'lib.esnext.sharedmemory.js',
  'misc.js',
  'react.js',
  'dom_extra_to_be_removed.js',
];

// Flowlib files taken verbatim from `tslib/`. They keep their `.js` name so they are checked as
// Flow libdefs, but their contents are fetched from the `.d.ts` file.
const TSLIB_SOURCED_FLOWLIB_FILES: ReadonlyArray<string> = [
  'lib.decorators.js',
  'lib.decorators.legacy.js',
  'lib.es2015.proxy.js',
  'lib.es2015.reflect.js',
  'lib.es2015.symbol.js',
  'lib.es2015.symbol.wellknown.js',
  'lib.es2016.intl.js',
  'lib.es2017.arraybuffer.js',
  'lib.es2017.date.js',
  'lib.es2017.intl.js',
  'lib.es2017.object.js',
  'lib.es2017.sharedmemory.js',
  'lib.es2017.string.js',
  'lib.es2017.typedarrays.js',
  'lib.es2018.intl.js',
  'lib.es2018.promise.js',
  'lib.es2018.regexp.js',
  'lib.es2019.intl.js',
  'lib.es2019.object.js',
  'lib.es2019.string.js',
  'lib.es2019.symbol.js',
  'lib.es2020.bigint.js',
  'lib.es2020.date.js',
  'lib.es2020.intl.js',
  'lib.es2020.number.js',
  'lib.es2020.sharedmemory.js',
  'lib.es2020.string.js',
  'lib.es2020.symbol.wellknown.js',
  'lib.es2021.intl.js',
  'lib.es2021.promise.js',
  'lib.es2021.string.js',
  'lib.es2021.weakref.js',
  'lib.es2022.array.js',
  'lib.es2022.error.js',
  'lib.es2022.intl.js',
  'lib.es2022.object.js',
  'lib.es2022.regexp.js',
  'lib.es2022.string.js',
  'lib.es2023.array.js',
  'lib.es2023.collection.js',
  'lib.es2023.intl.js',
  'lib.es2024.arraybuffer.js',
  'lib.es2024.collection.js',
  'lib.es2024.object.js',
  'lib.es2024.promise.js',
  'lib.es2024.regexp.js',
  'lib.es2024.sharedmemory.js',
  'lib.es2024.string.js',
  'lib.esnext.array.js',
  'lib.esnext.decorators.js',
  'lib.esnext.disposable.js',
  'lib.esnext.error.js',
  'lib.esnext.float16.js',
  'lib.esnext.intl.js',
  'lib.esnext.promise.js',
  'lib.esnext.sharedmemory.js',
];

const TSLIB_SOURCED_FLOWLIB_FILES_338: ReadonlyArray<string> = [
  ...TSLIB_SOURCED_FLOWLIB_FILES,
  'lib.es2015.generator.js',
  'lib.es2018.asyncgenerator.js',
  'lib.es2018.asynciterable.js',
];

// Flowlib files taken from `tslib/` with Flow-specific changes. Their contents are fetched from the
// adapted `.d.ts` file in `tslib_patches/`.
const TSLIB_PATCHED_FLOWLIB_FILES_338: ReadonlyArray<string> = [
  'lib.es2015.collection.js',
  'lib.es2015.core.js',
  'lib.es2015.iterable.js',
  'lib.es2015.promise.js',
  'lib.es2016.array.include.js',
  'lib.es2019.array.js',
  'lib.es2020.promise.js',
  'lib.esnext.collection.js',
  'lib.esnext.iterator.js',
];

function sourceUrlOf(
  url: string,
  tslibSourcedFiles: ReadonlyArray<string>,
  tslibPatchedFiles: ReadonlyArray<string>,
): string {
  const filename = url.substring(url.lastIndexOf('/') + 1);
  if (tslibPatchedFiles.includes(filename)) {
    return url.replace(/\/flowlib\/([^/]+)\.js$/, '/tslib_patches/$1.d.ts');
  }
  return tslibSourcedFiles.includes(filename)
    ? url.replace(/\/flowlib\/([^/]+)\.js$/, '/tslib/$1.d.ts')
    : url;
}

const TRY_LIB_CONTENTS = `
declare type $JSXIntrinsics = {
  [string]: {
    instance: any,
    props: {
      children?: React$Node,
      [key: string]: any,
    },
  },
};
`.slice(1);

// We need to normalize the URL.
// The register file function only accepts absolute path.
function normalizeUrlForFilename(url: string) {
  return url.startsWith('https://unpkg.com/')
    ? url.substring('https://unpkg.com'.length)
    : url;
}

function get(
  url: string,
  tslibSourcedFiles: ReadonlyArray<string>,
  tslibPatchedFiles: ReadonlyArray<string>,
) {
  const sourceUrl = sourceUrlOf(url, tslibSourcedFiles, tslibPatchedFiles);
  return new Promise<[string, string]>((resolve, reject) => {
    const req = new XMLHttpRequest();
    req.timeout = 5000;
    req.onload = () => {
      if (req.status === 200) {
        const name = normalizeUrlForFilename(url);
        resolve([name, req.response]);
      } else {
        reject(
          new Error(
            `Failed to load ${sourceUrl}: ${req.status} ${req.statusText}`,
          ),
        );
      }
    };
    req.onerror = () => {
      reject('Network error');
    };
    req.ontimeout = () => {
      reject('Network timed out');
    };
    req.onabort = () => {
      reject('Network request aborted');
    };
    req.open('GET', sourceUrl);
    req.send();
  });
}

function normalizeFlowReleaseVersion(version: string): string {
  const normalizedVersion = version.startsWith('v')
    ? version.substring(1)
    : version;
  if (!FLOW_RELEASE_VERSION.test(normalizedVersion)) {
    throw new Error(`Invalid Flow release version: ${version}`);
  }
  return normalizedVersion;
}

function versionedUnpkgComUrl(version: string): string {
  const releaseVersion = normalizeFlowReleaseVersion(version);
  return `https://unpkg.com/try-flow-website-js@${encodeURIComponent(
    releaseVersion,
  )}`;
}

function masterVersionBasePath(version: string): ?string {
  switch (version) {
    case MASTER_VERSION:
      return '/flow/master';
    default:
      return null;
  }
}

function isFlowJs(value: unknown): boolean {
  return (
    value != null &&
    typeof (value as any).checkContent === 'function' &&
    typeof (value as any).registerFile === 'function' &&
    typeof (value as any).initBuiltins === 'function'
  );
}

function getGlobalFlow(): ?FlowJs {
  const globalObject =
    typeof self !== 'undefined'
      ? self
      : typeof window !== 'undefined'
        ? window
        : typeof globalThis !== 'undefined'
          ? globalThis
          : null;
  if (globalObject == null || !isFlowJs((globalObject as any).flow)) {
    return null;
  }
  return (globalObject as any).flow;
}

function getLoadedFlow(flowModule: unknown): FlowJs {
  if (isFlowJs(flowModule)) {
    return flowModule as any;
  }
  const globalFlow = getGlobalFlow();
  if (globalFlow != null) {
    return globalFlow;
  }
  if (flowModule != null && isFlowJs((flowModule as any).default)) {
    return (flowModule as any).default;
  }
  throw new Error('flow.js loaded without exposing a Flow API');
}

function waitForReady(flow: FlowJs): Promise<FlowJs> {
  const ready = flow.ready;
  if (ready == null) {
    return Promise.resolve(flow);
  }
  return Promise.resolve(ready).then(() => flow);
}

export function load(
  withBaseUrl: (v: string) => string,
  version: string,
): Promise<FlowJs> {
  const cached = versionCache.get(version);
  if (cached) {
    return Promise.resolve(cached);
  }
  const masterBasePath = masterVersionBasePath(version);
  let tslibSourcedFiles = TSLIB_SOURCED_FLOWLIB_FILES_338;
  let tslibPatchedFiles = TSLIB_PATCHED_FLOWLIB_FILES_338;
  let flowPath: string;
  let libs: Array<string>;
  if (masterBasePath != null) {
    flowPath = withBaseUrl(`${masterBasePath}/flow.js`);
    libs = SPLIT_FLOWLIB_FILES.map(filename =>
      withBaseUrl(`${masterBasePath}/flowlib/${filename}`),
    );
  } else {
    let releaseVersion;
    try {
      releaseVersion = normalizeFlowReleaseVersion(version);
    } catch (error) {
      return Promise.reject(error);
    }

    const versionedBaseUrl = versionedUnpkgComUrl(releaseVersion);
    const minorVersion = parseInt(releaseVersion.split('.')[1], 10);
    tslibSourcedFiles =
      minorVersion >= 338
        ? TSLIB_SOURCED_FLOWLIB_FILES_338
        : TSLIB_SOURCED_FLOWLIB_FILES;
    tslibPatchedFiles =
      minorVersion >= 338 ? TSLIB_PATCHED_FLOWLIB_FILES_338 : [];
    flowPath = `${versionedBaseUrl}/flow.js`;
    libs =
      minorVersion >= 333
        ? SPLIT_FLOWLIB_FILES.map(
            filename => `${versionedBaseUrl}/flowlib/${filename}`,
          )
        : minorVersion >= 266
          ? [
              `${versionedBaseUrl}/flowlib/core.js`,
              `${versionedBaseUrl}/flowlib/react.js`,
            ]
          : [
              `${versionedBaseUrl}/flowlib/core.js`,
              `${versionedBaseUrl}/flowlib/react.js`,
              `${versionedBaseUrl}/flowlib/intl.js`,
            ];
  }
  const flowLoader = new Promise<FlowJs>((resolve, reject) => {
    requirejs([flowPath], flowModule => {
      try {
        resolve(getLoadedFlow(flowModule));
      } catch (error) {
        reject(error);
      }
    });
  });
  return Promise.all([
    flowLoader.then(waitForReady),
    ...libs.map(url => get(url, tslibSourcedFiles, tslibPatchedFiles)),
  ] as any).then(([flow, ...contents]) => {
    contents.forEach(nameAndContent => {
      flow.registerFile(nameAndContent[0], nameAndContent[1]);
    });
    flow.registerFile('try-lib.js', TRY_LIB_CONTENTS);
    flow.initBuiltins([...libs.map(normalizeUrlForFilename), 'try-lib.js']);
    versionCache.set(version, flow);
    return flow;
  });
}
