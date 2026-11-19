/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 * @format
 */

/* dedicated to Michael Shellmocker,
 * seven time world champion of testing shell commands */

const {watch} = require('fs');
const {readFile, writeFile} = require('fs').promises;
const {join} = require('path');

const {mkdirp} = require('../utils/async');

type ProcessEnv = {[key: string]: string | void};

/** list of arguments in a particular invocation */
export type Invocation = string[];
export type Invocations = Invocation[];
export type AllInvocations = {
  [name: string]: Invocations,
};

class ShellMocker {
  dir: string;
  binDir: string;
  outDir: string;
  names: Set<string>;

  constructor(dir: string) {
    this.dir = dir;
    this.binDir = join(this.dir, 'bin');
    this.outDir = join(this.dir, 'out');
    this.names = new Set();
  }

  async createFreshDirs(): Promise<void> {
    await mkdirp(this.binDir);
    await mkdirp(this.outDir);
  }

  prepareProcessEnv(env: Readonly<ProcessEnv>): ProcessEnv {
    // on Windows we'd need to use ; instead of :
    const PATH = `${this.binDir}:${env.PATH || ''}`;
    return {...env, PATH};
  }

  binFile(name: string): string {
    return join(this.binDir, name);
  }

  outFile(name: string): string {
    return join(this.outDir, name);
  }

  async add(name: string): Promise<void> {
    const binFile = this.binFile(name);
    const outFile = this.outFile(name);
    const mockScript = `#!/usr/bin/env bash\necho $@ >> ${outFile}`;
    await writeFile(binFile, mockScript, {mode: 0o777});
    await writeFile(outFile, '');
    this.names.add(name);
  }

  async clear(name: string): Promise<void> {
    await writeFile(this.outFile(name), '');
  }

  async clearAll(): Promise<void> {
    for (const [name] of this.names.entries()) {
      await this.clear(name);
    }
  }

  async get(name: string): Promise<Invocations> {
    const contents = await readFile(this.outFile(name), 'utf8');
    const lines = contents.split('\n');
    lines.pop(); // remove trailing empty string
    return lines.map(line => line.split(' '));
  }

  async getAll(): Promise<AllInvocations> {
    const ret: {[key: string]: Invocations} = {};
    for (const [name] of this.names.entries()) {
      ret[name] = await this.get(name);
    }
    return ret;
  }

  /**
   * wait until the specified mock is invoked OR timeout ms elapses
   * returned bool is whether an invocation occurred
   */
  async wait(name: string, timeout: number): Promise<boolean> {
    let watcher;
    const waitPromise = new Promise<boolean>(resolve => {
      watcher = watch(this.outFile(name), null, resolve.bind(null, true));
    });
    const timeoutPromise = new Promise<?boolean>(resolve =>
      setTimeout(resolve, timeout),
    ).then(() => {
      watcher.close();
      return false;
    });
    return await Promise.race([waitPromise, timeoutPromise]);
  }

  /**
   * wait until at least one mock is invoked OR timeout ms elapses
   * returned bool is whether an invocation occurred
   */
  async waitAll(timeout: number): Promise<boolean> {
    const waits = [];
    this.names.forEach(name => waits.push(this.wait(name, timeout)));
    const results = await Promise.all(waits);
    return results.some(x => x);
  }
}

module.exports = {
  default: ShellMocker,
};
