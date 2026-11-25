/*
 * @flow
 * @format
 */

import type {SuiteType} from '../Tester';
const {suite, test} = require('../Tester');

module.exports = (suite(({addFile, flowCmd}) => [
  test('prints each type once, so a recursive type is a cycle of refs', [
    addFile('cycle.js')
      .flowCmd([
        'dump-types',
        '--for-tool',
        '--json',
        '--graph',
        '--json-direct',
        '--strip-root',
        'cycle.js',
      ])
      .stdout(
        `
          {
            "$defs": {
              "0": {
                "def": {
                  "kind": "Type",
                  "type": {
                    "$ref": 5
                  },
                  "type_kind": "TypeAliasKind"
                },
                "kind": "Def",
                "reason": "cycle.js:1:6-9: \`Node\`"
              },
              "1": {
                "def": {
                  "kind": "Obj",
                  "objtype": {
                    "call_t": null,
                    "flags": {
                      "obj_kind": {
                        "kind": "Exact"
                      }
                    },
                    "props": {
                      "next": {
                        "kind": "Field",
                        "optional": false,
                        "polarity": "neutral",
                        "type": {
                          "$ref": 6
                        }
                      }
                    },
                    "proto_t": {
                      "$ref": 7
                    }
                  }
                },
                "kind": "Def",
                "reason": "cycle.js:1:13-25: object type"
              },
              "2": {
                "kind": "Union",
                "members": [
                  {
                    "$ref": 8
                  },
                  {
                    "$ref": 9
                  },
                  {
                    "$ref": 3
                  }
                ]
              },
              "3": {
                "def": {
                  "kind": "Obj",
                  "objtype": {
                    "call_t": null,
                    "flags": {
                      "obj_kind": {
                        "kind": "Exact"
                      }
                    },
                    "props": {
                      "next": {
                        "kind": "Field",
                        "optional": false,
                        "polarity": "neutral",
                        "type": {
                          "$ref": 6
                        }
                      }
                    },
                    "proto_t": {
                      "$ref": 7
                    }
                  }
                },
                "kind": "Def",
                "reason": "cycle.js:1:21-24: \`Node\`"
              },
              "4": {
                "def": {
                  "kind": "Mixed"
                },
                "kind": "Def",
                "reason": "cycle.js:1:6-9"
              },
              "5": {
                "def": {
                  "kind": "Obj",
                  "objtype": {
                    "call_t": null,
                    "flags": {
                      "obj_kind": {
                        "kind": "Exact"
                      }
                    },
                    "props": {
                      "next": {
                        "kind": "Field",
                        "optional": false,
                        "polarity": "neutral",
                        "type": {
                          "$ref": 6
                        }
                      }
                    },
                    "proto_t": {
                      "$ref": 7
                    }
                  }
                },
                "kind": "Def",
                "reason": "cycle.js:1:13-25: \`Node\`"
              },
              "6": {
                "kind": "Union",
                "members": [
                  {
                    "$ref": 10
                  },
                  {
                    "$ref": 8
                  },
                  {
                    "$ref": 9
                  }
                ]
              },
              "7": {
                "kind": "ObjProto"
              },
              "8": {
                "def": {
                  "kind": "Null"
                },
                "kind": "Def",
                "reason": "cycle.js:1:20-24: null or undefined"
              },
              "9": {
                "def": {
                  "kind": "Void"
                },
                "kind": "Def",
                "reason": "cycle.js:1:20-24: null or undefined"
              },
              "10": {
                "kind": "Annot",
                "type": {
                  "$ref": 5
                },
                "use_desc": false
              }
            },
            "schema": {
              "graph": true,
              "json_direct": true
            },
            "types": [
              {
                "end": 9,
                "endline": 1,
                "line": 1,
                "loc": {
                  "end": {
                    "column": 9,
                    "line": 1,
                    "offset": 9
                  },
                  "source": "cycle.js",
                  "start": {
                    "column": 6,
                    "line": 1,
                    "offset": 5
                  },
                  "type": "SourceFile"
                },
                "path": "cycle.js",
                "reasons": [],
                "start": 6,
                "type": {
                  "expression_type": {
                    "$ref": 0
                  }
                }
              },
              {
                "end": 25,
                "endline": 1,
                "line": 1,
                "loc": {
                  "end": {
                    "column": 25,
                    "line": 1,
                    "offset": 25
                  },
                  "source": "cycle.js",
                  "start": {
                    "column": 13,
                    "line": 1,
                    "offset": 12
                  },
                  "type": "SourceFile"
                },
                "path": "cycle.js",
                "reasons": [],
                "start": 13,
                "type": {
                  "expression_type": {
                    "$ref": 1
                  }
                }
              },
              {
                "end": 17,
                "endline": 1,
                "line": 1,
                "loc": {
                  "end": {
                    "column": 17,
                    "line": 1,
                    "offset": 17
                  },
                  "source": "cycle.js",
                  "start": {
                    "column": 14,
                    "line": 1,
                    "offset": 13
                  },
                  "type": "SourceFile"
                },
                "path": "cycle.js",
                "reasons": [],
                "start": 14,
                "type": {
                  "expression_type": {
                    "$ref": 2
                  }
                }
              },
              {
                "end": 24,
                "endline": 1,
                "line": 1,
                "loc": {
                  "end": {
                    "column": 24,
                    "line": 1,
                    "offset": 24
                  },
                  "source": "cycle.js",
                  "start": {
                    "column": 20,
                    "line": 1,
                    "offset": 19
                  },
                  "type": "SourceFile"
                },
                "path": "cycle.js",
                "reasons": [],
                "start": 20,
                "type": {
                  "expression_type": {
                    "$ref": 2
                  }
                }
              },
              {
                "end": 24,
                "endline": 1,
                "line": 1,
                "loc": {
                  "end": {
                    "column": 24,
                    "line": 1,
                    "offset": 24
                  },
                  "source": "cycle.js",
                  "start": {
                    "column": 21,
                    "line": 1,
                    "offset": 20
                  },
                  "type": "SourceFile"
                },
                "path": "cycle.js",
                "reasons": [],
                "start": 21,
                "type": {
                  "expression_type": {
                    "$ref": 3
                  },
                  "provider_type": {
                    "$ref": 4
                  }
                }
              },
              {
                "end": 24,
                "endline": 1,
                "line": 1,
                "loc": {
                  "end": {
                    "column": 24,
                    "line": 1,
                    "offset": 24
                  },
                  "source": "cycle.js",
                  "start": {
                    "column": 21,
                    "line": 1,
                    "offset": 20
                  },
                  "type": "SourceFile"
                },
                "path": "cycle.js",
                "reasons": [],
                "start": 21,
                "type": {
                  "expression_type": {
                    "$ref": 0
                  },
                  "provider_type": {
                    "$ref": 4
                  }
                }
              }
            ]
          }
        `,
        {normalize: ['json']},
      )
      .exitCodes([0]),
  ]),
  test('prints one entry for a type value the file refers to twice', [
    addFile('shared.js')
      .flowCmd([
        'dump-types',
        '--for-tool',
        '--json',
        '--graph',
        '--json-direct',
        '--strip-root',
        'shared.js',
      ])
      .stdout(
        `
          {
            "$defs": {
              "0": {
                "def": {
                  "kind": "Type",
                  "type": {
                    "$ref": 7
                  },
                  "type_kind": "TypeAliasKind"
                },
                "kind": "Def",
                "reason": "shared.js:1:6: \`T\`"
              },
              "1": {
                "def": {
                  "kind": "NumGeneral"
                },
                "kind": "Def",
                "reason": "shared.js:1:10-15: number"
              },
              "2": {
                "def": {
                  "kind": "Type",
                  "type": {
                    "$ref": 8
                  },
                  "type_kind": "TypeAliasKind"
                },
                "kind": "Def",
                "reason": "shared.js:2:6: \`U\`"
              },
              "3": {
                "def": {
                  "kind": "Obj",
                  "objtype": {
                    "call_t": null,
                    "flags": {
                      "obj_kind": {
                        "kind": "Exact"
                      }
                    },
                    "props": {
                      "p": {
                        "kind": "Field",
                        "optional": false,
                        "polarity": "neutral",
                        "type": {
                          "$ref": 9
                        }
                      },
                      "q": {
                        "kind": "Field",
                        "optional": false,
                        "polarity": "neutral",
                        "type": {
                          "$ref": 9
                        }
                      }
                    },
                    "proto_t": {
                      "$ref": 10
                    }
                  }
                },
                "kind": "Def",
                "reason": "shared.js:2:10-21: object type"
              },
              "4": {
                "def": {
                  "kind": "NumGeneral"
                },
                "kind": "Def",
                "reason": "shared.js:2:14: \`T\`"
              },
              "5": {
                "def": {
                  "kind": "Mixed"
                },
                "kind": "Def",
                "reason": "shared.js:1:6"
              },
              "6": {
                "def": {
                  "kind": "NumGeneral"
                },
                "kind": "Def",
                "reason": "shared.js:2:20: \`T\`"
              },
              "7": {
                "def": {
                  "kind": "NumGeneral"
                },
                "kind": "Def",
                "reason": "shared.js:1:10-15: \`T\`"
              },
              "8": {
                "def": {
                  "kind": "Obj",
                  "objtype": {
                    "call_t": null,
                    "flags": {
                      "obj_kind": {
                        "kind": "Exact"
                      }
                    },
                    "props": {
                      "p": {
                        "kind": "Field",
                        "optional": false,
                        "polarity": "neutral",
                        "type": {
                          "$ref": 9
                        }
                      },
                      "q": {
                        "kind": "Field",
                        "optional": false,
                        "polarity": "neutral",
                        "type": {
                          "$ref": 9
                        }
                      }
                    },
                    "proto_t": {
                      "$ref": 10
                    }
                  }
                },
                "kind": "Def",
                "reason": "shared.js:2:10-21: \`U\`"
              },
              "9": {
                "kind": "Annot",
                "type": {
                  "$ref": 7
                },
                "use_desc": false
              },
              "10": {
                "kind": "ObjProto"
              }
            },
            "schema": {
              "graph": true,
              "json_direct": true
            },
            "types": [
              {
                "end": 6,
                "endline": 1,
                "line": 1,
                "loc": {
                  "end": {
                    "column": 6,
                    "line": 1,
                    "offset": 6
                  },
                  "source": "shared.js",
                  "start": {
                    "column": 6,
                    "line": 1,
                    "offset": 5
                  },
                  "type": "SourceFile"
                },
                "path": "shared.js",
                "reasons": [],
                "start": 6,
                "type": {
                  "expression_type": {
                    "$ref": 0
                  }
                }
              },
              {
                "end": 15,
                "endline": 1,
                "line": 1,
                "loc": {
                  "end": {
                    "column": 15,
                    "line": 1,
                    "offset": 15
                  },
                  "source": "shared.js",
                  "start": {
                    "column": 10,
                    "line": 1,
                    "offset": 9
                  },
                  "type": "SourceFile"
                },
                "path": "shared.js",
                "reasons": [],
                "start": 10,
                "type": {
                  "expression_type": {
                    "$ref": 1
                  }
                }
              },
              {
                "end": 6,
                "endline": 2,
                "line": 2,
                "loc": {
                  "end": {
                    "column": 6,
                    "line": 2,
                    "offset": 23
                  },
                  "source": "shared.js",
                  "start": {
                    "column": 6,
                    "line": 2,
                    "offset": 22
                  },
                  "type": "SourceFile"
                },
                "path": "shared.js",
                "reasons": [],
                "start": 6,
                "type": {
                  "expression_type": {
                    "$ref": 2
                  }
                }
              },
              {
                "end": 21,
                "endline": 2,
                "line": 2,
                "loc": {
                  "end": {
                    "column": 21,
                    "line": 2,
                    "offset": 38
                  },
                  "source": "shared.js",
                  "start": {
                    "column": 10,
                    "line": 2,
                    "offset": 26
                  },
                  "type": "SourceFile"
                },
                "path": "shared.js",
                "reasons": [],
                "start": 10,
                "type": {
                  "expression_type": {
                    "$ref": 3
                  }
                }
              },
              {
                "end": 11,
                "endline": 2,
                "line": 2,
                "loc": {
                  "end": {
                    "column": 11,
                    "line": 2,
                    "offset": 28
                  },
                  "source": "shared.js",
                  "start": {
                    "column": 11,
                    "line": 2,
                    "offset": 27
                  },
                  "type": "SourceFile"
                },
                "path": "shared.js",
                "reasons": [],
                "start": 11,
                "type": {
                  "expression_type": {
                    "$ref": 4
                  }
                }
              },
              {
                "end": 14,
                "endline": 2,
                "line": 2,
                "loc": {
                  "end": {
                    "column": 14,
                    "line": 2,
                    "offset": 31
                  },
                  "source": "shared.js",
                  "start": {
                    "column": 14,
                    "line": 2,
                    "offset": 30
                  },
                  "type": "SourceFile"
                },
                "path": "shared.js",
                "reasons": [],
                "start": 14,
                "type": {
                  "expression_type": {
                    "$ref": 4
                  },
                  "provider_type": {
                    "$ref": 5
                  }
                }
              },
              {
                "end": 14,
                "endline": 2,
                "line": 2,
                "loc": {
                  "end": {
                    "column": 14,
                    "line": 2,
                    "offset": 31
                  },
                  "source": "shared.js",
                  "start": {
                    "column": 14,
                    "line": 2,
                    "offset": 30
                  },
                  "type": "SourceFile"
                },
                "path": "shared.js",
                "reasons": [],
                "start": 14,
                "type": {
                  "expression_type": {
                    "$ref": 0
                  },
                  "provider_type": {
                    "$ref": 5
                  }
                }
              },
              {
                "end": 17,
                "endline": 2,
                "line": 2,
                "loc": {
                  "end": {
                    "column": 17,
                    "line": 2,
                    "offset": 34
                  },
                  "source": "shared.js",
                  "start": {
                    "column": 17,
                    "line": 2,
                    "offset": 33
                  },
                  "type": "SourceFile"
                },
                "path": "shared.js",
                "reasons": [],
                "start": 17,
                "type": {
                  "expression_type": {
                    "$ref": 6
                  }
                }
              },
              {
                "end": 20,
                "endline": 2,
                "line": 2,
                "loc": {
                  "end": {
                    "column": 20,
                    "line": 2,
                    "offset": 37
                  },
                  "source": "shared.js",
                  "start": {
                    "column": 20,
                    "line": 2,
                    "offset": 36
                  },
                  "type": "SourceFile"
                },
                "path": "shared.js",
                "reasons": [],
                "start": 20,
                "type": {
                  "expression_type": {
                    "$ref": 6
                  },
                  "provider_type": {
                    "$ref": 5
                  }
                }
              },
              {
                "end": 20,
                "endline": 2,
                "line": 2,
                "loc": {
                  "end": {
                    "column": 20,
                    "line": 2,
                    "offset": 37
                  },
                  "source": "shared.js",
                  "start": {
                    "column": 20,
                    "line": 2,
                    "offset": 36
                  },
                  "type": "SourceFile"
                },
                "path": "shared.js",
                "reasons": [],
                "start": 20,
                "type": {
                  "expression_type": {
                    "$ref": 0
                  },
                  "provider_type": {
                    "$ref": 5
                  }
                }
              }
            ]
          }
        `,
        {normalize: ['json']},
      )
      .exitCodes([0]),
  ]),
  test('prints call properties as refs to their signatures, and no PolyT ids', [
    addFile('calls.js')
      .flowCmd([
        'dump-types',
        '--for-tool',
        '--json',
        '--graph',
        '--json-direct',
        '--strip-root',
        'calls.js',
      ])
      .stdout(
        `
          {
            "$defs": {
              "0": {
                "def": {
                  "kind": "Type",
                  "type": {
                    "$ref": 9
                  },
                  "type_kind": "TypeAliasKind"
                },
                "kind": "Def",
                "reason": "calls.js:1:6: \`O\`"
              },
              "1": {
                "def": {
                  "kind": "Obj",
                  "objtype": {
                    "call_t": {
                      "$ref": 10
                    },
                    "flags": {
                      "obj_kind": {
                        "kind": "Exact"
                      }
                    },
                    "props": {},
                    "proto_t": {
                      "$ref": 11
                    }
                  }
                },
                "kind": "Def",
                "reason": "calls.js:1:10-19: object type"
              },
              "2": {
                "def": {
                  "kind": "Void"
                },
                "kind": "Def",
                "reason": "calls.js:1:15-18: undefined"
              },
              "3": {
                "def": {
                  "kind": "Class",
                  "type": {
                    "$ref": 12
                  }
                },
                "kind": "Def",
                "reason": "calls.js:2:15: \`C\`"
              },
              "4": {
                "def": {
                  "kind": "Void"
                },
                "kind": "Def",
                "reason": "calls.js:3:7-10: undefined"
              },
              "5": {
                "def": {
                  "kind": "Poly",
                  "t_out": {
                    "$ref": 13
                  },
                  "tparams": [
                    {
                      "bound": {
                        "$ref": 7
                      },
                      "default": null,
                      "name": "T",
                      "polarity": "neutral"
                    }
                  ]
                },
                "kind": "Def",
                "reason": "calls.js:5:6: \`G\`"
              },
              "6": {
                "bound": {
                  "$ref": 7
                },
                "kind": "Generic",
                "name": "T",
                "no_infer": false
              },
              "7": {
                "def": {
                  "kind": "Mixed"
                },
                "kind": "Def",
                "reason": "calls.js:5:8: \`unknown\`"
              },
              "8": {
                "def": {
                  "kind": "Mixed"
                },
                "kind": "Def",
                "reason": "calls.js:5:8"
              },
              "9": {
                "def": {
                  "kind": "Obj",
                  "objtype": {
                    "call_t": {
                      "$ref": 10
                    },
                    "flags": {
                      "obj_kind": {
                        "kind": "Exact"
                      }
                    },
                    "props": {},
                    "proto_t": {
                      "$ref": 11
                    }
                  }
                },
                "kind": "Def",
                "reason": "calls.js:1:10-19: \`O\`"
              },
              "10": {
                "def": {
                  "funtype": {
                    "effect": {
                      "kind": "ArbitraryEffect"
                    },
                    "params": [],
                    "rest_param": null,
                    "return_t": {
                      "$ref": 2
                    },
                    "this_t": {
                      "status": {
                        "kind": "This_Function"
                      },
                      "type": {
                        "$ref": 14
                      }
                    },
                    "type_guard": null
                  },
                  "kind": "Fun",
                  "static": {
                    "$ref": 15
                  }
                },
                "kind": "Def",
                "reason": "calls.js:1:11-18: function type"
              },
              "11": {
                "kind": "FunProto"
              },
              "12": {
                "instance": {
                  "implements": [],
                  "inst": {
                    "call_t": {
                      "$ref": 16
                    },
                    "class_id": "\\"calls.js\\": (2, 14) to (2, 15)",
                    "class_name": "C",
                    "construct_t": null,
                    "own_props": {},
                    "proto_props": {
                      "constructor": {
                        "kind": "Method",
                        "optional": false,
                        "type": {
                          "$ref": 17
                        }
                      }
                    },
                    "type_args": []
                  },
                  "static": {
                    "$ref": 18
                  },
                  "super": {
                    "$ref": 19
                  }
                },
                "is_this": false,
                "kind": "ThisInstance",
                "name": "this"
              },
              "13": {
                "def": {
                  "kind": "Type",
                  "type": {
                    "$ref": 6
                  },
                  "type_kind": "TypeAliasKind"
                },
                "kind": "Def",
                "reason": "calls.js:5:6: \`G\`"
              },
              "14": {
                "def": {
                  "kind": "Mixed"
                },
                "kind": "Def",
                "reason": "calls.js:1:11-12: bound \`this\` in method"
              },
              "15": {
                "def": {
                  "kind": "Obj",
                  "objtype": {
                    "call_t": null,
                    "flags": {
                      "obj_kind": {
                        "kind": "Inexact"
                      }
                    },
                    "props": {},
                    "proto_t": {
                      "$ref": 11
                    }
                  }
                },
                "kind": "Def",
                "reason": "calls.js:1:11-18"
              },
              "16": {
                "def": {
                  "funtype": {
                    "effect": {
                      "kind": "ArbitraryEffect"
                    },
                    "params": [],
                    "rest_param": null,
                    "return_t": {
                      "$ref": 4
                    },
                    "this_t": {
                      "status": {
                        "kind": "This_Function"
                      },
                      "type": {
                        "$ref": 20
                      }
                    },
                    "type_guard": null
                  },
                  "kind": "Fun",
                  "static": {
                    "$ref": 21
                  }
                },
                "kind": "Def",
                "reason": "calls.js:3:3-10: function type"
              },
              "17": {
                "def": {
                  "funtype": {
                    "effect": {
                      "kind": "ArbitraryEffect"
                    },
                    "params": [],
                    "rest_param": null,
                    "return_t": {
                      "$ref": 22
                    },
                    "this_t": {
                      "status": {
                        "kind": "This_Method"
                      },
                      "type": {
                        "$ref": 23
                      }
                    },
                    "type_guard": null
                  },
                  "kind": "Fun",
                  "static": {
                    "$ref": 24
                  }
                },
                "kind": "Def",
                "reason": "calls.js:2:15"
              },
              "18": {
                "def": {
                  "kind": "Obj",
                  "objtype": {
                    "call_t": null,
                    "flags": {
                      "obj_kind": {
                        "kind": "Inexact"
                      }
                    },
                    "props": {
                      "name": {
                        "kind": "Field",
                        "optional": false,
                        "polarity": "neutral",
                        "type": {
                          "$ref": 25
                        }
                      }
                    },
                    "proto_t": {
                      "$ref": 11
                    }
                  }
                },
                "kind": "Def",
                "reason": "calls.js:2:15"
              },
              "19": {
                "kind": "ObjProto"
              },
              "20": {
                "def": {
                  "kind": "Mixed"
                },
                "kind": "Def",
                "reason": "calls.js:3:3-4: bound \`this\` in method"
              },
              "21": {
                "def": {
                  "kind": "Obj",
                  "objtype": {
                    "call_t": null,
                    "flags": {
                      "obj_kind": {
                        "kind": "Inexact"
                      }
                    },
                    "props": {},
                    "proto_t": {
                      "$ref": 11
                    }
                  }
                },
                "kind": "Def",
                "reason": "calls.js:3:3-10"
              },
              "22": {
                "def": {
                  "kind": "Void"
                },
                "kind": "Def",
                "reason": "calls.js:2:15: undefined"
              },
              "23": {
                "def": {
                  "kind": "Mixed"
                },
                "kind": "Def",
                "reason": "calls.js:2:15: bound \`this\` in method"
              },
              "24": {
                "kind": "Any"
              },
              "25": {
                "def": {
                  "kind": "StrGeneral"
                },
                "kind": "Def",
                "reason": "calls.js:2:15: string"
              }
            },
            "schema": {
              "graph": true,
              "json_direct": true
            },
            "types": [
              {
                "end": 6,
                "endline": 1,
                "line": 1,
                "loc": {
                  "end": {
                    "column": 6,
                    "line": 1,
                    "offset": 6
                  },
                  "source": "calls.js",
                  "start": {
                    "column": 6,
                    "line": 1,
                    "offset": 5
                  },
                  "type": "SourceFile"
                },
                "path": "calls.js",
                "reasons": [],
                "start": 6,
                "type": {
                  "expression_type": {
                    "$ref": 0
                  }
                }
              },
              {
                "end": 19,
                "endline": 1,
                "line": 1,
                "loc": {
                  "end": {
                    "column": 19,
                    "line": 1,
                    "offset": 19
                  },
                  "source": "calls.js",
                  "start": {
                    "column": 10,
                    "line": 1,
                    "offset": 9
                  },
                  "type": "SourceFile"
                },
                "path": "calls.js",
                "reasons": [],
                "start": 10,
                "type": {
                  "expression_type": {
                    "$ref": 1
                  }
                }
              },
              {
                "end": 18,
                "endline": 1,
                "line": 1,
                "loc": {
                  "end": {
                    "column": 18,
                    "line": 1,
                    "offset": 18
                  },
                  "source": "calls.js",
                  "start": {
                    "column": 15,
                    "line": 1,
                    "offset": 14
                  },
                  "type": "SourceFile"
                },
                "path": "calls.js",
                "reasons": [],
                "start": 15,
                "type": {
                  "expression_type": {
                    "$ref": 2
                  }
                }
              },
              {
                "end": 15,
                "endline": 2,
                "line": 2,
                "loc": {
                  "end": {
                    "column": 15,
                    "line": 2,
                    "offset": 36
                  },
                  "source": "calls.js",
                  "start": {
                    "column": 15,
                    "line": 2,
                    "offset": 35
                  },
                  "type": "SourceFile"
                },
                "path": "calls.js",
                "reasons": [],
                "start": 15,
                "type": {
                  "expression_type": {
                    "$ref": 3
                  }
                }
              },
              {
                "end": 10,
                "endline": 3,
                "line": 3,
                "loc": {
                  "end": {
                    "column": 10,
                    "line": 3,
                    "offset": 49
                  },
                  "source": "calls.js",
                  "start": {
                    "column": 7,
                    "line": 3,
                    "offset": 45
                  },
                  "type": "SourceFile"
                },
                "path": "calls.js",
                "reasons": [],
                "start": 7,
                "type": {
                  "expression_type": {
                    "$ref": 4
                  }
                }
              },
              {
                "end": 6,
                "endline": 5,
                "line": 5,
                "loc": {
                  "end": {
                    "column": 6,
                    "line": 5,
                    "offset": 59
                  },
                  "source": "calls.js",
                  "start": {
                    "column": 6,
                    "line": 5,
                    "offset": 58
                  },
                  "type": "SourceFile"
                },
                "path": "calls.js",
                "reasons": [],
                "start": 6,
                "type": {
                  "expression_type": {
                    "$ref": 5
                  }
                }
              },
              {
                "end": 8,
                "endline": 5,
                "line": 5,
                "loc": {
                  "end": {
                    "column": 8,
                    "line": 5,
                    "offset": 61
                  },
                  "source": "calls.js",
                  "start": {
                    "column": 8,
                    "line": 5,
                    "offset": 60
                  },
                  "type": "SourceFile"
                },
                "path": "calls.js",
                "reasons": [],
                "start": 8,
                "type": {
                  "expression_type": {
                    "$ref": 6
                  }
                }
              },
              {
                "end": 8,
                "endline": 5,
                "line": 5,
                "loc": {
                  "end": {
                    "column": 8,
                    "line": 5,
                    "offset": 61
                  },
                  "source": "calls.js",
                  "start": {
                    "column": 9,
                    "line": 5,
                    "offset": 61
                  },
                  "type": "SourceFile"
                },
                "path": "calls.js",
                "reasons": [],
                "start": 9,
                "type": {
                  "expression_type": {
                    "$ref": 7
                  }
                }
              },
              {
                "end": 13,
                "endline": 5,
                "line": 5,
                "loc": {
                  "end": {
                    "column": 13,
                    "line": 5,
                    "offset": 66
                  },
                  "source": "calls.js",
                  "start": {
                    "column": 13,
                    "line": 5,
                    "offset": 65
                  },
                  "type": "SourceFile"
                },
                "path": "calls.js",
                "reasons": [],
                "start": 13,
                "type": {
                  "expression_type": {
                    "$ref": 6
                  },
                  "provider_type": {
                    "$ref": 8
                  }
                }
              },
              {
                "end": 13,
                "endline": 5,
                "line": 5,
                "loc": {
                  "end": {
                    "column": 13,
                    "line": 5,
                    "offset": 66
                  },
                  "source": "calls.js",
                  "start": {
                    "column": 13,
                    "line": 5,
                    "offset": 65
                  },
                  "type": "SourceFile"
                },
                "path": "calls.js",
                "reasons": [],
                "start": 13,
                "type": {
                  "expression_type": {
                    "$ref": 6
                  },
                  "provider_type": {
                    "$ref": 8
                  }
                }
              }
            ]
          }
        `,
        {normalize: ['json']},
      )
      .exitCodes([0]),
  ]),
  test('fails a file with more types than --graph-node-cap', [
    addFile('cycle.js')
      .flowCmd([
        'dump-types',
        '--for-tool',
        '--json',
        '--graph',
        '--graph-node-cap',
        '2',
        '--json-direct',
        '--strip-root',
        'cycle.js',
      ])
      .stdout(
        `
          {
            "$defs": {},
            "schema": {
              "graph": true,
              "json_direct": true
            },
            "types": []
          }
        `,
        {normalize: ['json']},
      )
      .exitCodes([0]),
  ]),
  // `sleep.js` keeps the recheck it starts running for 15 s, and the server answers other commands
  // meanwhile from what it last checked. `--timeout 5` gives up waiting.
  test('waits for a running recheck', [
    addFile('cycle.js')
      .flowCmd(['dump-types', '--for-tool', '--json', '--graph', 'cycle.js'])
      .exitCodes([0]),
    addFile('sleep.js.ignored', 'sleep.js')
      .flowCmd([
        'dump-types',
        '--for-tool',
        '--json',
        '--timeout',
        '5',
        'cycle.js',
      ])
      .exitCodes([0]),
    flowCmd([
      'dump-types',
      '--for-tool',
      '--json',
      '--graph',
      '--timeout',
      '5',
      'cycle.js',
    ]).exitCodes([3]),
    flowCmd([
      'dump-types',
      '--for-tool',
      '--json',
      '--graph',
      '--wait-for-recheck',
      'false',
      'cycle.js',
    ])
      .stderr(
        'dump-types: --graph waits for a running recheck, so it cannot be combined with --wait-for-recheck false',
      )
      .exitCodes([64]),
  ]).waitForRecheck(false),
]): SuiteType);
