/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 * @format
 */

const {format} = require('util');

const parseArgs = require('minimist');

export type Flag =
  | Readonly<{
      type: 'string',
      name: string,
      description: string,
      argName: string,
      aliases?: ReadonlyArray<string>,
      default?: string,
      ...
    }>
  | Readonly<{
      type: 'boolean',
      name: string,
      description: string,
      aliases?: ReadonlyArray<string>,
      ...
    }>
  | Readonly<{
      type: 'enum',
      name: string,
      description: string,
      argName: string,
      validValues: ReadonlyArray<string>,
      aliases?: ReadonlyArray<string>,
      default?: string,
      ...
    }>;

const commonFlags = {
  bin: {
    type: 'string',
    name: 'bin',
    argName: 'path/to/flow',
    description: 'Path to the flow binary',
  } as Flag,
  flowconfigName: {
    type: 'string',
    name: 'flowconfigName',
    argName: '.flowconfig',
    description: 'Name of the flowconfig to use in checking',
    default: '.flowconfig',
  } as Flag,
  parallelism: {
    type: 'string',
    name: 'parallelism',
    argName: 'N',
    description: 'Number of tests to run in parallel',
    aliases: ['p'],
    default: '16',
  } as Flag,
  errorCheckCommand: {
    type: 'enum',
    name: 'check',
    argName: 'COMMAND',
    description: 'The flow command to check flow errors',
    validValues: ['check', 'full-check', 'status'],
    aliases: ['c'],
    default: 'status',
  } as Flag,
};

class ShowUsageException {
  exitCode: number;

  constructor(exitCode: number) {
    this.exitCode = exitCode;
  }
}

class Base<T extends interface {}, Input extends interface {} = interface {}> {
  static BAD_ARGS: number = 64;
  static OK: number = 0;

  // abstract
  static processArgv(argv: Input): T {
    throw new Error('Unimplemented abstract method: parse');
  }

  // abstract
  static async run(args: T): Promise<void> {
    throw new Error('Unimplemented abstract method: run');
  }

  static description(): string {
    throw new Error('Unimplemented abstract method: description');
  }

  // abstract
  static async usage(): Promise<string> {
    throw new Error('Unimplemented abstract method: usage');
  }

  // override
  static getFlags(): Array<Flag> {
    return [];
  }

  static getAllFlags(): Array<Flag> {
    return [].concat(this.getFlags(), {
      type: 'boolean',
      name: 'help',
      description: 'Shows this usage message',
      aliases: ['h'],
    });
  }

  // final
  static parse(): any {
    const boolean = [];
    const string = [];
    const defaults: {[key: string]: unknown} = {};
    const alias: {[key: string]: string | Array<string>} = {};

    const flags = this.getAllFlags();
    for (const flag of flags) {
      if (flag.type === 'string' || flag.type === 'enum') {
        string.push(flag.name);
        flag.default !== undefined && (defaults[flag.name] = flag.default);
      } else {
        boolean.push(flag.name);
      }
      if (flag.aliases !== undefined) {
        for (const aliasName of flag.aliases) {
          alias[aliasName] = flag.name;
        }
      }
    }

    const argv = parseArgs(process.argv.slice(3), {
      boolean,
      string,
      alias,
      default: defaults,
      unknown: flag => {
        if (flag.match(/^-/)) {
          process.stderr.write(format('Unsupported flag:', flag, '\n'));
          this.showUsage(this.BAD_ARGS);
        }
        return true;
      },
    });
    for (const flag of flags) {
      if (flag.type === 'string' || flag.type === 'enum') {
        if (argv[flag.name] === '') {
          (process.stderr.write(
            format('Missing required argument for flag: %s\n', flag.name),
          ),
            this.showUsage(this.BAD_ARGS));
        }
      }
      if (flag.type === 'enum') {
        if (flag.validValues.find(v => argv[flag.name] === v) === undefined) {
          process.stderr.write(
            format(
              '%s is not a support value for enum flag %s\n',
              argv[flag.name],
              flag.name,
            ),
          );
          this.showUsage(this.BAD_ARGS);
        }
      }
    }
    return argv;
  }

  // final
  static showUsage(exitCode: number): empty {
    throw new ShowUsageException(exitCode);
  }

  static async displayUsage(exn: ShowUsageException) {
    let usage = await this.usage();

    const flags = this.getAllFlags().sort((a, b) =>
      a.name.localeCompare(b.name),
    );

    usage += '\n\nOPTIONS:';
    for (const flag of flags) {
      const arg =
        flag.type === 'string' || flag.type === 'enum'
          ? ' ' + flag.argName
          : '';
      const values =
        flag.type === 'enum'
          ? format(
              ' [%s]',
              flag.validValues.map(x => format('"%s"', x)).join(', '),
            )
          : '';
      const defaultDesc =
        flag.default !== undefined
          ? format(' (default: "%s")', flag.default)
          : '';
      const names = []
        .concat(flag.name, flag.aliases || [])
        .map(name => (name.length == 1 ? '-' : '--') + name + arg)
        .join(', ');
      usage += '\n    ' + names;
      usage += '\n        ' + flag.description + values + defaultDesc;
    }

    usage += '\n';

    process.stderr.write(usage);
    process.exit(exn.exitCode);
  }

  // final
  static async go(): Promise<void> {
    try {
      const argv = this.parse();
      if (argv.help) {
        this.showUsage(this.OK);
      } else {
        await this.run(this.processArgv(argv));
      }
    } catch (e) {
      if (e instanceof ShowUsageException) {
        await this.displayUsage(e);
      } else {
        throw e;
      }
    }
  }
}

module.exports = {commonFlags, default: Base};
