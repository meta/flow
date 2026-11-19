/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict
 * @format
 */

// Adapted from https://github.com/DefinitelyTyped/DefinitelyTyped/blob/3503bd8d80d4c5ebc9673eb1a16335896e10f47b/types/prettier/index.d.ts

declare module 'prettier' {
  // declare export type LiteralUnion<T extends U, U = string> = T | (Pick<U, never> & { _?: never | void });

  declare export type AST = any;

  // https://github.com/prettier/prettier/blob/main/src/common/ast-path.js

  declare export class AstPath<T = any> {
    constructor(value: T): AstPath<T>;
    stack: Array<T>;
    getName(): string | number | null;
    getValue(): T;
    getNode(count?: number): T | null;
    getParentNode(count?: number): T | null;
    call<U>(callback: (path: this) => U, ...names: Array<string | number>): U;
    callParent<U>(callback: (path: this) => U, count?: number): U;
    each(
      callback: (path: this, index: number, value: any) => void,
      ...names: Array<string | number>
    ): void;
    map<U>(
      callback: (path: this, index: number, value: any) => U,
      ...names: Array<string | number>
    ): Array<U>;
    match(
      ...predicates: Array<
        (node: any, name: string | null, number: number | null) => boolean,
      >
    ): boolean;
  }

  declare export type BuiltInParser = (text: string, options?: any) => AST;
  declare export type BuiltInParserName =
    | 'angular'
    | 'babel-ts'
    | 'babel'
    | 'css'
    | 'espree'
    | 'flow'
    | 'glimmer'
    | 'graphql'
    | 'html'
    | 'json-stringify'
    | 'json'
    | 'json5'
    | 'less'
    | 'lwc'
    | 'markdown'
    | 'mdx'
    | 'meriyah'
    | 'scss'
    | 'typescript'
    | 'vue'
    | 'yaml';
  declare export type BuiltInParsers = {
    [key: BuiltInParserName]: BuiltInParser,
  };

  declare export type CustomParser = (
    text: string,
    parsers: BuiltInParsers,
    options: Options,
  ) => AST;

  declare type CustomParserName = string;

  declare export type Options = Partial<RequiredOptions>;
  declare export type RequiredOptions = {
    ...DocPrinterOptions,
    /**
     * Print semicolons at the ends of statements.
     * @default true
     */
    semi: boolean,
    /**
     * Use single quotes instead of double quotes.
     * @default false
     */
    singleQuote: boolean,
    /**
     * Use single quotes in JSX.
     * @default false
     */
    jsxSingleQuote: boolean,
    /**
     * Print trailing commas wherever possible.
     * @default 'es5'
     */
    trailingComma: 'none' | 'es5' | 'all',
    /**
     * Print spaces between brackets in object literals.
     * @default true
     */
    bracketSpacing: boolean,
    /**
     * Put the `>` of a multi-line HTML (HTML, JSX, Vue, Angular) element at the end of the last line instead of being
     * alone on the next line (does not apply to self closing elements).
     * @default false
     */
    bracketSameLine: boolean,
    /**
     * Put the `>` of a multi-line JSX element at the end of the last line instead of being alone on the next line.
     * @default false
     * @deprecated use bracketSameLine instead
     */
    jsxBracketSameLine: boolean,
    /**
     * Format only a segment of a file.
     * @default 0
     */
    rangeStart: number,
    /**
     * Format only a segment of a file.
     * @default Infinity
     */
    rangeEnd: number,
    /**
     * Specify which parser to use.
     */
    parser: BuiltInParserName | CustomParserName | CustomParser,
    /**
     * Specify the input filepath. This will be used to do parser inference.
     */
    filepath: string,
    /**
     * Prettier can restrict itself to only format files that contain a special comment, called a pragma, at the top of the file.
     * This is very useful when gradually transitioning large, unformatted codebases to prettier.
     * @default false
     */
    requirePragma: boolean,
    /**
     * Prettier can insert a special @format marker at the top of files specifying that
     * the file has been formatted with prettier. This works well when used in tandem with
     * the --require-pragma option. If there is already a docblock at the top of
     * the file then this option will add a newline to it with the @format marker.
     * @default false
     */
    insertPragma: boolean,
    /**
     * By default, Prettier will wrap markdown text as-is since some services use a linebreak-sensitive renderer.
     * In some cases you may want to rely on editor/viewer soft wrapping instead, so this option allows you to opt out.
     * @default 'preserve'
     */
    proseWrap: 'always' | 'never' | 'preserve',
    /**
     * Include parentheses around a sole arrow function parameter.
     * @default 'always'
     */
    arrowParens: 'avoid' | 'always',
    /**
     * Provide ability to support new languages to prettier.
     */
    plugins: Array<string | Plugin<>>,
    /**
     * Specify plugin directory paths to search for plugins if not installed in the same `node_modules` where prettier is located.
     */
    pluginSearchDirs: Array<string>,
    /**
     * How to handle whitespaces in HTML.
     * @default 'css'
     */
    htmlWhitespaceSensitivity: 'css' | 'strict' | 'ignore',
    /**
     * Which end of line characters to apply.
     * @default 'lf'
     */
    endOfLine: 'auto' | 'lf' | 'crlf' | 'cr',
    /**
     * Change when properties in objects are quoted.
     * @default 'as-needed'
     */
    quoteProps: 'as-needed' | 'consistent' | 'preserve',
    /**
     * Whether or not to indent the code inside <script> and <style> tags in Vue files.
     * @default false
     */
    vueIndentScriptAndStyle: boolean,
    /**
     * Control whether Prettier formats quoted code embedded in the file.
     * @default 'auto'
     */
    embeddedLanguageFormatting: 'auto' | 'off',
  };

  declare export type PrettierParserOptions<T = any> = {
    ...RequiredOptions,
    locStart: (node: T) => number,
    locEnd: (node: T) => number,
    originalText: string,
  };

  declare export type PrettierPrinterOptions<T = any> = {
    ...PrettierParserOptions<T>,
    printer: Printer<T>,
  };

  declare export type Plugin<T = any> = {
    languages?: Array<SupportLanguage> | void,
    parsers?: {[parserName: string]: Parser<T>} | void,
    printers?: {[astFormat: string]: Printer<T>} | void,
    options?: SupportOptions | void,
    defaultOptions?: Options | void,
  };

  declare export type Parser<T = any> = {
    parse: (
      text: string,
      parsers: {[parserName: string]: Parser<>},
      options: PrettierParserOptions<T>,
    ) => T,
    astFormat: string,
    hasPragma?: ((text: string) => boolean) | void,
    locStart: (node: T) => number,
    locEnd: (node: T) => number,
    preprocess?:
      ((text: string, options: PrettierParserOptions<T>) => string) | void,
  };

  declare export type Printer<T = any> = {
    print: (
      path: AstPath<T>,
      options: PrettierPrinterOptions<T>,
      print: (path: AstPath<T>) => Doc,
    ) => Doc,
    embed?:
      | ((
          path: AstPath<T>,
          print: (path: AstPath<T>) => Doc,
          textToDoc: (text: string, options: Options) => Doc,
          options: PrettierPrinterOptions<T>,
        ) => Doc | null)
      | void,
    insertPragma?: ((text: string) => string) | void,
    /**
     * @returns `null` if you want to remove this node
     * @returns `void` if you want to use modified newNode
     * @returns anything if you want to replace the node with it
     */
    massageAstNode?: ((node: any, newNode: any, parent: any) => any) | void,
    hasPrettierIgnore?: ((path: AstPath<T>) => boolean) | void,
    canAttachComment?: ((node: T) => boolean) | void,
    willPrintOwnComments?: ((path: AstPath<T>) => boolean) | void,
    printComment?:
      | ((commentPath: AstPath<T>, options: PrettierParserOptions<T>) => Doc)
      | void,
    handleComments?: {
      ownLine?:
        | ((
            commentNode: any,
            text: string,
            options: PrettierParserOptions<T>,
            ast: T,
            isLastComment: boolean,
          ) => boolean)
        | void,
      endOfLine?:
        | ((
            commentNode: any,
            text: string,
            options: PrettierParserOptions<T>,
            ast: T,
            isLastComment: boolean,
          ) => boolean)
        | void,
      remaining?:
        | ((
            commentNode: any,
            text: string,
            options: PrettierParserOptions<T>,
            ast: T,
            isLastComment: boolean,
          ) => boolean)
        | void,
    } | void,
  };

  declare export type CursorOptions = {
    ...Options,
    /**
     * Specify where the cursor is.
     */
    cursorOffset: number,
    rangeStart?: mixed,
    rangeEnd?: mixed,
  };

  declare export type CursorResult = {
    formatted: string,
    cursorOffset: number,
  };

  /**
   * `format` is used to format text using Prettier. [Options](https://prettier.io/docs/en/options.html) may be provided to override the defaults.
   */
  declare export function format(
    source: string,
    options?: Options,
  ): Promise<string> | string;

  /**
   * `check` checks to see if the file has been formatted with Prettier given those options and returns a `Boolean`.
   * This is similar to the `--list-different` parameter in the CLI and is useful for running Prettier in CI scenarios.
   */
  declare export function check(
    source: string,
    options?: Options,
  ): Promise<boolean> | boolean;

  /**
   * `formatWithCursor` both formats the code, and translates a cursor position from unformatted code to formatted code.
   * This is useful for editor integrations, to prevent the cursor from moving when code is formatted.
   *
   * The `cursorOffset` option should be provided, to specify where the cursor is. This option cannot be used with `rangeStart` and `rangeEnd`.
   */
  declare export function formatWithCursor(
    source: string,
    options: CursorOptions,
  ): Promise<CursorResult> | CursorResult;

  declare export type ResolveConfigOptions = {
    /**
     * If set to `false`, all caching will be bypassed.
     */
    useCache?: boolean | void,
    /**
     * Pass directly the path of the config file if you don't wish to search for it.
     */
    config?: string | void,
    /**
     * If set to `true` and an `.editorconfig` file is in your project,
     * Prettier will parse it and convert its properties to the corresponding prettier configuration.
     * This configuration will be overridden by `.prettierrc`, etc. Currently,
     * the following EditorConfig properties are supported:
     * - indent_style
     * - indent_size/tab_width
     * - max_line_length
     */
    editorconfig?: boolean | void,
  };

  /**
   * `resolveConfig` can be used to resolve configuration for a given source file,
   * passing its path as the first argument. The config search will start at the
   * file path and continue to search up the directory.
   * (You can use `process.cwd()` to start searching from the current directory).
   *
   * A promise is returned which will resolve to:
   *
   *  - An options object, providing a [config file](https://prettier.io/docs/en/configuration.html) was found.
   *  - `null`, if no file was found.
   *
   * The promise will be rejected if there was an error parsing the configuration file.
   */
  declare export var resolveConfig: {
    (filePath: string, options?: ResolveConfigOptions): Promise<Options | null>,
    sync(filePath: string, options?: ResolveConfigOptions): Options | null,
  };

  /**
   * `resolveConfigFile` can be used to find the path of the Prettier configuration file,
   * that will be used when resolving the config (i.e. when calling `resolveConfig`).
   *
   * A promise is returned which will resolve to:
   *
   * - The path of the configuration file.
   * - `null`, if no file was found.
   *
   * The promise will be rejected if there was an error parsing the configuration file.
   */
  declare export var resolveConfigFile: {
    (filePath?: string): Promise<string | null>,
    sync(filePath?: string): string | null,
  };

  /**
   * As you repeatedly call `resolveConfig`, the file system structure will be cached for performance. This function will clear the cache.
   * Generally this is only needed for editor integrations that know that the file system has changed since the last format took place.
   */
  declare export function clearConfigCache(): void;

  declare export type SupportLanguage = {
    name: string,
    since?: string | void,
    parsers: Array<BuiltInParserName> | Array<string>,
    group?: string | void,
    tmScope?: string | void,
    aceMode?: string | void,
    codemirrorMode?: string | void,
    codemirrorMimeType?: string | void,
    aliases?: Array<string> | void,
    extensions?: Array<string> | void,
    filenames?: Array<string> | void,
    linguistLanguageId?: number | void,
    vscodeLanguageIds?: Array<string> | void,
  };

  declare export type SupportOptionRange = {
    start: number,
    end: number,
    step: number,
  };

  declare export type SupportOptionType = 'int' | 'boolean' | 'choice' | 'path';

  declare export type BaseSupportOption<Type: SupportOptionType> = {
    readonly name?: string | void,
    since: string,
    /**
     * Usually you can use {@link CoreCategoryType}
     */
    category: string,
    /**
     * The type of the option.
     *
     * When passing a type other than the ones listed below, the option is
     * treated as taking any string as argument, and `--option <${type}>` will
     * be displayed in --help.
     */
    type: Type,
    /**
     * Indicate that the option is deprecated.
     *
     * Use a string to add an extra message to --help for the option,
     * for example to suggest a replacement option.
     */
    deprecated?: true | string | void,
    /**
     * Description to be displayed in --help. If omitted, the option won't be
     * shown at all in --help.
     */
    description?: string | void,
  };

  declare export type IntSupportOption = {
    ...BaseSupportOption<'int'>,
    default?: number | void,
    array?: false | void,
    range?: SupportOptionRange | void,
  };

  declare export type IntArraySupportOption = {
    ...BaseSupportOption<'int'>,
    default?: Array<{value: Array<number>}> | void,
    array: true,
  };

  declare export type BooleanSupportOption = {
    ...BaseSupportOption<'boolean'>,
    default?: boolean | void,
    array?: false | void,
    description: string,
    oppositeDescription?: string | void,
  };

  declare export type BooleanArraySupportOption = {
    ...BaseSupportOption<'boolean'>,
    default?: Array<{value: Array<boolean>}> | void,
    array: true,
  };

  declare export type ChoiceSupportOption<Value = any> = {
    ...BaseSupportOption<'choice'>,
    default?: Value | Array<{since: string, value: Value}> | void,
    description: string,
    choices: Array<{
      since?: string | void,
      value: Value,
      description: string,
    }>,
  };

  declare export type PathSupportOption = {
    ...BaseSupportOption<'path'>,
    default?: string | void,
    array?: false | void,
  };

  declare export type PathArraySupportOption = {
    ...BaseSupportOption<'path'>,
    default?: Array<{value: Array<string>}> | void,
    array: true,
  };

  declare export type SupportOption =
    | IntSupportOption
    | IntArraySupportOption
    | BooleanSupportOption
    | BooleanArraySupportOption
    | ChoiceSupportOption<>
    | PathSupportOption
    | PathArraySupportOption;

  declare export type SupportOptions = {
    [key: string]: SupportOption,
  };

  declare export type SupportInfo = {
    languages: Array<SupportLanguage>,
    options: Array<SupportOption>,
  };

  declare export type FileInfoOptions = {
    ignorePath?: string | void,
    withNodeModules?: boolean | void,
    plugins?: Array<string> | void,
    resolveConfig?: boolean | void,
  };

  declare export type FileInfoResult = {
    ignored: boolean,
    inferredParser: string | null,
  };

  declare export var getFileInfo: {
    (filePath: string, options?: FileInfoOptions): Promise<FileInfoResult>,
    sync(filePath: string, options?: FileInfoOptions): FileInfoResult,
  };

  /**
   * Returns an object representing the parsers, languages and file types Prettier supports for the current version.
   */
  declare export function getSupportInfo(): SupportInfo;

  /**
   * `version` field in `package.json`
   */
  declare export var version: string;

  declare type Quote = "'" | '"';
  declare type SkipOptions = {
    backwards?: boolean | void,
  };
  // https://github.com/prettier/prettier/blob/main/src/common/util-shared.js
  declare export var util: {
    addDanglingComment(node: any, comment: any, marker: any): void,
    addLeadingComment(node: any, comment: any): void,
    addTrailingComment(node: any, comment: any): void,
    getAlignmentSize(
      value: string,
      tabWidth: number,
      startIndex?: number,
    ): number,
    getIndentSize(value: string, tabWidth: number): number,
    getMaxContinuousCount(str: string, target: string): number,
    getNextNonSpaceNonCommentCharacterIndex<N>(
      text: string,
      node: N,
      locEnd: (node: N) => number,
    ): number | false,
    getStringWidth(text: string): number,
    hasNewline(text: string, index: number, opts?: SkipOptions): boolean,
    hasNewlineInRange(text: string, start: number, end: number): boolean,
    hasSpaces(text: string, index: number, opts?: SkipOptions): boolean,
    isNextLineEmpty<N>(
      text: string,
      node: N,
      locEnd: (node: N) => number,
    ): boolean,
    isNextLineEmptyAfterIndex(text: string, index: number): boolean,
    isPreviousLineEmpty<N>(
      text: string,
      node: N,
      locStart: (node: N) => number,
    ): boolean,
    makeString(
      rawContent: string,
      enclosingQuote: Quote,
      unescapeUnnecessaryEscapes?: boolean,
    ): string,
    skip(
      chars: string | RegExp,
    ): (
      text: string,
      index: number | false,
      opts?: SkipOptions,
    ) => number | false,
    skipEverythingButNewLine(
      text: string,
      index: number | false,
      opts?: SkipOptions,
    ): number | false,
    skipInlineComment(text: string, index: number | false): number | false,
    skipNewline(
      text: string,
      index: number | false,
      opts?: SkipOptions,
    ): number | false,
    skipSpaces(
      text: string,
      index: number | false,
      opts?: SkipOptions,
    ): number | false,
    skipToLineEnd(
      text: string,
      index: number | false,
      opts?: SkipOptions,
    ): number | false,
    skipTrailingComment(text: string, index: number | false): number | false,
    skipWhitespace(
      text: string,
      index: number | false,
      opts?: SkipOptions,
    ): number | false,
  };

  // https://github.com/prettier/prettier/blob/main/src/document/index.js
  declare type DocCommand =
    | Align
    | BreakParent
    | Concat
    | Cursor
    | Fill
    | Group
    | IfBreak
    | Indent
    | IndentIfBreak
    | Label
    | Line
    | LineSuffix
    | LineSuffixBoundary
    | Trim;
  declare type Doc = string | Array<Doc> | DocCommand;

  declare type Align = {
    type: 'align',
    contents: Doc,
    n: number | string | {type: 'root'},
  };

  declare type BreakParent = {
    type: 'break-parent',
  };

  declare type Concat = {
    type: 'concat',
    parts: Array<Doc>,
  };

  declare type Cursor = {
    type: 'cursor',
    placeholder: symbol,
  };

  declare type Fill = {
    type: 'fill',
    parts: Array<Doc>,
  };

  declare type Group = {
    type: 'group',
    contents: Doc,
    break: boolean,
    expandedStates: Array<Doc>,
  };

  declare type HardlineWithoutBreakParent = {
    ...Line,
    hard: true,
  };

  declare type IfBreak = {
    type: 'if-break',
    breakContents: Doc,
    flatContents: Doc,
  };

  declare type Indent = {
    type: 'indent',
    contents: Doc,
  };

  declare type IndentIfBreak = {
    type: 'indent-if-break',
  };

  declare type Label = {
    type: 'label',
  };

  declare type Line = {
    type: 'line',
    soft?: boolean | void,
    hard?: boolean | void,
    literal?: boolean | void,
  };

  declare type LineSuffix = {
    type: 'line-suffix',
    contents: Doc,
  };

  declare type LineSuffixBoundary = {
    type: 'line-suffix-boundary',
  };

  declare type LiterallineWithoutBreakParent = {
    ...Line,
    hard: true,
    literal: true,
  };

  declare type Softline = {
    ...Line,
    soft: true,
  };

  declare type Trim = {
    type: 'trim',
  };

  declare type GroupOptions = {
    shouldBreak?: boolean | void,
    id?: symbol | void,
  };

  declare type DocPrinterOptions = {
    /**
     * Specify the line length that the printer will wrap on.
     * @default 80
     */
    printWidth: number,
    /**
     * Specify the number of spaces per indentation-level.
     * @default 2
     */
    tabWidth: number,
    /**
     * Indent lines with tabs instead of spaces
     * @default false
     */
    useTabs: boolean,
    parentParser?: string | void,
    __embeddedInHtml?: boolean | void,
  };
  declare var doc: {
    builders: {
      addAlignmentToDoc(doc: Doc, size: number, tabWidth: number): Doc,
      /** @see [align](https://github.com/prettier/prettier/blob/main/commands.md#align) */
      align(widthOrString: Align['n'], doc: Doc): Align,
      /** @see [breakParent](https://github.com/prettier/prettier/blob/main/commands.md#breakparent) */
      readonly breakParent: BreakParent,
      /**
       * @see [concat](https://github.com/prettier/prettier/blob/main/commands.md#deprecated-concat)
       * @deprecated use `Doc[]` instead
       */
      concat(docs: Array<Doc>): Concat,
      /** @see [conditionalGroup](https://github.com/prettier/prettier/blob/main/commands.md#conditionalgroup) */
      conditionalGroup(alternatives: Array<Doc>, options?: GroupOptions): Group,
      /** @see [dedent](https://github.com/prettier/prettier/blob/main/commands.md#dedent) */
      dedent(doc: Doc): Align,
      /** @see [dedentToRoot](https://github.com/prettier/prettier/blob/main/commands.md#dedenttoroot) */
      dedentToRoot(doc: Doc): Align,
      /** @see [fill](https://github.com/prettier/prettier/blob/main/commands.md#fill) */
      fill(docs: Array<Doc>): Fill,
      /** @see [group](https://github.com/prettier/prettier/blob/main/commands.md#group) */
      group(doc: Doc, opts?: GroupOptions): Group,
      /** @see [hardline](https://github.com/prettier/prettier/blob/main/commands.md#hardline) */
      readonly hardline: Concat,
      /** @see [hardlineWithoutBreakParent](https://github.com/prettier/prettier/blob/main/commands.md#hardlinewithoutbreakparent-and-literallinewithoutbreakparent) */
      readonly hardlineWithoutBreakParent: HardlineWithoutBreakParent,
      /** @see [ifBreak](https://github.com/prettier/prettier/blob/main/commands.md#ifbreak) */
      ifBreak(
        ifBreak: Doc,
        noBreak?: Doc,
        options?: {groupId?: symbol | void},
      ): IfBreak,
      /** @see [indent](https://github.com/prettier/prettier/blob/main/commands.md#indent) */
      indent(doc: Doc): Indent,
      /** @see [indentIfBreak](https://github.com/prettier/prettier/blob/main/commands.md#indentifbreak) */
      indentIfBreak(
        doc: Doc,
        opts: {groupId: symbol, negate?: boolean | void},
      ): IndentIfBreak,
      /** @see [join](https://github.com/prettier/prettier/blob/main/commands.md#join) */
      join(sep: Doc, docs: Array<Doc>): Concat,
      /** @see [label](https://github.com/prettier/prettier/blob/main/commands.md#label) */
      label(label: string, doc: Doc): Label,
      /** @see [line](https://github.com/prettier/prettier/blob/main/commands.md#line) */
      readonly line: Line,
      /** @see [lineSuffix](https://github.com/prettier/prettier/blob/main/commands.md#linesuffix) */
      lineSuffix(suffix: Doc): LineSuffix,
      /** @see [lineSuffixBoundary](https://github.com/prettier/prettier/blob/main/commands.md#linesuffixboundary) */
      readonly lineSuffixBoundary: LineSuffixBoundary,
      /** @see [literalline](https://github.com/prettier/prettier/blob/main/commands.md#literalline) */
      readonly literalline: Concat,
      /** @see [literallineWithoutBreakParent](https://github.com/prettier/prettier/blob/main/commands.md#hardlinewithoutbreakparent-and-literallinewithoutbreakparent) */
      readonly literallineWithoutBreakParent: LiterallineWithoutBreakParent,
      /** @see [markAsRoot](https://github.com/prettier/prettier/blob/main/commands.md#markasroot) */
      markAsRoot(doc: Doc): Align,
      /** @see [softline](https://github.com/prettier/prettier/blob/main/commands.md#softline) */
      readonly softline: Softline,
      /** @see [trim](https://github.com/prettier/prettier/blob/main/commands.md#trim) */
      readonly trim: Trim,
      /** @see [cursor](https://github.com/prettier/prettier/blob/main/commands.md#cursor) */
      readonly cursor: Cursor,
    },
    debug: {
      printDocToDebug(doc: Doc): string,
    },
    printer: {
      printDocToString(
        doc: Doc,
        options: DocPrinterOptions,
      ): {
        formatted: string,
        cursorNodeStart?: number | void,
        cursorNodeText?: string | void,
      },
    },
    utils: {
      cleanDoc(doc: Doc): Doc,
      findInDoc<T = Doc>(
        doc: Doc,
        callback: (doc: Doc) => T,
        defaultValue: T,
      ): T,
      getDocParts(doc: Doc): Doc,
      isConcat(doc: Doc): boolean,
      isEmpty(doc: Doc): boolean,
      isLineNext(doc: Doc): boolean,
      mapDoc<T = Doc>(doc: Doc, callback: (doc: Doc) => T): T,
      normalizeDoc(doc: Doc): Doc,
      normalizeParts(parts: Array<Doc>): Array<Doc>,
      propagateBreaks(doc: Doc): void,
      removeLines(doc: Doc): Doc,
      replaceNewlinesWithLiterallines(doc: Doc): Doc,
      stripTrailingHardline(doc: Doc): Doc,
      traverseDoc(
        doc: Doc,
        onEnter?: (doc: Doc) => void | boolean,
        onExit?: (doc: Doc) => void,
        shouldTraverseConditionalGroups?: boolean,
      ): void,
      willBreak(doc: Doc): boolean,
    },
  };
}

declare module 'prettier/plugins/flow' {
  declare type AST = any;

  declare export var parsers: {
    flow: {
      astFormat: string,
      parse(text: string, parsers: any, options: any): AST,
      hasPragma?: ((text: string) => boolean) | void,
      locStart: (node: AST) => number,
      locEnd: (node: AST) => number,
      preprocess?: ((text: string, options: any) => string) | void,
    },
  };
}

declare module 'prettier/plugins/estree' {
  declare export var printers: {[key: string]: any};
}

declare module 'prettier/plugins/typescript' {
  declare type AST = any;

  declare export var parsers: {
    typescript: {
      astFormat: string,
      parse(text: string, parsers: any, options: any): AST,
      hasPragma?: ((text: string) => boolean) | void,
      locStart: (node: AST) => number,
      locEnd: (node: AST) => number,
      preprocess?: ((text: string, options: any) => string) | void,
    },
  };
}
