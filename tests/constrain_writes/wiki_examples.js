import * as React from 'react';

function ex1() {
  type Props = { prop: string };

  declare const x: number;
  declare const y: number;
  declare const props: Props;

  let product = Math.sqrt(x) + y;
  // `product` has type `number`
  let Component = ({prop}: Props) => { return <>{prop}</> }
  // `Component` has type`React.ComponentType<Props>`
  let element = <Component {...props} />
  // `element` has type renders Fragment

  type OtherProps = { other_prop: string };
  declare const OtherComponent: (props: OtherProps) => React.Node;
  declare const other_props: OtherProps

  product = "Our new product is..."; // Error
  Component = ({other_prop}: OtherProps) => { return <>{other_prop}</> }; // Error
  element = <OtherComponent {...other_props} />; // Error
}

function ex2() {
  type Props = { prop: string };

  declare const x: number;
  declare const y: number;
  declare const props: Props;

  let product: number | string = Math.sqrt(x) + y;

  let Component: unknown = ({prop}: Props) => { return <>{prop}</> }
  // No good type to represent this! Restructure code instead

  let element: React.Node = <></>


  type OtherProps = { other_prop: string };
  declare const OtherComponent: (props: OtherProps) => React.Node;
  declare const other_props: OtherProps

  product = "Our new product is...";
  Component = ({other_prop}: OtherProps) => { return <>{other_prop}</> };
  element = <OtherComponent {...other_props} />;
}

function ex3() {
  let topLevelAssigned;

  function helper() {
    topLevelAssigned = 42; // Error `topLevelAssigned` has type `string`
  }

  topLevelAssigned = "Hello world"; // This write determines the var's type
  topLevelAssigned = true; // Error `topLevelAssigned` has type `string`
}

function ex4() {
  declare const condition: boolean;

  let myNumberOrString;

  if (condition) {
    myNumberOrString = 42;
  } else {
    myNumberOrString = "Hello world";
  }

  myNumberOrString = 21; // fine
  myNumberOrString = "Goodbye"; // fine
  myNumberOrString = false; // Error `myNumberOrString` has type `number | string`
}

function ex5() {
  declare const condition: boolean;

  let oneBranchAssigned;

  if (condition) {
    oneBranchAssigned = "Hello world!";
  }

  oneBranchAssigned.toUpperCase(); // Error: `oneBranchAssigned` may be uninitialized
  oneBranchAssigned = 42; // Error `oneBranchAssigned` has type `string`
}

function ex6() {
  function findIDValue<T>(dict: {[key: string]: T}): T {
    let idVal = null; // initialized as `null`
    for (const key in dict) {
      if (key === 'ID') {
        idVal = dict[key]; // Infer that `idVal` has type `null | T`
      }
    }
    if (idVal === null) {
      throw new Error("No entry for ID!");
    }
    return idVal;
  }
}
