/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 * @format
 */

import type {ErrorCheckCommand} from '../errors';

const {execSync, spawn} = require('child_process');
const {randomBytes} = require('crypto');
const {createWriteStream, realpathSync, lstatSync} = require('fs');
// $FlowFixMe[prop-missing]
const {
  access,
  appendFile,
  readdir,
  readFile,
  stat,
  unlink,
  writeFile,
  cp,
  symlink,
} = require('fs').promises;
const {platform, tmpdir} = require('os');
const {basename, dirname, extname, join, sep: dir_sep} = require('path');
const {format} = require('util');
const EventEmitter = require('events');

const rpc = require('vscode-jsonrpc');
const {URI: VscodeURI, uriToFsPath} = require('vscode-uri');

import type {LSPMessage, RpcConnection} from './lsp';

const {exec, execManual, isRunning, mkdirp, sleep} = require('../utils/async');
const {getTestsDir} = require('../constants');
const {default: ShellMocker} = require('./ShellMocker');
const {
  describeServerCrash,
  isCrash,
  isServerLogName,
  logWindow,
} = require('./serverCrash');

import type {SuiteResult} from './runTestSuite';
import type {AllInvocations} from './ShellMocker';
import type {ServerExit, ServerLog} from './serverCrash';
import type {FlowResult} from '../flowResult';

type JsonValue = null | boolean | number | string | JsonObject;
type JsonObject = {[key: string]: JsonValue};

type CancellationToken = {
  readonly isCancellationRequested: boolean,
  onCancellationRequested(callback: () => any): void,
  ...
};

class TestBuilder {
  bin: string;
  dir: string;
  errorCheckCommand: ErrorCheckCommand;
  flowConfigFilename: string;
  lazyMode: 'ide' | 'fs' | null;
  server: null | child_process$ChildProcess = null;
  lsp: null | {
    connection: RpcConnection,
    process: child_process$ChildProcess,
    outstandingRequestsFromServer: Map<
      number,
      {resolve: (v: any) => void, reject: (v: Error) => void},
    >,
    outstandingRequestsInfo: {nextId: number, mostRecent: ?number, ...},
    stderr: Array<string>,
    messageEmitter: EventEmitter,
    ...
  } = null;
  lspMessages: Array<LSPMessage>; // this should outlive the death of the lsp+server in a step
  lspEmitter: EventEmitter;
  serverEmitter: EventEmitter;
  sourceDir: string;
  suiteName: string;
  tmpDir: string;
  serverCrash: ServerExit | null = null;
  serverLogMarks: Map<string, number> = new Map();
  allowFlowServerToDie: boolean = false;
  logStream: stream$Writable | null;
  shellMocker: ShellMocker;
  waitForRecheck: boolean;
  noAutoRestart: boolean;

  constructor(
    bin: string,
    errorCheckCommand: ErrorCheckCommand,
    baseDir: string,
    providedTestsDir: ?string,
    suiteName: string,
    testNum: number,
    flowConfigFilename: string,
    lazyMode: 'ide' | 'fs' | null,
    wait_for_recheck: boolean,
    noAutoRestart: boolean,
  ) {
    this.bin = bin;
    // If we're testing lazy mode, then we must use status
    this.errorCheckCommand = lazyMode == null ? errorCheckCommand : 'status';
    this.suiteName = suiteName;
    this.lspEmitter = new EventEmitter();
    this.serverEmitter = new EventEmitter();
    this.dir = join(baseDir, String(testNum));
    this.sourceDir = join(getTestsDir(providedTestsDir), suiteName);
    this.tmpDir = join(baseDir, 'tmp', String(testNum));
    this.flowConfigFilename = flowConfigFilename;
    this.lazyMode = lazyMode;
    this.lsp = null;
    this.lspMessages = [];
    this.shellMocker = new ShellMocker(join(baseDir, 'mock', String(testNum)));
    this.waitForRecheck = wait_for_recheck;
    this.noAutoRestart = noAutoRestart;
  }

  getFileName(): string {
    return join(this.dir, 'test.js');
  }

  async log(fmt: string, ...args: Array<unknown>): Promise<void> {
    const {logStream} = this;
    if (logStream != null) {
      const now = new Date();
      function fixWidth(num: number, width: number): string {
        const str = String(num);
        return str.length < width ? '0'.repeat(width - str.length) + str : str;
      }
      const msg = format(
        '[%s %s:%s:%s.%s] %s\n',
        now.toLocaleDateString('en-US'),
        fixWidth(now.getHours(), 2),
        fixWidth(now.getMinutes(), 2),
        fixWidth(now.getSeconds(), 2),
        fixWidth(now.getMilliseconds(), 3),
        format(fmt, ...args),
      );
      return new Promise((resolve, reject) => {
        logStream.write(msg, 'utf8', err => (err ? reject(err) : resolve()));
      });
    }
  }

  async closeLog(): Promise<void> {
    const {logStream} = this;
    if (logStream != null) {
      this.logStream = null;
      return new Promise((resolve, reject) => {
        logStream.end('', 'utf8', resolve);
      });
    }
  }

  async createFreshDir(): Promise<void> {
    await mkdirp(this.dir);
    await mkdirp(this.tmpDir);
    await this.shellMocker.createFreshDirs();

    this.logStream = createWriteStream(join(this.tmpDir, 'test.log'));

    let configBuffer = null;
    const files = await readdir(this.sourceDir);
    for (const file of files) {
      if (basename(file) === this.flowConfigFilename) {
        configBuffer = await readFile(join(this.sourceDir, file));
      }
    }

    if (configBuffer != null) {
      await writeFile(join(this.dir, '.flowconfig'), configBuffer.toString());
    } else {
      await exec(format('%s init %s', this.bin, this.dir), {cwd: __dirname});
    }
    await writeFile(this.getFileName(), '/* @flow */\n');
  }

  async addCode(code: string): Promise<void> {
    const filename = this.getFileName();
    await appendFile(filename, '\n' + code + '\n');
    await this.forceRecheck([filename]);
  }

  async addFileImpl(source: string, dest: string): Promise<string> {
    source = join(this.sourceDir, source);
    dest = join(this.dir, dest);
    await mkdirp(dirname(dest));
    const contents_buffer = await readFile(source);
    let contents = contents_buffer.toString();
    if (contents.match(/@thisWillBeFlowInTest/)) {
      // Undo what the convert command did
      contents = contents.replace(/@thisWillBeFlowInTest/, '@flow');
    }
    await writeFile(dest, contents);
    return dest;
  }

  async addFile(source: string, dest: string): Promise<void> {
    const filename = await this.addFileImpl(source, dest);
    await this.forceRecheck([filename]);
  }

  async addFiles(sources: Array<string>): Promise<void> {
    const filenames = await Promise.all(
      sources.map(source => this.addFileImpl(source, source)),
    );
    await this.forceRecheck(filenames);
  }

  async addSymlinkDirs(
    destinationPathPairs: Array<[string, string]>,
  ): Promise<void> {
    const filenames = await Promise.all(
      destinationPathPairs.map(async ([destination, path]) => {
        const dest = join(this.dir, destination);
        await mkdirp(dirname(dest));
        await symlink(path, dest, 'dir');
        return dest;
      }),
    );
    await this.forceRecheck(filenames);
  }

  async removeFileImpl(file: string): Promise<string> {
    file = join(this.dir, file);
    await unlink(file);
    return file;
  }

  async removeFile(file: string): Promise<void> {
    const filename = await this.removeFileImpl(file);
    await this.forceRecheck([filename]);
  }

  async removeFiles(files: Array<string>): Promise<void> {
    const filenames = await Promise.all(
      files.map(file => this.removeFileImpl(file)),
    );
    await this.forceRecheck(filenames);
  }

  async modifyFile(
    filename: string,
    searchValue: string | RegExp,
    replaceValue: string,
  ): Promise<void> {
    filename = join(this.dir, filename);
    const buffer = await readFile(filename);
    const contents = buffer.toString().replace(searchValue, replaceValue);
    await writeFile(filename, contents);
    await this.forceRecheck([filename]);
  }

  async readFileInDir(relPath: string): Promise<string> {
    const buffer = await readFile(join(this.dir, relPath));
    return buffer.toString();
  }

  async fileExistsInDir(relPath: string): Promise<boolean> {
    try {
      // `access` checks existence without reading the file into memory.
      await access(join(this.dir, relPath));
      return true;
    } catch (_) {
      return false;
    }
  }

  async mockShellCommand(name: string): Promise<void> {
    await this.shellMocker.add(name);
  }

  async clearMockInvocations(): Promise<void> {
    await this.shellMocker.clearAll();
  }

  async waitUntilMockInvocation(timeout: number): Promise<boolean> {
    return await this.shellMocker.waitAll(timeout);
  }

  async getMockInvocationsSinceStartOfStep(): Promise<AllInvocations> {
    return await this.shellMocker.getAll();
  }

  async execManualAndLog(
    cmd: string,
    options?: child_process$execOpts,
  ): Promise<[?child_process$Error, string, string]> {
    await this.log(`# ${cmd}...`);
    const [err, stdout_, stderr_] = await execManual(cmd, options);
    await this.log(`# Finished ${cmd}.`);
    const stderr = typeof stderr_ === 'string' ? stderr_ : stderr_.toString();
    const stdout = typeof stdout_ === 'string' ? stdout_ : stdout_.toString();
    if (err != null) {
      await this.log(`# err.code=${String(err.code)} err=${String(err)}`);
    }
    if (stderr !== '') {
      await this.log(`# stderr=BEGIN`);
      await this.log(stderr);
      await this.log(`# END stderr`);
    }
    return [err, stdout, stderr];
  }

  async flowCmd(
    args: Array<string>,
    stdinFile?: string,
  ): Promise<[number | string | null, string, string]> {
    let cmd = format(
      '%s %s %s',
      this.bin,
      args.map(arg => format('"%s"', arg)).join(' '),
      stdinFile == null ? '' : format('< %s', stdinFile),
    );
    const [err, stdout, stderr] = await this.execManualAndLog(cmd, {
      cwd: this.dir,
      env: {
        ...process.env,
        FLOW_TEMP_DIR: this.tmpDir,
      },
    });
    const code = err == null ? 0 : err.code;

    return [code, stdout, stderr];
  }

  async getFlowErrors(retry?: boolean = true): Promise<FlowResult> {
    let cmd;
    match (this.errorCheckCommand) {
      'full-check' | 'check' => {
        cmd = format(
          '%s full-check --strip-root --json %s',
          this.bin,
          this.dir,
        );
      }
      'status' => {
        // No-op if it's already running
        await this.startFlowServer();
        cmd = format(
          '%s status --no-auto-start --strip-root --json %s',
          this.bin,
          this.dir,
        );
      }
    }

    const [err, stdout, stderr] = await this.execManualAndLog(cmd, {
      cwd: __dirname,
      maxBuffer: 1024 * 1024,
      env: {
        ...process.env,
        FLOW_TEMP_DIR: this.tmpDir,
      },
    });

    // 0 - no errors
    // 2 - Some errors
    if (err == null || err.code === 2) {
      return JSON.parse(stdout.toString());
    }

    throw new Error(format('Flow check failed!', err, stdout, stderr));
  }

  async startFlowServer(): Promise<void> {
    if (this.server !== null) {
      return;
    }
    const lazyMode =
      this.lazyMode === null ? [] : ['--lazy-mode', this.lazyMode];
    const noAutoRestart = this.noAutoRestart ? ['--no-auto-restart'] : [];
    const args = [
      'server',
      '--strip-root',
      '--debug',
      '--file-watcher',
      'none',
      '--wait-for-recheck',
      String(this.waitForRecheck),
    ]
      .concat(noAutoRestart)
      .concat(lazyMode)
      .concat([this.dir]);
    await this.log('# %s %s', this.bin, args.join(' '));
    const serverProcess = spawn(this.bin, args, {
      // Useful for debugging flow server
      // stdio: ["pipe", "pipe", process.stderr],
      cwd: this.dir,
      env: this.shellMocker.prepareProcessEnv({
        ...process.env,
        FLOW_TEMP_DIR: this.tmpDir,
        OCAMLRUNPARAM: 'b',
      }),
    });
    this.server = serverProcess;
    this.serverEmitter.emit('server');

    const stderr = [];
    serverProcess.stderr.on('data', (data: string | Buffer) => {
      stderr.push(data.toString());
    });

    serverProcess.on('exit', (code: number | null, signal: string | null) => {
      // `stopFlowServer` clears `this.server` before it kills, so a null one here means the
      // harness is the one that asked.
      const context = {
        harnessStoppedIt: this.server == null,
        stepAllowsDeath: this.allowFlowServerToDie,
      };
      if (isCrash(context)) {
        this.serverCrash = {code, signal, stderr: stderr.join('')};
      }
      this.stopFlowServer();
    });

    // Wait for the server to be ready or die
    await new Promise((resolve, reject) => {
      function done() {
        serverProcess.stderr.removeListener('data', resolveOnReady);
        serverProcess.stderr.removeListener('exit', resolveOnExit);
        resolve();
      }
      const resolveOnReady = (data: string) => {
        stderr
          .concat([data])
          .join('')
          .match(/Server is free/) && done();
      };
      const resolveOnExit = done;

      serverProcess.stderr.on('data', resolveOnReady);
      serverProcess.on('exit', resolveOnExit);
    });
  }

  stopFlowServer(): void {
    const server = this.server;
    if (server != null) {
      this.server = null;
      server.stdin.end();
      server.kill();
      this.serverEmitter.emit('server');
    }
  }

  async createLSPConnection(): Promise<void> {
    if (this.lsp != null) {
      // No-op if the lsp server is already running
    }

    const args = ['lsp', '--autostop', '--lazy-mode', 'ide'];
    const lspProcess = spawn(this.bin, args, {
      // Useful for debugging flow lsp
      // stdio: ["pipe", "pipe", process.stderr],
      cwd: this.dir,
      env: this.shellMocker.prepareProcessEnv({
        ...process.env,
        FLOW_TEMP_DIR: this.tmpDir,
        OCAMLRUNPARAM: 'b',
      }),
    });
    const connection = rpc.createMessageConnection(
      new rpc.StreamMessageReader(lspProcess.stdout),
      new rpc.StreamMessageWriter(lspProcess.stdin),
    );
    connection.trace(rpc.Trace.Verbose, this);
    connection.listen();

    // In nodejs, the 'exit' even is fired when the process exits;
    // the 'close' event is fired when its stdio streams have been closed.
    // The streams might be closed before exit if the process manually
    // closes them, or after if it leaves that to the system.
    lspProcess.on('exit', (code: number | null, signal: string | null) => {
      this.cleanupLSPConnection();
    });

    lspProcess.on('close', () => this.cleanupLSPConnection());

    const messageEmitter = new EventEmitter();
    const messages: Array<LSPMessage> = [];
    const outstandingRequestsInfo = {
      nextId: 1,
      mostRecent: null as ?number,
    };
    const outstandingRequestsFromServer: Map<
      number,
      {resolve: (v: any) => void, reject: (v: Error) => void},
    > = new Map();

    connection.onRequest(
      (
        method: string,
        rawParams: Array<unknown> | {...} | void,
        cancellationToken: CancellationToken,
      ) => {
        const id = outstandingRequestsInfo.nextId;
        outstandingRequestsInfo.mostRecent = id;
        outstandingRequestsInfo.nextId++;
        const params = this.sanitizeIncomingLSPMessage(rawParams);
        messages.push({method, id, params});
        messageEmitter.emit('message');

        cancellationToken.onCancellationRequested(() => {
          // The underlying Jsonrpc cancellation-request-notification has been
          // wrapped up by vscode-jsonrpc into a CancellationToken. We'll unwrap
          // it, for our messages[] array, so that tests can match on it.
          const synthesizedParams = {id};
          messages.push({method: '$/cancelRequest', params: synthesizedParams});
          messageEmitter.emit('message');
        });

        const promise = new Promise(
          (resolve: (v: any) => void, reject: (v: Error) => void) => {
            outstandingRequestsFromServer.set(id, {resolve, reject});
          },
        );
        return promise;
      },
    );

    connection.onNotification(
      (method: string, rawParams: Array<unknown> | {...} | void) => {
        const params = this.sanitizeIncomingLSPMessage(rawParams);
        messages.push({method, params});
        messageEmitter.emit('message');
      },
    );

    const stderr = [];
    lspProcess.stderr.on('data', (data: string | Buffer) => {
      stderr.push(data.toString());
    });

    await this.log('Created LSP process with pid %d', lspProcess.pid);

    this.lspMessages = messages;
    this.lsp = {
      process: lspProcess,
      connection,
      outstandingRequestsFromServer,
      outstandingRequestsInfo,
      stderr,
      messageEmitter,
    };
    this.lspEmitter.emit('lsp');
  }

  cleanupLSPConnection(): void {
    const lsp = this.lsp;
    if (lsp != null) {
      this.lsp = null;
      lsp.process.stdin.end();
      lsp.process.kill();
      lsp.connection.dispose();
      this.lspEmitter.emit('lsp');
      // but leave lspMessages so it can be examined even after LSP has gone
    }
  }

  getDirUrl(): string {
    return VscodeURI.file(this.dir).toString();
  }

  // sanitizeIncomingLSPMessage: removes a few known fields from server output
  // that are known to be specific to an instance of a test, and replaces
  // them with something fixed.
  sanitizeIncomingLSPMessage(params: any): any {
    // LSP sends back document URLs, to files within the test project
    const dirUrl = this.getDirUrl();
    const replaceDir = (str: string): string => {
      let out = str;
      let index;
      while ((index = out.indexOf(dirUrl)) >= 0) {
        out =
          out.substr(0, index) +
          '<PLACEHOLDER_PROJECT_URL>' +
          out.substr(index + dirUrl.length);
      }
      return out;
    };

    const params2 = JSON.parse(replaceDir(JSON.stringify(params)));

    // Legacy IDE sends back an array of objects where those objects have
    // a '.flowVersion' field
    function replace(obj: JsonObject) {
      for (var k in obj) {
        if (!obj.hasOwnProperty(k)) {
          continue;
        }
        if (obj[k] == null) {
          continue;
        }
        switch (typeof obj[k]) {
          case 'object':
            replace(obj[k]);
            break;
          case 'string':
            if (k == 'flowVersion') {
              obj[k] = '<VERSION STUBBED FOR TEST>';
            }
            break;
        }
      }
    }

    replace(params2);
    return params2;
  }

  // sanitizeOutgoingLSPMessage: replaces some placeholders with values
  // that can only be computed by the builder instance
  sanitizeOutgoingLSPMessage(params: Array<unknown>): Array<unknown> {
    const params2: any = JSON.parse(JSON.stringify(params));

    const dir = this.dir;
    const dirUrl = this.getDirUrl();
    function replace(obj: JsonObject) {
      for (const k in obj) {
        if (!obj.hasOwnProperty(k)) {
          continue;
        }
        switch (typeof obj[k]) {
          case 'object':
            if (obj[k] != null) {
              replace(obj[k]);
            }
            break;
          case 'string':
            if (obj[k].startsWith('<PLACEHOLDER')) {
              obj[k] = obj[k].replace(/^<PLACEHOLDER_PROJECT_URL>/, dirUrl);
            }
            break;
        }
      }
    }

    replace(params2);
    return params2;
  }

  async sendLSPNotification(
    method: string,
    argsRaw: Array<unknown>,
  ): Promise<void> {
    const lsp = this.lsp;
    if (lsp == null) {
      throw new Error('No LSP process running! Cannot sendLSPNotification');
    }
    const args = this.sanitizeOutgoingLSPMessage(argsRaw);
    lsp.connection.sendNotification(method, ...args);
  }

  async sendLSPResponse(
    id: number | 'mostRecent',
    argsRaw: Array<unknown>,
  ): Promise<void> {
    const lsp = this.lsp;
    if (lsp == null) {
      throw new Error('No LSP process running! Cannot sendLSPResponse');
    }
    if (id === 'mostRecent') {
      id = lsp.outstandingRequestsInfo.mostRecent || 0;
    }
    const callbacks = lsp.outstandingRequestsFromServer.get(id);
    if (callbacks == null) {
      throw new Error(`No request id ${id} has arrived`);
    }
    lsp.outstandingRequestsFromServer.delete(id);
    if (argsRaw.length == 1 && argsRaw[0] instanceof Error) {
      const e = argsRaw[0] as Error;
      callbacks.reject(e);
    } else {
      const args = this.sanitizeOutgoingLSPMessage(argsRaw);
      callbacks.resolve(...args);
    }
  }

  // This sends an LSP request and, when the response comes back, adds
  // the response to the message queue. It doesn't fulfil its returned promise
  // until that time.
  async sendLSPRequestAndWaitForResponse(
    method: string,
    argsRaw: Array<unknown>,
  ): Promise<void> {
    const lsp = this.lsp;
    const lspMessages = this.lspMessages;
    if (lsp == null) {
      throw new Error(
        'No lsp process running! Cannot sendLSPRequestAndWaitForResponse',
      );
    }

    const args = this.sanitizeOutgoingLSPMessage(argsRaw);
    let resultRaw;
    try {
      resultRaw = await lsp.connection.sendRequest(method, ...args);
    } catch (error) {
      const message = error.message;
      error = {message, ...error}; // otherwise it doesn't show up in JSON.stringify
      lspMessages.push({method, error});
      lsp.messageEmitter.emit('message');
      return;
    }
    const result = this.sanitizeIncomingLSPMessage(resultRaw);
    lspMessages.push({method, result});
    lsp.messageEmitter.emit('message');
  }

  waitUntilLSPMessage(
    timeoutMs: number,
    expectedMethod: string,
    expectedContents?: string,
  ): Promise<void> {
    const lsp = this.lsp;
    const lspMessages = this.lspMessages;

    return new Promise(resolve => {
      var timeout = null;
      var emitter = null;
      var alreadyDone = false;
      const startTime = new Date().getTime();

      if (lsp == null) {
        this.log('No LSP process running! Cannot waitUntilLSPMessage');
        resolve();
        return;
      }

      const doneWithVerb = async (verb: string) => {
        if (alreadyDone) {
          return;
        }
        alreadyDone = true;
        const duration = new Date().getTime() - startTime;
        emitter && emitter.removeListener('message', checkMessages);
        timeout && clearTimeout(timeout);
        await this.log('%s message %s in %dms', verb, expectedMethod, duration);
        resolve();
      };

      let nextMessageIndex = 0;
      const checkMessages = () => {
        for (; nextMessageIndex < lspMessages.length; nextMessageIndex++) {
          const message = lspMessages[nextMessageIndex];
          if (
            Builder.doesMessageFuzzyMatch(
              message,
              expectedMethod,
              expectedContents,
            )
          ) {
            doneWithVerb('Got');
          }
        }
      };

      // It's unavoidably racey whether the log message gets printed
      // before or after we get the right message
      this.log('Starting to wait %dms for %s', timeoutMs, expectedMethod);

      // Our backlog of messages gets cleared out at the start of each step.
      // If we've already received some messages since the start of the step,
      // let's account for them
      checkMessages();

      // And account for all further messages that arrive
      emitter = lsp.messageEmitter.on('message', checkMessages);

      // ... until our stopping condition
      timeout = setTimeout(() => doneWithVerb('Failed to get'), timeoutMs);
    });
  }

  // waitUntilLSPMessageCount is a confusing method. It looks at all messages
  // that have arrived in this step so far, plus all messages that arrive
  // until the timeout, including both notifications and responses. It stops
  // as soon as either the specified number of messages or until the timeout
  // happens, whichever comes first.
  waitUntilLSPMessageCount(
    timeoutMs: number,
    expectedCount: number,
  ): Promise<void> {
    const lsp = this.lsp;
    const lspMessages = this.lspMessages;

    return new Promise(resolve => {
      if (lsp == null) {
        this.log('No LSP process running! Cannot waitUntilLSPMessageCount');
        resolve();
        return;
      }
      if (expectedCount === 0) {
        resolve();
        return;
      }

      const onMessage = () => {
        expectedCount--;
        if (expectedCount === 0) {
          done('Received');
        }
      };
      var timeout = null;
      const done = (ok: string) => {
        this.lsp &&
          this.lsp.messageEmitter.removeListener('message', onMessage);
        timeout && clearTimeout(timeout);
        this.log(
          '%s all %d messages in under %dms',
          ok,
          expectedCount,
          timeoutMs,
        ).then(resolve);
      };

      // Our backlog of messages gets cleared out at the start of each step.
      // If we've already received some messages since the start of the step,
      // let's account for them:
      lspMessages.forEach(onMessage);

      // And account for all further messages that arrive, until our stopping
      // condition:
      this.log('Starting to wait %dms for messages', timeoutMs).then(() => {
        const onTimeout = () => {
          this.log('%dms timeout fired', timeoutMs).then(() =>
            done('Failed to receive'),
          );
        };
        timeout = setTimeout(onTimeout, timeoutMs);
      });

      lsp.messageEmitter.on('message', onMessage);
    });
  }

  getLSPMessagesSinceStartOfStep(): Array<LSPMessage> {
    return this.lspMessages;
  }

  getLSPStderrSinceStartOfStep(): string {
    return this.lsp ? this.lsp.stderr.join('') : '';
  }

  clearLSPMessages(): void {
    this.lspMessages.splice(0, this.lspMessages.length);
  }

  clearLSPStderr(): void {
    this.lsp && this.lsp.stderr.splice(0, this.lsp.stderr.length);
  }

  waitUntilLSPStatus(
    timeoutMs: number,
    expected: 'stopped' | 'running',
  ): Promise<void> {
    return new Promise(resolve => {
      var timeout = null;
      var emitter = null;
      var alreadyDone = false;
      const startTime = new Date().getTime();

      const doneWithVerb = async (verb: string) => {
        if (alreadyDone) {
          return;
        }
        alreadyDone = true;
        const duration = new Date().getTime() - startTime;
        timeout && clearTimeout(timeout);
        emitter && emitter.removeListener('lsp', checkStatus);
        await this.log('%s LSP %s status in %dms', verb, expected, duration);
        resolve();
      };

      const checkStatus = () => {
        if (
          (expected === 'running' && this.lsp != null) ||
          (expected === 'stopped' && this.lsp == null)
        ) {
          doneWithVerb('Got');
        }
      };

      // It's unavoidably racey whether the async logger does its work before
      // or after we get the first successful checkStatus
      this.log('Waiting up to %dms for %s LSP status', timeoutMs, expected);

      // Test whether we're okay already?
      checkStatus();

      // And look for further changes
      emitter = this.lspEmitter.on('lsp', checkStatus);

      // ... until our stopping condition
      timeout = setTimeout(() => doneWithVerb('Failed to get'), timeoutMs);
    });
  }

  waitUntilServerStatus(
    timeoutMs: number,
    expected: 'stopped' | 'running',
  ): Promise<void> {
    // TODO(ljw): this should check for externally-launched flow servers too
    return new Promise(resolve => {
      var timeout = null;
      var emitter = null;
      var alreadyDone = false;
      const startTime = new Date().getTime();

      const doneWithVerb = async (verb: string) => {
        if (alreadyDone) {
          return;
        }
        alreadyDone = true;
        const duration = new Date().getTime() - startTime;
        emitter && emitter.removeListener('server', checkStatus);
        timeout && clearTimeout(timeout);
        await this.log('%s server %s status in %dms', verb, expected, duration);
        resolve();
      };

      const checkStatus = () => {
        if (
          (expected === 'running' && this.server != null) ||
          (expected === 'stopped' && this.server == null)
        ) {
          doneWithVerb('Got');
        }
      };

      // It's unavoidably racey whether the async logger does its work before
      // or after we get the first successful checkStatus
      this.log('Waiting up to %dms for %s server status', timeoutMs, expected);

      // Test whether we're okay already?
      checkStatus();

      // And look for further changes
      emitter = this.serverEmitter.on('server', checkStatus);

      // ... until our stopping condition
      timeout = setTimeout(() => doneWithVerb('Failed to get'), timeoutMs);
    });
  }

  async cleanup(): Promise<void> {
    this.cleanupLSPConnection();
    this.stopFlowServer();
    this.closeLog();
    // We'll also do a belt-and-braces "flow stop" in case
    // any flow servers were spawned by the test that we're
    // not tracking and handling in stopFlowServer.
    // This call comes last, so that places like process.on('exit')
    // can invoke cleanup(), and at least do all the work they
    // can, even though they don't hang around waiting for flow stop
    // to finish.
    await this.flowCmd(['stop']);
  }

  async serverLogNames(): Promise<Array<string>> {
    return (await readdir(this.tmpDir)).filter(isServerLogName);
  }

  /**
   * A log can be gone by the time it is opened — Flow renames `.log` to `.log.old` when a server
   * restarts — and this whole path exists only to describe a failure. Losing one log beats turning
   * a crash report into an unrelated `ENOENT`, so a file that will not open is skipped. A `tmpDir`
   * that will not list is a different matter and still throws: that is the harness being broken.
   */
  async readLogIfPresent(name: string): Promise<Buffer | null> {
    try {
      return await readFile(join(this.tmpDir, name));
    } catch {
      return null;
    }
  }

  /**
   * Remembers how long each Flow log is, so that a crash can be reported with what the server
   * wrote during the step that killed it rather than with an arbitrary slice of the whole file.
   */
  async markServerLogs(): Promise<void> {
    const marks = new Map<string, number>();
    for (const name of await this.serverLogNames()) {
      try {
        marks.set(name, (await stat(join(this.tmpDir, name))).size);
      } catch {
        continue;
      }
    }
    this.serverLogMarks = marks;
  }

  async readServerLogsSinceMark(): Promise<Array<ServerLog>> {
    const logs: Array<ServerLog> = [];
    for (const name of await this.serverLogNames()) {
      const bytes = await this.readLogIfPresent(name);
      if (bytes == null) {
        continue;
      }
      const contents = logWindow(bytes, this.serverLogMarks.get(name) ?? 0);
      if (contents.trim() !== '') {
        logs.push({name, contents});
      }
    }
    return logs;
  }

  async assertServerDidNotCrash(): Promise<void> {
    const crash = this.serverCrash;
    if (crash != null) {
      throw new Error(
        describeServerCrash(
          crash,
          await this.readServerLogsSinceMark(),
          this.tmpDir,
        ),
      );
    }
  }

  setAllowFlowServerToDie(allow: boolean): void {
    this.allowFlowServerToDie = allow;
  }

  async forceRecheck(files: Array<string>): Promise<void> {
    if (this.server && (await isRunning(this.server.pid))) {
      const files_str = files.map(s => `"${s}"`).join(' ');
      const [err, stdout, stderr] = await this.execManualAndLog(
        format('%s force-recheck --no-auto-start %s', this.bin, files_str),
        {
          env: {...process.env, FLOW_TEMP_DIR: this.tmpDir},
        },
      );

      // No server running (6) is ok - the file change might have killed the
      // server and we raced it here
      if (err && err.code !== 6) {
        throw new Error(
          format(
            'flow force-recheck failed! err.code=%s err=%s stdout=%s stderr=%s files=%s',
            err.code,
            err,
            stdout,
            stderr,
            files,
          ),
        );
      }
    }
  }
}

class Builder {
  runID: string;
  dir: string;
  providedTestsDir: ?string;
  errorCheckCommand: ErrorCheckCommand;

  static builders: Array<TestBuilder> = [];

  static getDirForRun(runID: string): string {
    // tmpdir() is a symlink on macOS, canonicalize it
    const tmp = realpathSync(tmpdir());
    const uri = VscodeURI.file(join(tmp, 'flow', 'tests', runID));
    return uriToFsPath(uri, /* keepDriveLetterCasing */ true);
  }

  /**
   * Tests whether two LSP messages are the same. Currently enforces that
   * the order of properties match as well, but could be loosened in the
   * future since that's not required by the LSP.
   */
  static doesMessageMatch(actual: LSPMessage, expected: LSPMessage): boolean {
    const replacer =
      typeof expected.id == 'undefined' && typeof actual.id !== 'undefined'
        ? (key: string, value: unknown) => (key == 'id' ? undefined : value)
        : null;
    return JSON.stringify(actual, replacer) === JSON.stringify(expected);
  }

  /**
   * doesMessageFuzzyMatch(actual, 'M') judges whether the method name of the
   * actual message was M. And doesMethodMatch(actual, 'M', '{C1,C2,...}')
   * judges also whether the strings C1, C2, ... were all found in the JSON
   * representation of the actual message.
   */
  static doesMessageFuzzyMatch(
    actual: LSPMessage,
    expectedMethod: string,
    expectedContents?: string,
  ): boolean {
    if (actual.method !== expectedMethod) {
      return false;
    }
    if (expectedContents === undefined) {
      return true;
    }
    if (expectedContents === 'null') {
      return actual.result !== undefined && actual.result === 'null';
    }
    const iOpenBrace = expectedContents.indexOf('{');
    const iCloseBrace = expectedContents.lastIndexOf('}');
    const parts = expectedContents
      .substring(iOpenBrace + 1, iCloseBrace)
      .split(',');
    const json = JSON.stringify(actual);
    for (const part of parts) {
      if (!json.includes(part)) {
        return false;
      }
    }
    return true;
  }

  constructor(errorCheckCommand: ErrorCheckCommand, providedTestsDir: ?string) {
    this.errorCheckCommand = errorCheckCommand;
    this.providedTestsDir = providedTestsDir;
    this.runID = randomBytes(5).toString('hex');
    this.dir = Builder.getDirForRun(this.runID);
    process.stderr.write(format('Tests will be built in %s\n', this.dir));

    // If something weird happens, lets make sure to stop all the flow servers
    // we started. Note that cleanup is an async method that does synchronous
    // cleanup and then kicks off a final 'flow stop' and awaits until it's
    // done. In our exit handler we won't live to see the end of that await.
    process.on('exit', this.cleanup);
  }

  cleanup: () => Promise<void> = async () => {
    await Promise.all(Builder.builders.map(builder => builder.cleanup()));
  };

  baseDirForSuite(suiteName: string): string {
    return join(this.dir, suiteName.replace('/', 'zS'));
  }

  async createFreshTest(
    bin: string,
    suiteName: string,
    providedTestsDir: ?string,
    testNum: number,
    flowConfigFilename: string,
    lazyMode: 'fs' | 'ide' | null,
    wait_for_recheck: boolean,
    noAutoRestart: boolean,
  ): Promise<TestBuilder> {
    const testBuilder = new TestBuilder(
      bin,
      this.errorCheckCommand,
      this.baseDirForSuite(suiteName),
      providedTestsDir,
      suiteName,
      testNum,
      flowConfigFilename,
      lazyMode,
      wait_for_recheck,
      noAutoRestart,
    );
    Builder.builders.push(testBuilder);
    await testBuilder.createFreshDir();
    return testBuilder;
  }

  async saveResults(suiteName: string, results: SuiteResult): Promise<void> {
    const dir = this.baseDirForSuite(suiteName);
    const resultsFile = join(dir, 'results.json');
    await mkdirp(dir);
    await writeFile(
      resultsFile,
      JSON.stringify(
        {
          suiteName: suiteName,
          results,
        },
        null,
        2,
      ),
    );
  }
}

module.exports = {
  TestBuilder,
  Builder,
  default: Builder,
};
