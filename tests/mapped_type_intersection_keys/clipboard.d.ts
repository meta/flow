// tsc accepts. The shape of @lexical/clipboard's ClipboardImportExtension.d.ts.
export interface ClipboardData {
  'text/html'?: string | undefined;
  'text/plain': string;
  [mimeType: string & {}]: string | undefined;
}
export type MimeTypeConfig = {
  [key in keyof ClipboardData | (string & {})]?: number | undefined;
};
export type MimeTypePriority = {
  readonly [key in keyof ClipboardData | (string & {})]?: number | undefined;
};
export declare const DEFAULT_MIME_TYPE_CONFIG: MimeTypeConfig;
export declare const DEFAULT_MIME_TYPE_PRIORITY: MimeTypePriority;
