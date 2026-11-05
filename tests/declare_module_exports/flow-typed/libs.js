declare module "declare_module_exports" {
  declare module.exports: number;
}

declare module "declare_m_e_with_other_value_declares" {
  declare module.exports: number;
  declare const str: string;
}

declare module "declare_m_e_with_other_type_declares" {
  declare module.exports: number;
  declare type str2 = string;
}

/**
 * `declare var exports` used to determine the type of exports of the module,
 * but now behaves just like `declare var foo`, s/foo/exports.
 */

declare module "declare_var_exports" {
  declare var exports: number;
}

/**
 * Ensure that, if both are present, `declare module.exports` wins
 */
declare module "declare_m_e_with_declare_var_e" {
  declare module.exports: number;
  declare var exports: string;
}

/**
 * Ensure that the intersection of the two declarations is exported.
 */
declare module "declare_overloaded_function" {
  declare function foo(x: string): string;
  declare function foo(x: number): number;
}

/**
 * Properties of a `module.exports` with several call signatures are named
 * exports, as they are with one.
 */
declare module "declare_m_e_with_call_signatures" {
  declare module.exports: {
    (x: number): number;
    (x: string): string;
    ok: number;
  };
}

/**
 * Namespace members of an overloaded function are named exports.
 */
declare module "declare_m_e_overloaded_function_and_namespace" {
  declare function f(x: number): number;
  declare function f(x: string): string;
  declare namespace f {
    declare var ok: number;
  }
  declare module.exports: typeof f;
}
