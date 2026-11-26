/**
 * @format
 * @flow
 */

declare const any: any;

any as ?number as number;
any as ?number as null;
any as ?number as undefined;

any as number as ?number;
any as null as ?number;
any as undefined as ?number;

any as ?number as number | null;
any as ?number as number | undefined;

any as number | null as ?number;
any as number | undefined as ?number;
