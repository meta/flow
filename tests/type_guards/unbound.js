function no_params(): x is number { // error x unbound
  return true;
}

function unbound_type_guard_param(x: number): y is number {  // error y is not a param
  return true;
}

type No_param = () => x is number; // error x unbound

type Unbound_param = (y: number) => x is number; // error x unbound

type No_param_name = (n: number) => x is number; // error x unbound
