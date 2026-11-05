/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cmp::Ordering;
use std::hash::Hash;
use std::hash::Hasher;
use std::ops::Deref;
use std::sync::Arc;

use dupe::Dupe;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;

#[derive(
    Debug,
    Clone,
    Dupe,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct Syntax<M: Dupe, Internal: Dupe> {
    pub leading: Arc<[Comment<M>]>,
    pub trailing: Arc<[Comment<M>]>,
    pub internal: Internal,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct IdentifierInner<M: Dupe, T: Dupe> {
    pub loc: T,
    pub name: FlowSmolStr,
    pub comments: Option<Syntax<M, ()>>,
}

#[derive(
    Clone,
    Dupe,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct Identifier<M: Dupe, T: Dupe>(pub Arc<IdentifierInner<M, T>>);

impl<M: Dupe, T: Dupe> Deref for Identifier<M, T> {
    type Target = IdentifierInner<M, T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<M: Dupe + std::fmt::Debug, T: Dupe + std::fmt::Debug> std::fmt::Debug for Identifier<M, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<M: Dupe, T: Dupe> Identifier<M, T> {
    pub fn new(inner: IdentifierInner<M, T>) -> Self {
        Self(Arc::new(inner))
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct PrivateName<M: Dupe> {
    pub loc: M,
    pub name: FlowSmolStr,
    pub comments: Option<Syntax<M, ()>>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct StringLiteral<M: Dupe> {
    pub value: FlowSmolStr,
    pub raw: FlowSmolStr,
    pub comments: Option<Syntax<M, ()>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NumberLiteral<M: Dupe> {
    pub value: f64,
    pub raw: FlowSmolStr,
    pub comments: Option<Syntax<M, ()>>,
}

impl<M: PartialEq + Dupe> PartialEq for NumberLiteral<M> {
    fn eq(&self, other: &Self) -> bool {
        self.value.to_bits() == other.value.to_bits()
            && self.raw == other.raw
            && self.comments == other.comments
    }
}

impl<M: Eq + Dupe> Eq for NumberLiteral<M> {}

impl<M: Hash + Dupe> Hash for NumberLiteral<M> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.to_bits().hash(state);
        self.raw.hash(state);
        self.comments.hash(state);
    }
}

impl<M: PartialOrd + Dupe> PartialOrd for NumberLiteral<M> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match self.value.total_cmp(&other.value) {
            Ordering::Equal => {}
            ord => return Some(ord),
        }
        match self.raw.partial_cmp(&other.raw) {
            Some(Ordering::Equal) => {}
            ord => return ord,
        }
        self.comments.partial_cmp(&other.comments)
    }
}

impl<M: Ord + Dupe> Ord for NumberLiteral<M> {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.value.total_cmp(&other.value) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.raw.cmp(&other.raw) {
            Ordering::Equal => {}
            ord => return ord,
        }
        self.comments.cmp(&other.comments)
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct BigIntLiteral<M: Dupe> {
    /// This will be None if we couldn't parse `raw`.
    /// That could be if the number is out of range or invalid (like a float)
    pub value: Option<i64>,
    pub raw: FlowSmolStr,
    pub comments: Option<Syntax<M, ()>>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct BooleanLiteral<M: Dupe> {
    pub value: bool,
    pub comments: Option<Syntax<M, ()>>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct RegExpLiteral<M: Dupe> {
    pub pattern: FlowSmolStr,
    pub flags: String,
    pub raw: String,
    pub comments: Option<Syntax<M, ()>>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct ModuleRefLiteral<M: Dupe> {
    pub value: FlowSmolStr,
    pub require_loc: M,
    pub def_loc_opt: Option<M>,
    pub prefix_len: usize,
    pub raw: FlowSmolStr,
    pub comments: Option<Syntax<M, ()>>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub enum VarianceKind {
    Plus,
    Minus,
    Readonly,
    Writeonly,
    In,
    Out,
    InOut,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct Variance<M: Dupe> {
    pub loc: M,
    pub kind: VarianceKind,
    pub comments: Option<Syntax<M, ()>>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct ComputedKey<M: Dupe, T: Dupe> {
    pub loc: M,
    pub expression: expression::Expression<M, T>,
    pub comments: Option<Syntax<M, ()>>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum VariableKind {
    Var,
    Let,
    Const,
}

impl VariableKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            VariableKind::Var => "var",
            VariableKind::Let => "let",
            VariableKind::Const => "const",
        }
    }
}

pub mod types {
    use std::ops::Deref;
    use std::sync::Arc;

    use dupe::Dupe;

    use super::BigIntLiteral;
    use super::BooleanLiteral;
    use super::Comment;
    use super::Identifier;
    use super::NumberLiteral;
    use super::PrivateName;
    use super::StringLiteral;
    use super::Syntax;
    use super::Variance;

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Conditional<M: Dupe, T: Dupe> {
        pub check_type: Type<M, T>,
        pub extends_type: Type<M, T>,
        pub true_type: Type<M, T>,
        pub false_type: Type<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Infer<M: Dupe, T: Dupe> {
        pub tparam: TypeParam<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod function {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Param<M: Dupe, T: Dupe> {
            pub loc: M,
            pub param: ParamKind<M, T>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum ParamKind<M: Dupe, T: Dupe> {
            Anonymous(Type<M, T>),
            Labeled {
                name: Identifier<M, T>,
                annot: Type<M, T>,
                optional: bool,
            },
            Destructuring(super::super::pattern::Pattern<M, T>),
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct RestParam<M: Dupe, T: Dupe> {
            pub loc: M,
            pub argument: Param<M, T>,
            pub comments: Option<Syntax<M, ()>>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct ThisParam<M: Dupe, T: Dupe> {
            pub loc: M,
            pub annot: Annotation<M, T>,
            pub comments: Option<Syntax<M, ()>>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Params<M: Dupe, T: Dupe> {
            pub loc: M,
            pub this: Option<ThisParam<M, T>>,
            pub params: Arc<[Param<M, T>]>,
            pub rest: Option<RestParam<M, T>>,
            pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum ReturnAnnotation<M: Dupe, T: Dupe> {
            Missing(M),
            Available(Annotation<M, T>),
            TypeGuard(TypeGuard<M, T>),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Function<M: Dupe, T: Dupe> {
        pub tparams: Option<TypeParams<M, T>>,
        pub params: function::Params<M, T>,
        pub return_: function::ReturnAnnotation<M, T>,
        pub comments: Option<Syntax<M, ()>>,
        pub effect: super::function::Effect,
    }

    pub mod component_params {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Param<M: Dupe, T: Dupe> {
            pub loc: T,
            pub name: super::super::statement::component_params::ParamName<M, T>,
            pub annot: Annotation<M, T>,
            pub optional: bool,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct RestParam<M: Dupe, T: Dupe> {
            pub loc: T,
            pub argument: Option<Identifier<M, T>>,
            pub annot: Type<M, T>,
            pub optional: bool,
            pub comments: Option<Syntax<M, ()>>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Params<M: Dupe, T: Dupe> {
            pub loc: T,
            pub params: Arc<[Param<M, T>]>,
            pub rest: Option<RestParam<M, T>>,
            pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Component<M: Dupe, T: Dupe> {
        pub tparams: Option<TypeParams<M, T>>,
        pub params: component_params::Params<M, T>,
        pub renders: ComponentRendersAnnotation<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod generic {
        use std::sync::Arc;

        use dupe::Dupe;

        use super::super::StringLiteral;
        use super::super::Syntax;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Identifier<M: Dupe, T: Dupe> {
            Unqualified(super::super::Identifier<M, T>),
            Qualified(Arc<Qualified<M, T>>),
            ImportTypeAnnot(Arc<ImportType<M, T>>),
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Qualified<M: Dupe, T: Dupe> {
            pub loc: M,
            pub qualification: Identifier<M, T>,
            pub id: super::super::Identifier<M, T>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct ImportType<M: Dupe, T: Dupe> {
            pub loc: T,
            pub source: (M, StringLiteral<M>),
            pub comments: Option<Syntax<M, ()>>,
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Generic<M: Dupe, T: Dupe> {
        pub id: generic::Identifier<M, T>,
        pub targs: Option<TypeArgs<M, T>>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct IndexedAccess<M: Dupe, T: Dupe> {
        pub object: Type<M, T>,
        pub index: Type<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct OptionalIndexedAccess<M: Dupe, T: Dupe> {
        pub indexed_access: IndexedAccess<M, T>,
        pub optional: bool,
    }

    pub mod object {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Property<M: Dupe, T: Dupe> {
            NormalProperty(NormalProperty<M, T>),
            SpreadProperty(SpreadProperty<M, T>),
            Indexer(Indexer<M, T>),
            CallProperty(CallProperty<M, T>),
            InternalSlot(InternalSlot<M, T>),
            MappedType(MappedType<M, T>),
            PrivateField(PrivateField<M>),
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct NormalProperty<M: Dupe, T: Dupe> {
            pub loc: M,
            pub key: super::super::expression::object::Key<M, T>,
            pub value: PropertyValue<M, T>,
            pub optional: bool,
            pub static_: bool,
            pub proto: bool,
            pub method: bool,
            pub abstract_: bool,
            pub override_: bool,
            pub variance: Option<Variance<M>>,
            pub ts_accessibility: Option<super::super::class::ts_accessibility::TSAccessibility<M>>,
            pub init: Option<super::super::expression::Expression<M, T>>,
            pub comments: Option<Syntax<M, ()>>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum PropertyValue<M: Dupe, T: Dupe> {
            Init(Option<Type<M, T>>),
            Get(M, Function<M, T>),
            Set(M, Function<M, T>),
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct SpreadProperty<M: Dupe, T: Dupe> {
            pub loc: M,
            pub argument: Type<M, T>,
            pub comments: Option<Syntax<M, ()>>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Indexer<M: Dupe, T: Dupe> {
            pub loc: M,
            pub id: Option<Identifier<M, T>>,
            pub key: Type<M, T>,
            pub value: Type<M, T>,
            pub static_: bool,
            pub variance: Option<Variance<M>>,
            pub optional: bool,
            pub comments: Option<Syntax<M, ()>>,
        }

        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum MappedTypeOptionalFlag {
            PlusOptional,
            MinusOptional,
            Optional,
            NoOptionalFlag,
        }

        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum MappedTypeVarianceOp {
            Add,
            Remove,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct MappedType<M: Dupe, T: Dupe> {
            pub loc: M,
            pub key_tparam: TypeParam<M, T>,
            pub prop_type: Type<M, T>,
            pub source_type: Type<M, T>,
            pub name_type: Option<Type<M, T>>,
            pub variance: Option<Variance<M>>,
            pub variance_op: Option<MappedTypeVarianceOp>,
            pub optional: MappedTypeOptionalFlag,
            pub comments: Option<Syntax<M, ()>>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct PrivateField<M: Dupe> {
            pub loc: M,
            pub key: PrivateName<M>,
            pub comments: Option<Syntax<M, ()>>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct CallProperty<M: Dupe, T: Dupe> {
            pub loc: M,
            pub value: (M, Function<M, T>),
            pub static_: bool,
            pub comments: Option<Syntax<M, ()>>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct InternalSlot<M: Dupe, T: Dupe> {
            pub loc: M,
            pub id: Identifier<M, M>,
            pub value: Type<M, T>,
            pub optional: bool,
            pub static_: bool,
            pub method: bool,
            pub comments: Option<Syntax<M, ()>>,
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Object<M: Dupe, T: Dupe> {
        pub exact: bool,
        pub inexact: bool,
        pub properties: Arc<[object::Property<M, T>]>,
        pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Interface<M: Dupe, T: Dupe> {
        pub body: (M, Object<M, T>),
        pub extends: Arc<[(M, Generic<M, T>)]>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Nullable<M: Dupe, T: Dupe> {
        pub argument: Type<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod typeof_ {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Target<M: Dupe, T: Dupe> {
            Unqualified(Identifier<M, T>),
            Qualified(Arc<Qualified<M, T>>),
            Import(Arc<super::generic::ImportType<M, T>>),
        }

        impl<M: Dupe, T: Dupe> Target<M, T> {
            pub fn loc(&self) -> &T {
                match self {
                    Target::Unqualified(identifier) => &identifier.loc,
                    Target::Qualified(qualified) => &qualified.loc,
                    Target::Import(it) => &it.loc,
                }
            }
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Qualified<M: Dupe, T: Dupe> {
            pub loc: T,
            pub qualification: Target<M, T>,
            pub id: Identifier<M, T>,
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Typeof<M: Dupe, T: Dupe> {
        pub argument: typeof_::Target<M, T>,
        pub targs: Option<TypeArgs<M, T>>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Keyof<M: Dupe, T: Dupe> {
        pub argument: Type<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum RendersVariant {
        Normal,
        Maybe,
        Star,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Renders<M: Dupe, T: Dupe> {
        pub operator_loc: M,
        pub argument: Type<M, T>,
        pub comments: Option<Syntax<M, ()>>,
        pub variant: RendersVariant,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct ReadOnly<M: Dupe, T: Dupe> {
        pub argument: Type<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod tuple {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct LabeledElement<M: Dupe, T: Dupe> {
            pub name: Identifier<M, T>,
            pub annot: Type<M, T>,
            pub variance: Option<Variance<M>>,
            pub optional: bool,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct SpreadElement<M: Dupe, T: Dupe> {
            pub name: Option<Identifier<M, T>>,
            pub annot: Type<M, T>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Element<M: Dupe, T: Dupe> {
            UnlabeledElement {
                loc: M,
                annot: Type<M, T>,
                optional: bool,
            },
            LabeledElement {
                loc: M,
                element: LabeledElement<M, T>,
            },
            SpreadElement {
                loc: M,
                element: SpreadElement<M, T>,
            },
        }

        impl<M: Dupe, T: Dupe> Element<M, T> {
            pub fn loc(&self) -> &M {
                match self {
                    Self::UnlabeledElement { loc, .. } => loc,
                    Self::LabeledElement { loc, .. } => loc,
                    Self::SpreadElement { loc, .. } => loc,
                }
            }

            pub fn loc_mut(&mut self) -> &mut M {
                match self {
                    Self::UnlabeledElement { loc, .. } => loc,
                    Self::LabeledElement { loc, .. } => loc,
                    Self::SpreadElement { loc, .. } => loc,
                }
            }
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Tuple<M: Dupe, T: Dupe> {
        pub elements: Arc<[tuple::Element<M, T>]>,
        pub inexact: bool,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Array<M: Dupe, T: Dupe> {
        pub argument: Type<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Union<M: Dupe, T: Dupe> {
        pub types: (Type<M, T>, Type<M, T>, Vec<Type<M, T>>),
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Intersection<M: Dupe, T: Dupe> {
        pub types: (Type<M, T>, Type<M, T>, Vec<Type<M, T>>),
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod type_template_literal {
        use dupe::Dupe;
        use flow_data_structure_wrapper::smol_str::FlowSmolStr;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Value {
            pub raw: FlowSmolStr,
            pub cooked: FlowSmolStr,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Element<M: Dupe> {
            pub loc: M,
            pub value: Value,
            pub tail: bool,
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct TypeTemplateLiteral<M: Dupe, T: Dupe> {
        pub quasis: Arc<[type_template_literal::Element<M>]>,
        pub types: Arc<[Type<M, T>]>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum BooleanRaw {
        Boolean,
        Bool,
    }

    // Yes, we could add a little complexity here to show that Any and Void
    // should never be declared nullable, but that check can happen later
    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum TypeInner<M: Dupe, T: Dupe> {
        Any {
            loc: T,
            comments: Option<Syntax<M, ()>>,
        },
        Mixed {
            loc: T,
            comments: Option<Syntax<M, ()>>,
        },
        Empty {
            loc: T,
            comments: Option<Syntax<M, ()>>,
        },
        Void {
            loc: T,
            comments: Option<Syntax<M, ()>>,
        },
        Null {
            loc: T,
            comments: Option<Syntax<M, ()>>,
        },
        Number {
            loc: T,
            comments: Option<Syntax<M, ()>>,
        },
        BigInt {
            loc: T,
            comments: Option<Syntax<M, ()>>,
        },
        String {
            loc: T,
            comments: Option<Syntax<M, ()>>,
        },
        Boolean {
            loc: T,
            raw: BooleanRaw,
            comments: Option<Syntax<M, ()>>,
        },
        Symbol {
            loc: T,
            comments: Option<Syntax<M, ()>>,
        },
        Exists {
            loc: T,
            comments: Option<Syntax<M, ()>>,
        },
        Nullable {
            loc: T,
            inner: Arc<Nullable<M, T>>,
        },
        Function {
            loc: T,
            inner: Arc<Function<M, T>>,
        },
        ConstructorType {
            loc: T,
            abstract_: bool,
            inner: Arc<Function<M, T>>,
        },
        Component {
            loc: T,
            inner: Arc<Component<M, T>>,
        },
        Object {
            loc: T,
            inner: Arc<Object<M, T>>,
        },
        Interface {
            loc: T,
            inner: Arc<Interface<M, T>>,
        },
        Array {
            loc: T,
            inner: Arc<Array<M, T>>,
        },
        Conditional {
            loc: T,
            inner: Arc<Conditional<M, T>>,
        },
        Infer {
            loc: T,
            inner: Arc<Infer<M, T>>,
        },
        Generic {
            loc: T,
            inner: Arc<Generic<M, T>>,
        },
        IndexedAccess {
            loc: T,
            inner: Arc<IndexedAccess<M, T>>,
        },
        OptionalIndexedAccess {
            loc: T,
            inner: Arc<OptionalIndexedAccess<M, T>>,
        },
        Union {
            loc: T,
            inner: Arc<Union<M, T>>,
        },
        Intersection {
            loc: T,
            inner: Arc<Intersection<M, T>>,
        },
        Typeof {
            loc: T,
            inner: Arc<Typeof<M, T>>,
        },
        Keyof {
            loc: T,
            inner: Arc<Keyof<M, T>>,
        },
        Renders {
            loc: T,
            inner: Arc<Renders<M, T>>,
        },
        ReadOnly {
            loc: T,
            inner: Arc<ReadOnly<M, T>>,
        },
        Tuple {
            loc: T,
            inner: Arc<Tuple<M, T>>,
        },
        StringLiteral {
            loc: T,
            literal: StringLiteral<M>,
        },
        NumberLiteral {
            loc: T,
            literal: NumberLiteral<M>,
        },
        BigIntLiteral {
            loc: T,
            literal: BigIntLiteral<M>,
        },
        BooleanLiteral {
            loc: T,
            literal: BooleanLiteral<M>,
        },
        TemplateLiteral {
            loc: T,
            inner: Arc<TypeTemplateLiteral<M, T>>,
        },
        Unknown {
            loc: T,
            comments: Option<Syntax<M, ()>>,
        },
        Never {
            loc: T,
            comments: Option<Syntax<M, ()>>,
        },
        Undefined {
            loc: T,
            comments: Option<Syntax<M, ()>>,
        },
        UniqueSymbol {
            loc: T,
            symbol_loc: M,
            comments: Option<Syntax<M, ()>>,
        },
    }

    impl<M: Dupe, T: Dupe> TypeInner<M, T> {
        pub fn loc(&self) -> &T {
            match self {
                Self::Any { loc, .. } => loc,
                Self::Mixed { loc, .. } => loc,
                Self::Empty { loc, .. } => loc,
                Self::Void { loc, .. } => loc,
                Self::Null { loc, .. } => loc,
                Self::Number { loc, .. } => loc,
                Self::BigInt { loc, .. } => loc,
                Self::String { loc, .. } => loc,
                Self::Boolean { loc, .. } => loc,
                Self::Symbol { loc, .. } => loc,
                Self::Exists { loc, .. } => loc,
                Self::Nullable { loc, .. } => loc,
                Self::Function { loc, .. } => loc,
                Self::ConstructorType { loc, .. } => loc,
                Self::Component { loc, .. } => loc,
                Self::Object { loc, .. } => loc,
                Self::Interface { loc, .. } => loc,
                Self::Array { loc, .. } => loc,
                Self::Conditional { loc, .. } => loc,
                Self::Infer { loc, .. } => loc,
                Self::Generic { loc, .. } => loc,
                Self::IndexedAccess { loc, .. } => loc,
                Self::OptionalIndexedAccess { loc, .. } => loc,
                Self::Union { loc, .. } => loc,
                Self::Intersection { loc, .. } => loc,
                Self::Typeof { loc, .. } => loc,
                Self::Keyof { loc, .. } => loc,
                Self::Renders { loc, .. } => loc,
                Self::ReadOnly { loc, .. } => loc,
                Self::Tuple { loc, .. } => loc,
                Self::StringLiteral { loc, .. } => loc,
                Self::NumberLiteral { loc, .. } => loc,
                Self::BigIntLiteral { loc, .. } => loc,
                Self::BooleanLiteral { loc, .. } => loc,
                Self::TemplateLiteral { loc, .. } => loc,
                Self::Unknown { loc, .. } => loc,
                Self::Never { loc, .. } => loc,
                Self::Undefined { loc, .. } => loc,
                Self::UniqueSymbol { loc, .. } => loc,
            }
        }

        pub fn loc_mut(&mut self) -> &mut T {
            match self {
                Self::Any { loc, .. } => loc,
                Self::Mixed { loc, .. } => loc,
                Self::Empty { loc, .. } => loc,
                Self::Void { loc, .. } => loc,
                Self::Null { loc, .. } => loc,
                Self::Number { loc, .. } => loc,
                Self::BigInt { loc, .. } => loc,
                Self::String { loc, .. } => loc,
                Self::Boolean { loc, .. } => loc,
                Self::Symbol { loc, .. } => loc,
                Self::Exists { loc, .. } => loc,
                Self::Nullable { loc, .. } => loc,
                Self::Function { loc, .. } => loc,
                Self::ConstructorType { loc, .. } => loc,
                Self::Component { loc, .. } => loc,
                Self::Object { loc, .. } => loc,
                Self::Interface { loc, .. } => loc,
                Self::Array { loc, .. } => loc,
                Self::Conditional { loc, .. } => loc,
                Self::Infer { loc, .. } => loc,
                Self::Generic { loc, .. } => loc,
                Self::IndexedAccess { loc, .. } => loc,
                Self::OptionalIndexedAccess { loc, .. } => loc,
                Self::Union { loc, .. } => loc,
                Self::Intersection { loc, .. } => loc,
                Self::Typeof { loc, .. } => loc,
                Self::Keyof { loc, .. } => loc,
                Self::Renders { loc, .. } => loc,
                Self::ReadOnly { loc, .. } => loc,
                Self::Tuple { loc, .. } => loc,
                Self::StringLiteral { loc, .. } => loc,
                Self::NumberLiteral { loc, .. } => loc,
                Self::BigIntLiteral { loc, .. } => loc,
                Self::BooleanLiteral { loc, .. } => loc,
                Self::TemplateLiteral { loc, .. } => loc,
                Self::Unknown { loc, .. } => loc,
                Self::Never { loc, .. } => loc,
                Self::Undefined { loc, .. } => loc,
                Self::UniqueSymbol { loc, .. } => loc,
            }
        }
    }

    #[derive(
        Clone,
        Dupe,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Type<M: Dupe, T: Dupe>(pub Arc<TypeInner<M, T>>);

    impl<M: Dupe, T: Dupe> Deref for Type<M, T> {
        type Target = TypeInner<M, T>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<M: Dupe + std::fmt::Debug, T: Dupe + std::fmt::Debug> std::fmt::Debug for Type<M, T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.0.fmt(f)
        }
    }

    impl<M: Dupe, T: Dupe> Type<M, T> {
        pub fn new(inner: TypeInner<M, T>) -> Self {
            Self(Arc::new(inner))
        }

        pub fn loc(&self) -> &T {
            self.0.loc()
        }
    }

    // Type.annotation is a concrete syntax node with a location that starts at
    // the colon and ends after the type. For example, "var a: number", the
    // identifier a would have a property annot which contains a
    // Type.annotation with a location from column 6-14
    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Annotation<M: Dupe, T: Dupe> {
        pub loc: M,
        pub annotation: Type<M, T>,
    }

    // Same convention about the colon holds for type guards.
    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct TypeGuardAnnotation<M: Dupe, T: Dupe> {
        pub loc: M,
        pub guard: TypeGuard<M, T>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum AnnotationOrHint<M: Dupe, T: Dupe> {
        Missing(T),
        Available(Annotation<M, T>),
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum ComponentRendersAnnotation<M: Dupe, T: Dupe> {
        MissingRenders(T),
        AvailableRenders(M, Renders<M, T>),
    }

    pub mod type_param {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum BoundKind {
            Colon,
            Extends,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct ConstModifier<M: Dupe> {
            pub loc: M,
            pub comments: Option<Syntax<M, ()>>,
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct TypeParam<M: Dupe, T: Dupe> {
        pub loc: M,
        pub name: Identifier<M, T>,
        pub bound: AnnotationOrHint<M, T>,
        pub bound_kind: type_param::BoundKind,
        pub variance: Option<Variance<M>>,
        pub default: Option<Type<M, T>>,
        pub const_: Option<type_param::ConstModifier<M>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct TypeParams<M: Dupe, T: Dupe> {
        pub loc: M,
        pub params: Arc<[TypeParam<M, T>]>,
        pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct TypeArgs<M: Dupe, T: Dupe> {
        pub loc: M,
        pub arguments: Arc<[Type<M, T>]>,
        pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum PredicateKind<M: Dupe, T: Dupe> {
        Declared(Arc<super::expression::Expression<M, T>>),
        Inferred,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Predicate<M: Dupe, T: Dupe> {
        pub loc: M,
        pub kind: PredicateKind<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum TypeGuardKind {
        Default,
        Asserts,
        Implies,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct TypeGuard<M: Dupe, T: Dupe> {
        pub loc: M,
        pub kind: TypeGuardKind,
        pub guard: (Identifier<M, M>, Option<Type<M, T>>),
        pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
    }
}

pub mod statement {
    use std::ops::Deref;
    use std::sync::Arc;

    use dupe::Dupe;

    use super::Comment;
    use super::Identifier;
    use super::StringLiteral;
    use super::Syntax;
    use super::class::Class;
    use super::function::Function;
    use super::match_::Match;
    use super::pattern::Pattern;
    use super::types::Annotation;
    use super::types::Type;
    use super::types::TypeParams;

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Block<M: Dupe, T: Dupe> {
        pub body: Arc<[Statement<M, T>]>,
        pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
    }

    pub mod if_ {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Alternate<M: Dupe, T: Dupe> {
            pub loc: M,
            pub body: Statement<M, T>,
            pub comments: Option<Syntax<M, ()>>,
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct If<M: Dupe, T: Dupe> {
        pub test: super::expression::Expression<M, T>,
        pub consequent: Statement<M, T>,
        pub alternate: Option<if_::Alternate<M, T>>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Labeled<M: Dupe, T: Dupe> {
        pub label: Identifier<M, M>,
        pub body: Statement<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Break<M: Dupe> {
        pub label: Option<Identifier<M, M>>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Continue<M: Dupe> {
        pub label: Option<Identifier<M, M>>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Debugger<M: Dupe> {
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct With<M: Dupe, T: Dupe> {
        pub object: super::expression::Expression<M, T>,
        pub body: Statement<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct TypeAlias<M: Dupe, T: Dupe> {
        pub id: Identifier<M, T>,
        pub tparams: Option<TypeParams<M, T>>,
        pub right: Type<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct OpaqueType<M: Dupe, T: Dupe> {
        pub id: Identifier<M, T>,
        pub tparams: Option<TypeParams<M, T>>,
        pub impl_type: Option<Type<M, T>>,
        pub lower_bound: Option<Type<M, T>>,
        pub upper_bound: Option<Type<M, T>>,
        pub legacy_upper_bound: Option<Type<M, T>>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub type MatchStatement<M, T> = Match<M, T, Statement<M, T>>;

    pub mod switch {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Case<M: Dupe, T: Dupe> {
            pub loc: M,
            pub test: Option<super::super::expression::Expression<M, T>>,
            pub case_test_loc: Option<M>,
            pub consequent: Arc<[Statement<M, T>]>,
            pub comments: Option<Syntax<M, ()>>,
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Switch<M: Dupe, T: Dupe> {
        pub discriminant: super::expression::Expression<M, T>,
        pub cases: Arc<[switch::Case<M, T>]>,
        pub comments: Option<Syntax<M, ()>>,
        pub exhaustive_out: T,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Return<M: Dupe, T: Dupe> {
        pub argument: Option<super::expression::Expression<M, T>>,
        pub comments: Option<Syntax<M, ()>>,
        pub return_out: T,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Throw<M: Dupe, T: Dupe> {
        pub argument: super::expression::Expression<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod try_ {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct CatchClause<M: Dupe, T: Dupe> {
            pub loc: M,
            pub param: Option<Pattern<M, T>>,
            pub body: (M, Block<M, T>),
            pub comments: Option<Syntax<M, ()>>,
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Try<M: Dupe, T: Dupe> {
        pub block: (M, Block<M, T>),
        pub handler: Option<try_::CatchClause<M, T>>,
        pub finalizer: Option<(M, Block<M, T>)>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod variable {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Declarator<M: Dupe, T: Dupe> {
            pub loc: M,
            pub id: Pattern<M, T>,
            pub init: Option<super::super::expression::Expression<M, T>>,
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct VariableDeclaration<M: Dupe, T: Dupe> {
        pub declarations: Arc<[variable::Declarator<M, T>]>,
        pub kind: super::VariableKind,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct While<M: Dupe, T: Dupe> {
        pub test: super::expression::Expression<M, T>,
        pub body: Statement<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct DoWhile<M: Dupe, T: Dupe> {
        pub body: Statement<M, T>,
        pub test: super::expression::Expression<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod for_ {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Init<M: Dupe, T: Dupe> {
            InitDeclaration((M, VariableDeclaration<M, T>)),
            InitExpression(super::super::expression::Expression<M, T>),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct For<M: Dupe, T: Dupe> {
        pub init: Option<for_::Init<M, T>>,
        pub test: Option<super::expression::Expression<M, T>>,
        pub update: Option<super::expression::Expression<M, T>>,
        pub body: Statement<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod for_in {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Left<M: Dupe, T: Dupe> {
            LeftDeclaration((M, VariableDeclaration<M, T>)),
            LeftPattern(Pattern<M, T>),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct ForIn<M: Dupe, T: Dupe> {
        pub left: for_in::Left<M, T>,
        pub right: super::expression::Expression<M, T>,
        pub body: Statement<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod for_of {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Left<M: Dupe, T: Dupe> {
            LeftDeclaration((M, VariableDeclaration<M, T>)),
            LeftPattern(Pattern<M, T>),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct ForOf<M: Dupe, T: Dupe> {
        pub left: for_of::Left<M, T>,
        pub right: super::expression::Expression<M, T>,
        pub body: Statement<M, T>,
        pub await_: bool,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod enum_declaration {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum MemberName<M: Dupe> {
            Identifier(Identifier<M, M>),
            StringLiteral(M, super::super::StringLiteral<M>),
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct DefaultedMember<M: Dupe> {
            pub loc: M,
            pub id: MemberName<M>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct InitializedMember<I, M: Dupe> {
            pub loc: M,
            pub id: MemberName<M>,
            pub init: (M, I),
        }

        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum ExplicitType {
            Boolean,
            Number,
            String,
            Symbol,
            BigInt,
        }

        impl ExplicitType {
            pub fn as_str(&self) -> &'static str {
                match self {
                    Self::Boolean => "boolean",
                    Self::Number => "number",
                    Self::String => "string",
                    Self::Symbol => "symbol",
                    Self::BigInt => "bigint",
                }
            }
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Member<M: Dupe> {
            BooleanMember(InitializedMember<super::super::BooleanLiteral<M>, M>),
            NumberMember(InitializedMember<super::super::NumberLiteral<M>, M>),
            StringMember(InitializedMember<super::super::StringLiteral<M>, M>),
            BigIntMember(InitializedMember<super::super::BigIntLiteral<M>, M>),
            DefaultedMember(DefaultedMember<M>),
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Body<M: Dupe> {
            pub loc: M,
            pub members: Arc<[Member<M>]>,
            pub explicit_type: Option<(M, ExplicitType)>,
            pub has_unknown_members: Option<M>,
            pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
        }

        impl<M: Dupe> Body<M> {
            pub fn loc(&self) -> &M {
                &self.loc
            }

            pub fn loc_mut(&mut self) -> &mut M {
                &mut self.loc
            }
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct EnumDeclaration<M: Dupe, T: Dupe> {
        pub id: Identifier<M, T>,
        pub body: enum_declaration::Body<M>,
        pub const_: bool,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod component_params {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct RestParam<M: Dupe, T: Dupe> {
            pub loc: T,
            pub argument: Pattern<M, T>,
            pub comments: Option<Syntax<M, ()>>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Param<M: Dupe, T: Dupe> {
            pub loc: T,
            /// Name should only be an Identifier or StringLiteral. However, we allow parsing
            /// it as an option to have better error messages.  
            pub name: ParamName<M, T>,
            pub local: Pattern<M, T>,
            pub default: Option<super::super::expression::Expression<M, T>>,
            pub shorthand: bool,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum ParamName<M: Dupe, T: Dupe> {
            Identifier(Identifier<M, T>),
            StringLiteral((M, StringLiteral<M>)),
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Params<M: Dupe, T: Dupe> {
            pub loc: T,
            pub params: Arc<[Param<M, T>]>,
            pub rest: Option<RestParam<M, T>>,
            pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct ComponentDeclaration<M: Dupe, T: Dupe> {
        pub id: Identifier<M, T>,
        pub tparams: Option<TypeParams<M, T>>,
        pub params: component_params::Params<M, T>,
        pub renders: super::types::ComponentRendersAnnotation<M, T>,
        pub body: Option<(M, Block<M, T>)>,
        pub comments: Option<Syntax<M, ()>>,
        pub sig_loc: M,
        pub async_: bool,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Interface<M: Dupe, T: Dupe> {
        pub id: Identifier<M, T>,
        pub tparams: Option<TypeParams<M, T>>,
        pub extends: Arc<[(M, super::types::Generic<M, T>)]>,
        pub body: (M, super::types::Object<M, T>),
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum DeclareClassExtends<M: Dupe, T: Dupe> {
        ExtendsIdent(super::types::Generic<M, T>),
        ExtendsCall {
            callee: (M, super::types::Generic<M, T>),
            arg: Box<(M, DeclareClassExtends<M, T>)>,
        },
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct DeclareClass<M: Dupe, T: Dupe> {
        pub id: Identifier<M, T>,
        pub tparams: Option<TypeParams<M, T>>,
        pub body: (M, super::types::Object<M, T>),
        pub extends: Option<(M, DeclareClassExtends<M, T>)>,
        pub mixins: Arc<[(M, super::types::Generic<M, T>)]>,
        pub implements: Option<super::class::Implements<M, T>>,
        pub abstract_: bool,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct DeclareComponent<M: Dupe, T: Dupe> {
        pub id: Identifier<M, T>,
        pub tparams: Option<TypeParams<M, T>>,
        pub params: component_params::Params<M, T>,
        pub renders: super::types::ComponentRendersAnnotation<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct DeclareVariable<M: Dupe, T: Dupe> {
        pub declarations: Arc<[variable::Declarator<M, T>]>,
        pub kind: super::VariableKind,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct DeclareFunction<M: Dupe, T: Dupe> {
        pub id: Option<Identifier<M, T>>,
        pub annot: Annotation<M, T>,
        pub predicate: Option<super::types::Predicate<M, T>>,
        pub comments: Option<Syntax<M, ()>>,
        pub implicit_declare: bool,
    }

    pub mod declare_module {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Id<M: Dupe, T: Dupe> {
            Identifier(Identifier<M, T>),
            Literal((T, StringLiteral<M>)),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct DeclareModule<M: Dupe, T: Dupe> {
        pub id: declare_module::Id<M, T>,
        pub body: (M, Block<M, T>),
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct DeclareModuleExports<M: Dupe, T: Dupe> {
        pub annot: super::types::Annotation<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod declare_namespace {
        use super::*;

        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Keyword {
            Namespace,
            Module,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Id<M: Dupe, T: Dupe> {
            Global(Identifier<M, M>),
            Local(Identifier<M, T>),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct DeclareNamespace<M: Dupe, T: Dupe> {
        pub id: declare_namespace::Id<M, T>,
        pub body: (M, Block<M, T>),
        pub implicit_declare: bool,
        pub comments: Option<Syntax<M, ()>>,
        pub keyword: declare_namespace::Keyword,
    }

    pub mod export_named_declaration {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct ExportSpecifier<M: Dupe, T: Dupe> {
            pub loc: M,
            pub local: Identifier<M, T>,
            pub exported: Option<Identifier<M, T>>,
            pub export_kind: ExportKind,
            pub from_remote: bool,
            pub imported_name_def_loc: Option<M>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct ExportBatchSpecifier<M: Dupe, T: Dupe> {
            pub loc: M,
            pub specifier: Option<Identifier<M, T>>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Specifier<M: Dupe, T: Dupe> {
            ExportSpecifiers(Vec<ExportSpecifier<M, T>>),
            ExportBatchSpecifier(ExportBatchSpecifier<M, T>),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct ExportNamedDeclaration<M: Dupe, T: Dupe> {
        pub declaration: Option<Statement<M, T>>,
        pub specifiers: Option<export_named_declaration::Specifier<M, T>>,
        pub source: Option<(T, StringLiteral<M>)>,
        pub attributes: Option<(M, Vec<import_declaration::ImportAttribute<M, T>>)>,
        pub export_kind: ExportKind,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum ExportAssignmentRhs<M: Dupe, T: Dupe> {
        Expression(super::expression::Expression<M, T>),
        DeclareFunction(M, DeclareFunction<M, T>),
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct ExportAssignment<M: Dupe, T: Dupe> {
        pub rhs: ExportAssignmentRhs<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct NamespaceExportDeclaration<M: Dupe, T: Dupe> {
        pub id: super::Identifier<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod export_default_declaration {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Declaration<M: Dupe, T: Dupe> {
            Declaration(Statement<M, T>),
            Expression(super::super::expression::Expression<M, T>),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct ExportDefaultDeclaration<M: Dupe, T: Dupe> {
        pub default: T,
        pub declaration: export_default_declaration::Declaration<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    impl<M: Dupe, T: Dupe> ExportDefaultDeclaration<M, T> {
        pub fn export_kind(&self) -> ExportKind {
            use export_default_declaration::Declaration;

            match &self.declaration {
                Declaration::Declaration(declaration)
                    if matches!(&**declaration, StatementInner::InterfaceDeclaration { .. }) =>
                {
                    ExportKind::ExportType
                }
                _ => ExportKind::ExportValue,
            }
        }
    }

    pub mod declare_export_declaration {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Declaration<M: Dupe, T: Dupe> {
            Variable {
                loc: M,
                declaration: Arc<DeclareVariable<M, T>>,
            },
            Function {
                loc: M,
                declaration: Arc<DeclareFunction<M, T>>,
            },
            Class {
                loc: M,
                declaration: Arc<DeclareClass<M, T>>,
            },
            Component {
                loc: M,
                declaration: Arc<DeclareComponent<M, T>>,
            },
            DefaultType {
                type_: Arc<Type<M, T>>,
            },
            NamedType {
                loc: M,
                declaration: Arc<TypeAlias<M, T>>,
            },
            NamedOpaqueType {
                loc: M,
                declaration: Arc<OpaqueType<M, T>>,
            },
            Interface {
                loc: M,
                declaration: Arc<Interface<M, T>>,
            },
            Enum {
                loc: M,
                declaration: Arc<EnumDeclaration<M, T>>,
            },
            Namespace {
                loc: M,
                declaration: Box<DeclareNamespace<M, T>>,
            },
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct DeclareExportDeclaration<M: Dupe, T: Dupe> {
        pub default: Option<M>,
        pub declaration: Option<declare_export_declaration::Declaration<M, T>>,
        pub specifiers: Option<export_named_declaration::Specifier<M, T>>,
        pub source: Option<(T, StringLiteral<M>)>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum ImportKind {
        ImportType,
        ImportTypeof,
        ImportValue,
    }

    /// The evaluation phase an import is modified with, as introduced by the
    /// deferred import evaluation proposal:
    ///
    /// ```ignore
    /// import defer * as ns from "m";
    /// const ns = await import.defer("m");
    /// ```
    ///
    /// [None] on an unmodified `import` / `import()`.
    ///
    /// The source phase (`import source x from "m"`, `import.source("m")`) is
    /// not parsed yet. Adding it is a `Source` variant here plus the parse and
    /// validation arms the compiler will point at: nothing about the field,
    /// the ESTree `phase` property or the WASM wire format needs to change to
    /// carry it.
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum ImportPhase {
        Defer,
    }

    impl ImportPhase {
        /// The ESTree `phase` property value for this phase.
        pub fn as_str(self) -> &'static str {
            match self {
                ImportPhase::Defer => "defer",
            }
        }
    }

    pub mod import_declaration {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Specifier<M: Dupe, T: Dupe> {
            ImportNamedSpecifiers(Vec<NamedSpecifier<M, T>>),
            ImportNamespaceSpecifier((M, Identifier<M, T>)),
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct NamedSpecifier<M: Dupe, T: Dupe> {
            pub kind: Option<ImportKind>,
            // Location of the per-specifier `type`/`typeof` keyword when [kind]
            // is [Some]; [None] when [kind] is [None]. Preserved so downstream
            // consumers (ESTree/Babel adapters, codemods) can span the full
            // specifier syntax including the keyword without rescanning source.
            pub kind_loc: Option<M>,
            pub local: Option<Identifier<M, T>>,
            pub remote: Identifier<M, T>,
            pub remote_name_def_loc: Option<M>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct DefaultIdentifier<M: Dupe, T: Dupe> {
            pub identifier: Identifier<M, T>,
            pub remote_default_name_def_loc: Option<M>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum ImportAttributeKey<M: Dupe, T: Dupe> {
            Identifier(Identifier<M, T>),
            StringLiteral(M, StringLiteral<M>),
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct ImportAttribute<M: Dupe, T: Dupe> {
            pub loc: M,
            pub key: ImportAttributeKey<M, T>,
            pub value: (T, StringLiteral<M>),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct ImportDeclaration<M: Dupe, T: Dupe> {
        pub import_kind: ImportKind,
        pub phase: Option<ImportPhase>,
        pub source: (T, StringLiteral<M>),
        pub default: Option<import_declaration::DefaultIdentifier<M, T>>,
        pub specifiers: Option<import_declaration::Specifier<M, T>>,
        pub attributes: Option<(M, Vec<import_declaration::ImportAttribute<M, T>>)>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod import_equals_declaration {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum ModuleReference<M: Dupe, T: Dupe> {
            ExternalModuleReference(T, StringLiteral<M>),
            Identifier(super::super::types::generic::Identifier<M, T>),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct ImportEqualsDeclaration<M: Dupe, T: Dupe> {
        pub id: super::Identifier<M, T>,
        pub module_reference: import_equals_declaration::ModuleReference<M, T>,
        pub import_kind: ImportKind,
        pub is_export: bool,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Expression<M: Dupe, T: Dupe> {
        pub expression: super::expression::Expression<M, T>,
        pub directive: Option<String>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Empty<M: Dupe> {
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum ExportKind {
        ExportType,
        ExportValue,
    }

    pub mod record_declaration {
        use std::sync::Arc;

        use dupe::Dupe;

        use super::super::Syntax;
        use super::super::Variance;
        use super::super::class;
        use super::super::expression::Expression;
        use super::super::expression::object::Key;
        use super::super::types::Annotation;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct InvalidPropertySyntax<M: Dupe> {
            pub invalid_variance: Option<Variance<M>>,
            pub invalid_optional: Option<M>,
            pub invalid_suffix_semicolon: Option<M>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct InvalidSyntax<M: Dupe> {
            pub invalid_infix_equals: Option<M>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Property<M: Dupe, T: Dupe> {
            pub loc: T,
            pub key: Key<M, T>,
            pub annot: Annotation<M, T>,
            pub default_value: Option<Expression<M, T>>,
            pub comments: Option<Syntax<M, ()>>,
            pub invalid_syntax: Option<InvalidPropertySyntax<M>>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct StaticProperty<M: Dupe, T: Dupe> {
            pub loc: T,
            pub key: Key<M, T>,
            pub annot: Annotation<M, T>,
            pub value: Expression<M, T>,
            pub comments: Option<Syntax<M, ()>>,
            pub invalid_syntax: Option<InvalidPropertySyntax<M>>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum BodyElement<M: Dupe, T: Dupe> {
            Method(class::Method<M, T>),
            Property(Property<M, T>),
            StaticProperty(StaticProperty<M, T>),
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Body<M: Dupe, T: Dupe> {
            pub loc: M,
            pub body: Arc<[BodyElement<M, T>]>,
            pub comments: Option<Syntax<M, ()>>,
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct RecordDeclaration<M: Dupe, T: Dupe> {
        pub id: Identifier<M, T>,
        pub tparams: Option<TypeParams<M, T>>,
        pub implements: Option<super::class::Implements<M, T>>,
        pub body: record_declaration::Body<M, T>,
        pub comments: Option<Syntax<M, ()>>,
        pub invalid_syntax: Option<record_declaration::InvalidSyntax<M>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum StatementInner<M: Dupe, T: Dupe> {
        Block {
            loc: M,
            inner: Arc<Block<M, T>>,
        },
        Break {
            loc: M,
            inner: Arc<Break<M>>,
        },
        ClassDeclaration {
            loc: M,
            inner: Arc<Class<M, T>>,
        },
        ComponentDeclaration {
            loc: M,
            inner: Arc<ComponentDeclaration<M, T>>,
        },
        Continue {
            loc: M,
            inner: Arc<Continue<M>>,
        },
        Debugger {
            loc: M,
            inner: Arc<Debugger<M>>,
        },
        DeclareClass {
            loc: M,
            inner: Arc<DeclareClass<M, T>>,
        },
        DeclareComponent {
            loc: M,
            inner: Arc<DeclareComponent<M, T>>,
        },
        DeclareEnum {
            loc: M,
            inner: Arc<EnumDeclaration<M, T>>,
        },
        DeclareExportDeclaration {
            loc: M,
            inner: Arc<DeclareExportDeclaration<M, T>>,
        },
        DeclareFunction {
            loc: M,
            inner: Arc<DeclareFunction<M, T>>,
        },
        DeclareInterface {
            loc: M,
            inner: Arc<Interface<M, T>>,
        },
        DeclareModule {
            loc: M,
            inner: Arc<DeclareModule<M, T>>,
        },
        DeclareModuleExports {
            loc: M,
            inner: Arc<DeclareModuleExports<M, T>>,
        },
        DeclareNamespace {
            loc: M,
            inner: Arc<DeclareNamespace<M, T>>,
        },
        DeclareTypeAlias {
            loc: M,
            inner: Arc<TypeAlias<M, T>>,
        },
        DeclareOpaqueType {
            loc: M,
            inner: Arc<OpaqueType<M, T>>,
        },
        DeclareVariable {
            loc: M,
            inner: Arc<DeclareVariable<M, T>>,
        },
        DoWhile {
            loc: M,
            inner: Arc<DoWhile<M, T>>,
        },
        Empty {
            loc: M,
            inner: Arc<Empty<M>>,
        },
        EnumDeclaration {
            loc: M,
            inner: Arc<EnumDeclaration<M, T>>,
        },
        ExportDefaultDeclaration {
            loc: M,
            inner: Arc<ExportDefaultDeclaration<M, T>>,
        },
        ExportNamedDeclaration {
            loc: M,
            inner: Arc<ExportNamedDeclaration<M, T>>,
        },
        ExportAssignment {
            loc: M,
            inner: Arc<ExportAssignment<M, T>>,
        },
        NamespaceExportDeclaration {
            loc: M,
            inner: Arc<NamespaceExportDeclaration<M, T>>,
        },
        Expression {
            loc: M,
            inner: Arc<Expression<M, T>>,
        },
        For {
            loc: M,
            inner: Arc<For<M, T>>,
        },
        ForIn {
            loc: M,
            inner: Arc<ForIn<M, T>>,
        },
        ForOf {
            loc: M,
            inner: Arc<ForOf<M, T>>,
        },
        FunctionDeclaration {
            loc: M,
            inner: Arc<Function<M, T>>,
        },
        If {
            loc: M,
            inner: Arc<If<M, T>>,
        },
        ImportDeclaration {
            loc: M,
            inner: Arc<ImportDeclaration<M, T>>,
        },
        ImportEqualsDeclaration {
            loc: M,
            inner: Arc<ImportEqualsDeclaration<M, T>>,
        },
        InterfaceDeclaration {
            loc: M,
            inner: Arc<Interface<M, T>>,
        },
        Labeled {
            loc: M,
            inner: Arc<Labeled<M, T>>,
        },
        Match {
            loc: M,
            inner: Arc<MatchStatement<M, T>>,
        },
        RecordDeclaration {
            loc: M,
            inner: Arc<RecordDeclaration<M, T>>,
        },
        Return {
            loc: M,
            inner: Arc<Return<M, T>>,
        },
        Switch {
            loc: M,
            inner: Arc<Switch<M, T>>,
        },
        Throw {
            loc: M,
            inner: Arc<Throw<M, T>>,
        },
        Try {
            loc: M,
            inner: Arc<Try<M, T>>,
        },
        TypeAlias {
            loc: M,
            inner: Arc<TypeAlias<M, T>>,
        },
        OpaqueType {
            loc: M,
            inner: Arc<OpaqueType<M, T>>,
        },
        VariableDeclaration {
            loc: M,
            inner: Arc<VariableDeclaration<M, T>>,
        },
        While {
            loc: M,
            inner: Arc<While<M, T>>,
        },
        With {
            loc: M,
            inner: Arc<With<M, T>>,
        },
    }

    impl<M: Dupe, T: Dupe> StatementInner<M, T> {
        pub fn loc(&self) -> &M {
            match self {
                Self::Block { loc, .. } => loc,
                Self::Break { loc, .. } => loc,
                Self::ClassDeclaration { loc, .. } => loc,
                Self::ComponentDeclaration { loc, .. } => loc,
                Self::Continue { loc, .. } => loc,
                Self::Debugger { loc, .. } => loc,
                Self::DeclareClass { loc, .. } => loc,
                Self::DeclareComponent { loc, .. } => loc,
                Self::DeclareEnum { loc, .. } => loc,
                Self::DeclareExportDeclaration { loc, .. } => loc,
                Self::DeclareFunction { loc, .. } => loc,
                Self::DeclareInterface { loc, .. } => loc,
                Self::DeclareModule { loc, .. } => loc,
                Self::DeclareModuleExports { loc, .. } => loc,
                Self::DeclareNamespace { loc, .. } => loc,
                Self::DeclareTypeAlias { loc, .. } => loc,
                Self::DeclareOpaqueType { loc, .. } => loc,
                Self::DeclareVariable { loc, .. } => loc,
                Self::DoWhile { loc, .. } => loc,
                Self::Empty { loc, .. } => loc,
                Self::EnumDeclaration { loc, .. } => loc,
                Self::ExportDefaultDeclaration { loc, .. } => loc,
                Self::ExportNamedDeclaration { loc, .. } => loc,
                Self::ExportAssignment { loc, .. } => loc,
                Self::NamespaceExportDeclaration { loc, .. } => loc,
                Self::Expression { loc, .. } => loc,
                Self::For { loc, .. } => loc,
                Self::ForIn { loc, .. } => loc,
                Self::ForOf { loc, .. } => loc,
                Self::FunctionDeclaration { loc, .. } => loc,
                Self::If { loc, .. } => loc,
                Self::ImportDeclaration { loc, .. } => loc,
                Self::ImportEqualsDeclaration { loc, .. } => loc,
                Self::InterfaceDeclaration { loc, .. } => loc,
                Self::Labeled { loc, .. } => loc,
                Self::Match { loc, .. } => loc,
                Self::RecordDeclaration { loc, .. } => loc,
                Self::Return { loc, .. } => loc,
                Self::Switch { loc, .. } => loc,
                Self::Throw { loc, .. } => loc,
                Self::Try { loc, .. } => loc,
                Self::TypeAlias { loc, .. } => loc,
                Self::OpaqueType { loc, .. } => loc,
                Self::VariableDeclaration { loc, .. } => loc,
                Self::While { loc, .. } => loc,
                Self::With { loc, .. } => loc,
            }
        }

        pub fn loc_mut(&mut self) -> &mut M {
            match self {
                Self::Block { loc, .. } => loc,
                Self::Break { loc, .. } => loc,
                Self::ClassDeclaration { loc, .. } => loc,
                Self::ComponentDeclaration { loc, .. } => loc,
                Self::Continue { loc, .. } => loc,
                Self::Debugger { loc, .. } => loc,
                Self::DeclareClass { loc, .. } => loc,
                Self::DeclareComponent { loc, .. } => loc,
                Self::DeclareEnum { loc, .. } => loc,
                Self::DeclareExportDeclaration { loc, .. } => loc,
                Self::DeclareFunction { loc, .. } => loc,
                Self::DeclareInterface { loc, .. } => loc,
                Self::DeclareModule { loc, .. } => loc,
                Self::DeclareModuleExports { loc, .. } => loc,
                Self::DeclareNamespace { loc, .. } => loc,
                Self::DeclareTypeAlias { loc, .. } => loc,
                Self::DeclareOpaqueType { loc, .. } => loc,
                Self::DeclareVariable { loc, .. } => loc,
                Self::DoWhile { loc, .. } => loc,
                Self::Empty { loc, .. } => loc,
                Self::EnumDeclaration { loc, .. } => loc,
                Self::ExportDefaultDeclaration { loc, .. } => loc,
                Self::ExportNamedDeclaration { loc, .. } => loc,
                Self::ExportAssignment { loc, .. } => loc,
                Self::NamespaceExportDeclaration { loc, .. } => loc,
                Self::Expression { loc, .. } => loc,
                Self::For { loc, .. } => loc,
                Self::ForIn { loc, .. } => loc,
                Self::ForOf { loc, .. } => loc,
                Self::FunctionDeclaration { loc, .. } => loc,
                Self::If { loc, .. } => loc,
                Self::ImportDeclaration { loc, .. } => loc,
                Self::ImportEqualsDeclaration { loc, .. } => loc,
                Self::InterfaceDeclaration { loc, .. } => loc,
                Self::Labeled { loc, .. } => loc,
                Self::Match { loc, .. } => loc,
                Self::RecordDeclaration { loc, .. } => loc,
                Self::Return { loc, .. } => loc,
                Self::Switch { loc, .. } => loc,
                Self::Throw { loc, .. } => loc,
                Self::Try { loc, .. } => loc,
                Self::TypeAlias { loc, .. } => loc,
                Self::OpaqueType { loc, .. } => loc,
                Self::VariableDeclaration { loc, .. } => loc,
                Self::While { loc, .. } => loc,
                Self::With { loc, .. } => loc,
            }
        }
    }

    #[derive(
        Clone,
        Dupe,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Statement<M: Dupe, T: Dupe>(pub Arc<StatementInner<M, T>>);

    impl<M: Dupe, T: Dupe> Deref for Statement<M, T> {
        type Target = StatementInner<M, T>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<M: Dupe + std::fmt::Debug, T: Dupe + std::fmt::Debug> std::fmt::Debug for Statement<M, T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.0.fmt(f)
        }
    }

    impl<M: Dupe, T: Dupe> Statement<M, T> {
        pub fn new(inner: StatementInner<M, T>) -> Self {
            Self(Arc::new(inner))
        }

        pub fn loc(&self) -> &M {
            self.0.loc()
        }
    }
}

pub mod expression {
    use std::ops::Deref;
    use std::sync::Arc;

    use dupe::Dupe;

    use super::BigIntLiteral;
    use super::BooleanLiteral;
    use super::Comment;
    use super::ComputedKey;
    use super::Identifier;
    use super::ModuleRefLiteral;
    use super::NumberLiteral;
    use super::PrivateName;
    use super::RegExpLiteral;
    use super::StringLiteral;
    use super::Syntax;
    use super::class::Class;
    use super::function::Function;
    use super::jsx;
    use super::match_::Match;
    use super::pattern::Pattern;
    use super::types::Annotation;
    use super::types::Type;

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct CallTypeArgImplicit<M: Dupe, T: Dupe> {
        pub loc: T,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum CallTypeArg<M: Dupe, T: Dupe> {
        Explicit(Type<M, T>),
        Implicit(CallTypeArgImplicit<M, T>),
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct CallTypeArgs<M: Dupe, T: Dupe> {
        pub loc: T,
        pub arguments: Arc<[CallTypeArg<M, T>]>,
        pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct SpreadElement<M: Dupe, T: Dupe> {
        pub loc: M,
        pub argument: Expression<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum ArrayElement<M: Dupe, T: Dupe> {
        Expression(Expression<M, T>),
        Spread(SpreadElement<M, T>),
        Hole(M),
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Array<M: Dupe, T: Dupe> {
        pub elements: Arc<[ArrayElement<M, T>]>,
        pub trailing_comma: bool,
        pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
    }

    pub mod template_literal {
        use dupe::Dupe;
        use flow_data_structure_wrapper::smol_str::FlowSmolStr;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Value {
            pub raw: FlowSmolStr,
            pub cooked: FlowSmolStr,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Element<M: Dupe> {
            pub loc: M,
            pub value: Value,
            pub tail: bool,
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct TemplateLiteral<M: Dupe, T: Dupe> {
        pub quasis: Arc<[template_literal::Element<M>]>,
        pub expressions: Arc<[Expression<M, T>]>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct TaggedTemplate<M: Dupe, T: Dupe> {
        pub tag: Expression<M, T>,
        pub targs: Option<CallTypeArgs<M, T>>,
        pub quasi: (M, TemplateLiteral<M, T>),
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod object {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Key<M: Dupe, T: Dupe> {
            StringLiteral((T, StringLiteral<M>)),
            NumberLiteral((T, NumberLiteral<M>)),
            BigIntLiteral((T, BigIntLiteral<M>)),
            Identifier(Identifier<M, T>),
            PrivateName(PrivateName<M>),
            Computed(ComputedKey<M, T>),
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum NormalProperty<M: Dupe, T: Dupe> {
            Init {
                loc: M,
                key: Key<M, T>,
                value: Expression<M, T>,
                shorthand: bool,
            },
            Method {
                loc: M,
                key: Key<M, T>,
                value: (M, Function<M, T>),
            },
            Get {
                loc: M,
                key: Key<M, T>,
                value: (M, Function<M, T>),
                comments: Option<Syntax<M, ()>>,
            },
            Set {
                loc: M,
                key: Key<M, T>,
                value: (M, Function<M, T>),
                comments: Option<Syntax<M, ()>>,
            },
        }

        impl<M: Dupe, T: Dupe> NormalProperty<M, T> {
            pub fn loc(&self) -> &M {
                match self {
                    Self::Init { loc, .. } => loc,
                    Self::Method { loc, .. } => loc,
                    Self::Get { loc, .. } => loc,
                    Self::Set { loc, .. } => loc,
                }
            }

            pub fn loc_mut(&mut self) -> &mut M {
                match self {
                    Self::Init { loc, .. } => loc,
                    Self::Method { loc, .. } => loc,
                    Self::Get { loc, .. } => loc,
                    Self::Set { loc, .. } => loc,
                }
            }
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct SpreadProperty<M: Dupe, T: Dupe> {
            pub loc: M,
            pub argument: Expression<M, T>,
            pub comments: Option<Syntax<M, ()>>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Property<M: Dupe, T: Dupe> {
            NormalProperty(NormalProperty<M, T>),
            SpreadProperty(SpreadProperty<M, T>),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Object<M: Dupe, T: Dupe> {
        pub properties: Arc<[object::Property<M, T>]>,
        pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Record<M: Dupe, T: Dupe> {
        pub constructor: Expression<M, T>,
        pub targs: Option<CallTypeArgs<M, T>>,
        pub properties: (M, Object<M, T>),
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Sequence<M: Dupe, T: Dupe> {
        pub expressions: Arc<[Expression<M, T>]>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum UnaryOperator {
        Minus,
        Plus,
        Not,
        BitNot,
        Typeof,
        Void,
        Delete,
        Await,
        Nonnull,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Unary<M: Dupe, T: Dupe> {
        pub operator: UnaryOperator,
        pub argument: Expression<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum BinaryOperator {
        Equal,
        NotEqual,
        StrictEqual,
        StrictNotEqual,
        LessThan,
        LessThanEqual,
        GreaterThan,
        GreaterThanEqual,
        LShift,
        RShift,
        RShift3,
        Plus,
        Minus,
        Mult,
        Exp,
        Div,
        Mod,
        BitOr,
        Xor,
        BitAnd,
        In,
        Instanceof,
    }

    impl BinaryOperator {
        pub fn as_str(&self) -> &'static str {
            match self {
                Self::Equal => "==",
                Self::NotEqual => "!=",
                Self::StrictEqual => "===",
                Self::StrictNotEqual => "!==",
                Self::LessThan => "<",
                Self::LessThanEqual => "<=",
                Self::GreaterThan => ">",
                Self::GreaterThanEqual => ">=",
                Self::LShift => "<<",
                Self::RShift => ">>",
                Self::RShift3 => ">>>",
                Self::Plus => "+",
                Self::Minus => "-",
                Self::Mult => "*",
                Self::Exp => "**",
                Self::Div => "/",
                Self::Mod => "%",
                Self::BitOr => "|",
                Self::Xor => "^",
                Self::BitAnd => "&",
                Self::In => "in",
                Self::Instanceof => "instanceof",
            }
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Binary<M: Dupe, T: Dupe> {
        pub operator: BinaryOperator,
        pub left: Expression<M, T>,
        pub right: Expression<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum AssignmentOperator {
        PlusAssign,
        MinusAssign,
        MultAssign,
        ExpAssign,
        DivAssign,
        ModAssign,
        LShiftAssign,
        RShiftAssign,
        RShift3Assign,
        BitOrAssign,
        BitXorAssign,
        BitAndAssign,
        NullishAssign,
        AndAssign,
        OrAssign,
    }

    impl AssignmentOperator {
        pub fn as_str(&self) -> &'static str {
            match self {
                Self::PlusAssign => "+=",
                Self::MinusAssign => "-=",
                Self::MultAssign => "*=",
                Self::ExpAssign => "**=",
                Self::DivAssign => "/=",
                Self::ModAssign => "%=",
                Self::LShiftAssign => "<<=",
                Self::RShiftAssign => ">>=",
                Self::RShift3Assign => ">>>=",
                Self::BitOrAssign => "|=",
                Self::BitXorAssign => "^=",
                Self::BitAndAssign => "&=",
                Self::NullishAssign => "??=",
                Self::AndAssign => "&&=",
                Self::OrAssign => "||=",
            }
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Assignment<M: Dupe, T: Dupe> {
        pub operator: Option<AssignmentOperator>,
        pub left: Pattern<M, T>,
        pub right: Expression<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum UpdateOperator {
        Increment,
        Decrement,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Update<M: Dupe, T: Dupe> {
        pub operator: UpdateOperator,
        pub argument: Expression<M, T>,
        pub prefix: bool,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum LogicalOperator {
        Or,
        And,
        NullishCoalesce,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Logical<M: Dupe, T: Dupe> {
        pub operator: LogicalOperator,
        pub left: Expression<M, T>,
        pub right: Expression<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Conditional<M: Dupe, T: Dupe> {
        pub test: Expression<M, T>,
        pub consequent: Expression<M, T>,
        pub alternate: Expression<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum ExpressionOrSpread<M: Dupe, T: Dupe> {
        Expression(Expression<M, T>),
        Spread(SpreadElement<M, T>),
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct ArgList<M: Dupe, T: Dupe> {
        pub loc: M,
        pub arguments: Arc<[ExpressionOrSpread<M, T>]>,
        pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct New<M: Dupe, T: Dupe> {
        pub callee: Expression<M, T>,
        pub targs: Option<CallTypeArgs<M, T>>,
        pub arguments: Option<ArgList<M, T>>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Call<M: Dupe, T: Dupe> {
        pub callee: Expression<M, T>,
        pub targs: Option<CallTypeArgs<M, T>>,
        pub arguments: ArgList<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        Dupe,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum OptionalCallKind<M: Dupe> {
        Optional,
        NonOptional,
        AssertNonnull { assertion_loc: M },
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct OptionalCall<M: Dupe, T: Dupe> {
        pub call: Call<M, T>,
        pub filtered_out: T,
        pub optional: OptionalCallKind<M>,
    }

    pub mod member {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Property<M: Dupe, T: Dupe> {
            PropertyIdentifier(Identifier<M, T>),
            PropertyPrivateName(PrivateName<M>),
            PropertyExpression(Expression<M, T>),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Member<M: Dupe, T: Dupe> {
        pub object: Expression<M, T>,
        pub property: member::Property<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        Dupe,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum OptionalMemberKind<M: Dupe> {
        Optional,
        NonOptional,
        AssertNonnull { assertion_loc: M },
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct OptionalMember<M: Dupe, T: Dupe> {
        pub member: Member<M, T>,
        pub filtered_out: T,
        pub optional: OptionalMemberKind<M>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Yield<M: Dupe, T: Dupe> {
        pub argument: Option<Expression<M, T>>,
        pub comments: Option<Syntax<M, ()>>,
        pub delegate: bool,
        pub result_out: T,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct TypeCast<M: Dupe, T: Dupe> {
        pub expression: Expression<M, T>,
        pub annot: Annotation<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }
    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct AsExpression<M: Dupe, T: Dupe> {
        pub expression: Expression<M, T>,
        pub annot: Annotation<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }
    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct AsConstExpression<M: Dupe, T: Dupe> {
        pub expression: Expression<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct TSSatisfies<M: Dupe, T: Dupe> {
        pub expression: Expression<M, T>,
        pub annot: Annotation<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct MetaProperty<M: Dupe> {
        pub meta: Identifier<M, M>,
        pub property: Identifier<M, M>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct This<M: Dupe> {
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Super<M: Dupe> {
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Import<M: Dupe, T: Dupe> {
        pub argument: Expression<M, T>,
        pub options: Option<Expression<M, T>>,
        /// [Some] for a phase-modified dynamic import (`import.defer(...)`),
        /// [None] for a plain `import(...)`.
        pub phase: Option<super::statement::ImportPhase>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub type MatchExpression<M, T> = Match<M, T, Expression<M, T>>;

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum ExpressionInner<M: Dupe, T: Dupe> {
        Array {
            loc: T,
            inner: Arc<Array<M, T>>,
        },
        ArrowFunction {
            loc: T,
            inner: Arc<Function<M, T>>,
        },
        AsConstExpression {
            loc: T,
            inner: Arc<AsConstExpression<M, T>>,
        },
        AsExpression {
            loc: T,
            inner: Arc<AsExpression<M, T>>,
        },
        Assignment {
            loc: T,
            inner: Arc<Assignment<M, T>>,
        },
        Binary {
            loc: T,
            inner: Arc<Binary<M, T>>,
        },
        Call {
            loc: T,
            inner: Arc<Call<M, T>>,
        },
        Class {
            loc: T,
            inner: Arc<Class<M, T>>,
        },
        Conditional {
            loc: T,
            inner: Arc<Conditional<M, T>>,
        },
        Function {
            loc: T,
            inner: Arc<Function<M, T>>,
        },
        Identifier {
            loc: T,
            inner: Identifier<M, T>,
        },
        Import {
            loc: T,
            inner: Arc<Import<M, T>>,
        },
        JSXElement {
            loc: T,
            inner: Arc<jsx::Element<M, T>>,
        },
        JSXFragment {
            loc: T,
            inner: Arc<jsx::Fragment<M, T>>,
        },
        StringLiteral {
            loc: T,
            inner: Arc<StringLiteral<M>>,
        },
        BooleanLiteral {
            loc: T,
            inner: Arc<BooleanLiteral<M>>,
        },
        NullLiteral {
            loc: T,
            inner: Arc<Option<Syntax<M, ()>>>,
        },
        NumberLiteral {
            loc: T,
            inner: Arc<NumberLiteral<M>>,
        },
        BigIntLiteral {
            loc: T,
            inner: Arc<BigIntLiteral<M>>,
        },
        RegExpLiteral {
            loc: T,
            inner: Arc<RegExpLiteral<M>>,
        },
        ModuleRefLiteral {
            loc: T,
            inner: Arc<ModuleRefLiteral<M>>,
        },
        Logical {
            loc: T,
            inner: Arc<Logical<M, T>>,
        },
        Match {
            loc: T,
            inner: Arc<MatchExpression<M, T>>,
        },
        Member {
            loc: T,
            inner: Arc<Member<M, T>>,
        },
        MetaProperty {
            loc: T,
            inner: Arc<MetaProperty<M>>,
        },
        New {
            loc: T,
            inner: Arc<New<M, T>>,
        },
        Object {
            loc: T,
            inner: Arc<Object<M, T>>,
        },
        OptionalCall {
            loc: T,
            inner: Arc<OptionalCall<M, T>>,
        },
        OptionalMember {
            loc: T,
            inner: Arc<OptionalMember<M, T>>,
        },
        Record {
            loc: T,
            inner: Arc<Record<M, T>>,
        },
        Sequence {
            loc: T,
            inner: Arc<Sequence<M, T>>,
        },
        Super {
            loc: T,
            inner: Arc<Super<M>>,
        },
        TaggedTemplate {
            loc: T,
            inner: Arc<TaggedTemplate<M, T>>,
        },
        TemplateLiteral {
            loc: T,
            inner: Arc<TemplateLiteral<M, T>>,
        },
        This {
            loc: T,
            inner: Arc<This<M>>,
        },
        TypeCast {
            loc: T,
            inner: Arc<TypeCast<M, T>>,
        },
        TSSatisfies {
            loc: T,
            inner: Arc<TSSatisfies<M, T>>,
        },
        Unary {
            loc: T,
            inner: Arc<Unary<M, T>>,
        },
        Update {
            loc: T,
            inner: Arc<Update<M, T>>,
        },
        Yield {
            loc: T,
            inner: Arc<Yield<M, T>>,
        },
    }

    impl<M: Dupe, T: Dupe> ExpressionInner<M, T> {
        pub fn loc(&self) -> &T {
            match self {
                Self::Array { loc, .. } => loc,
                Self::ArrowFunction { loc, .. } => loc,
                Self::AsConstExpression { loc, .. } => loc,
                Self::AsExpression { loc, .. } => loc,
                Self::Assignment { loc, .. } => loc,
                Self::Binary { loc, .. } => loc,
                Self::Call { loc, .. } => loc,
                Self::Class { loc, .. } => loc,
                Self::Conditional { loc, .. } => loc,
                Self::Function { loc, .. } => loc,
                Self::Identifier { loc, .. } => loc,
                Self::Import { loc, .. } => loc,
                Self::JSXElement { loc, .. } => loc,
                Self::JSXFragment { loc, .. } => loc,
                Self::StringLiteral { loc, .. } => loc,
                Self::BooleanLiteral { loc, .. } => loc,
                Self::NullLiteral { loc, .. } => loc,
                Self::NumberLiteral { loc, .. } => loc,
                Self::BigIntLiteral { loc, .. } => loc,
                Self::RegExpLiteral { loc, .. } => loc,
                Self::ModuleRefLiteral { loc, .. } => loc,
                Self::Logical { loc, .. } => loc,
                Self::Match { loc, .. } => loc,
                Self::Member { loc, .. } => loc,
                Self::MetaProperty { loc, .. } => loc,
                Self::New { loc, .. } => loc,
                Self::Object { loc, .. } => loc,
                Self::OptionalCall { loc, .. } => loc,
                Self::OptionalMember { loc, .. } => loc,
                Self::Record { loc, .. } => loc,
                Self::Sequence { loc, .. } => loc,
                Self::Super { loc, .. } => loc,
                Self::TaggedTemplate { loc, .. } => loc,
                Self::TemplateLiteral { loc, .. } => loc,
                Self::This { loc, .. } => loc,
                Self::TypeCast { loc, .. } => loc,
                Self::TSSatisfies { loc, .. } => loc,
                Self::Unary { loc, .. } => loc,
                Self::Update { loc, .. } => loc,
                Self::Yield { loc, .. } => loc,
            }
        }

        pub fn loc_mut(&mut self) -> &mut T {
            match self {
                Self::Array { loc, .. } => loc,
                Self::ArrowFunction { loc, .. } => loc,
                Self::AsConstExpression { loc, .. } => loc,
                Self::AsExpression { loc, .. } => loc,
                Self::Assignment { loc, .. } => loc,
                Self::Binary { loc, .. } => loc,
                Self::Call { loc, .. } => loc,
                Self::Class { loc, .. } => loc,
                Self::Conditional { loc, .. } => loc,
                Self::Function { loc, .. } => loc,
                Self::Identifier { loc, .. } => loc,
                Self::Import { loc, .. } => loc,
                Self::JSXElement { loc, .. } => loc,
                Self::JSXFragment { loc, .. } => loc,
                Self::StringLiteral { loc, .. } => loc,
                Self::BooleanLiteral { loc, .. } => loc,
                Self::NullLiteral { loc, .. } => loc,
                Self::NumberLiteral { loc, .. } => loc,
                Self::BigIntLiteral { loc, .. } => loc,
                Self::RegExpLiteral { loc, .. } => loc,
                Self::ModuleRefLiteral { loc, .. } => loc,
                Self::Logical { loc, .. } => loc,
                Self::Match { loc, .. } => loc,
                Self::Member { loc, .. } => loc,
                Self::MetaProperty { loc, .. } => loc,
                Self::New { loc, .. } => loc,
                Self::Object { loc, .. } => loc,
                Self::OptionalCall { loc, .. } => loc,
                Self::OptionalMember { loc, .. } => loc,
                Self::Record { loc, .. } => loc,
                Self::Sequence { loc, .. } => loc,
                Self::Super { loc, .. } => loc,
                Self::TaggedTemplate { loc, .. } => loc,
                Self::TemplateLiteral { loc, .. } => loc,
                Self::This { loc, .. } => loc,
                Self::TypeCast { loc, .. } => loc,
                Self::TSSatisfies { loc, .. } => loc,
                Self::Unary { loc, .. } => loc,
                Self::Update { loc, .. } => loc,
                Self::Yield { loc, .. } => loc,
            }
        }
    }

    #[derive(
        Clone,
        Dupe,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Expression<M: Dupe, T: Dupe>(pub Arc<ExpressionInner<M, T>>);

    impl<M: Dupe, T: Dupe> Deref for Expression<M, T> {
        type Target = ExpressionInner<M, T>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<M: Dupe + std::fmt::Debug, T: Dupe + std::fmt::Debug> std::fmt::Debug for Expression<M, T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.0.fmt(f)
        }
    }

    impl<M: Dupe, T: Dupe> Expression<M, T> {
        pub fn new(inner: ExpressionInner<M, T>) -> Self {
            Self(Arc::new(inner))
        }
    }
}

pub mod jsx {
    use std::sync::Arc;

    use dupe::Dupe;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;

    use super::Comment;
    use super::StringLiteral;
    use super::Syntax;
    use super::expression::Expression;

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Identifier<M: Dupe, T: Dupe> {
        pub loc: T,
        pub name: FlowSmolStr,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct NamespacedName<M: Dupe, T: Dupe> {
        pub loc: M,
        pub namespace: Identifier<M, T>,
        pub name: Identifier<M, T>,
    }

    pub mod expression_container {
        use dupe::Dupe;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Expression<M: Dupe, T: Dupe> {
            Expression(super::super::expression::Expression<M, T>),
            EmptyExpression,
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct ExpressionContainer<M: Dupe, T: Dupe> {
        pub expression: expression_container::Expression<M, T>,
        pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Text {
        pub value: FlowSmolStr,
        pub raw: FlowSmolStr,
    }

    pub mod attribute {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Name<M: Dupe, T: Dupe> {
            Identifier(super::Identifier<M, T>),
            NamespacedName(NamespacedName<M, T>),
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Value<M: Dupe, T: Dupe> {
            StringLiteral((T, StringLiteral<M>)),
            ExpressionContainer((T, ExpressionContainer<M, T>)),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Attribute<M: Dupe, T: Dupe> {
        pub loc: M,
        pub name: attribute::Name<M, T>,
        pub value: Option<attribute::Value<M, T>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct SpreadAttribute<M: Dupe, T: Dupe> {
        pub loc: M,
        pub argument: Expression<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod member_expression {
        use std::sync::Arc;

        use dupe::Dupe;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Object<M: Dupe, T: Dupe> {
            Identifier(super::Identifier<M, T>),
            MemberExpression(Arc<super::MemberExpression<M, T>>),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct MemberExpression<M: Dupe, T: Dupe> {
        pub loc: M,
        pub object: member_expression::Object<M, T>,
        pub property: Identifier<M, T>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum Name<M: Dupe, T: Dupe> {
        Identifier(Identifier<M, T>),
        NamespacedName(NamespacedName<M, T>),
        MemberExpression(MemberExpression<M, T>),
    }

    impl<L: Dupe> Name<L, L> {
        pub fn loc(&self) -> &L {
            match self {
                Self::Identifier(id) => &id.loc,
                Self::NamespacedName(ns) => &ns.loc,
                Self::MemberExpression(e) => &e.loc,
            }
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum OpeningAttribute<M: Dupe, T: Dupe> {
        Attribute(Attribute<M, T>),
        SpreadAttribute(SpreadAttribute<M, T>),
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Opening<M: Dupe, T: Dupe> {
        pub loc: M,
        pub name: Name<M, T>,
        pub targs: Option<super::expression::CallTypeArgs<M, T>>,
        pub self_closing: bool,
        pub attributes: Arc<[OpeningAttribute<M, T>]>,
        /// The typed AST records the props inferred for the element here. The
        /// parser stores the element's loc.
        pub props: T,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Closing<M: Dupe, T: Dupe> {
        pub loc: M,
        pub name: Name<M, T>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct SpreadChild<M: Dupe, T: Dupe> {
        pub expression: Expression<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum Child<M: Dupe, T: Dupe> {
        Element {
            loc: T,
            inner: Element<M, T>,
        },
        Fragment {
            loc: T,
            inner: Fragment<M, T>,
        },
        ExpressionContainer {
            loc: T,
            inner: ExpressionContainer<M, T>,
        },
        SpreadChild {
            loc: T,
            inner: SpreadChild<M, T>,
        },
        Text {
            loc: T,
            inner: Text,
        },
    }

    impl<M: Dupe, T: Dupe> Child<M, T> {
        pub fn loc(&self) -> &T {
            match self {
                Self::Element { loc, .. } => loc,
                Self::Fragment { loc, .. } => loc,
                Self::ExpressionContainer { loc, .. } => loc,
                Self::SpreadChild { loc, .. } => loc,
                Self::Text { loc, .. } => loc,
            }
        }

        pub fn loc_mut(&mut self) -> &mut T {
            match self {
                Self::Element { loc, .. } => loc,
                Self::Fragment { loc, .. } => loc,
                Self::ExpressionContainer { loc, .. } => loc,
                Self::SpreadChild { loc, .. } => loc,
                Self::Text { loc, .. } => loc,
            }
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Element<M: Dupe, T: Dupe> {
        pub opening_element: Opening<M, T>,
        pub closing_element: Option<Closing<M, T>>,
        pub children: (M, Vec<Child<M, T>>),
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Fragment<M: Dupe, T: Dupe> {
        pub frag_opening_element: M,
        pub frag_closing_element: M,
        pub frag_children: (M, Vec<Child<M, T>>),
        pub frag_comments: Option<Syntax<M, ()>>,
    }
}

pub mod match_ {
    use std::sync::Arc;

    use dupe::Dupe;

    use super::Syntax;
    use super::expression::Expression;
    use super::match_pattern::MatchPattern;

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct InvalidSyntax<M: Dupe> {
        pub invalid_prefix_case: Option<M>,
        pub invalid_infix_colon: Option<M>,
        pub invalid_suffix_semicolon: Option<M>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Case<M: Dupe, T: Dupe, B> {
        pub loc: M,
        pub pattern: MatchPattern<M, T>,
        pub body: B,
        pub guard: Option<Expression<M, T>>,
        pub comments: Option<Syntax<M, ()>>,
        pub invalid_syntax: InvalidSyntax<M>,
        pub case_match_root_loc: M,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Match<M: Dupe, T: Dupe, B> {
        pub arg: Expression<M, T>,
        pub cases: Arc<[Case<M, T, B>]>,
        pub match_keyword_loc: M,
        pub comments: Option<Syntax<M, ()>>,
    }
}

pub mod match_pattern {
    use std::sync::Arc;

    use dupe::Dupe;

    use super::BigIntLiteral;
    use super::BooleanLiteral;
    use super::Comment;
    use super::Identifier;
    use super::NumberLiteral;
    use super::StringLiteral;
    use super::Syntax;
    use super::VariableKind;

    pub mod unary_pattern {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Operator {
            Plus,
            Minus,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Argument<M: Dupe> {
            NumberLiteral(NumberLiteral<M>),
            BigIntLiteral(BigIntLiteral<M>),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct UnaryPattern<M: Dupe> {
        pub operator: unary_pattern::Operator,
        pub argument: (M, unary_pattern::Argument<M>),
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod member_pattern {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Base<M: Dupe, T: Dupe> {
            BaseIdentifier(Identifier<M, T>),
            BaseMember(Arc<MemberPattern<M, T>>),
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Property<M: Dupe, T: Dupe> {
            PropertyString { loc: M, literal: StringLiteral<M> },
            PropertyNumber { loc: M, literal: NumberLiteral<M> },
            PropertyBigInt { loc: M, literal: BigIntLiteral<M> },
            PropertyIdentifier(Identifier<M, T>),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct MemberPattern<M: Dupe, T: Dupe> {
        pub loc: T,
        pub base: member_pattern::Base<M, T>,
        pub property: member_pattern::Property<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct BindingPattern<M: Dupe, T: Dupe> {
        pub kind: VariableKind,
        pub id: Identifier<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct RestPattern<M: Dupe, T: Dupe> {
        pub loc: M,
        pub argument: Option<(M, BindingPattern<M, T>)>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod object_pattern {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Key<M: Dupe, T: Dupe> {
            StringLiteral((M, StringLiteral<M>)),
            NumberLiteral((M, NumberLiteral<M>)),
            BigIntLiteral((M, BigIntLiteral<M>)),
            Identifier(Identifier<M, T>),
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct PropertyStruct<M: Dupe, T: Dupe> {
            pub key: Key<M, T>,
            pub pattern: MatchPattern<M, T>,
            pub shorthand: bool,
            pub comments: Option<Syntax<M, ()>>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Property<M: Dupe, T: Dupe> {
            Valid {
                loc: M,
                property: PropertyStruct<M, T>,
            },
            InvalidShorthand {
                loc: M,
                identifier: Identifier<M, M>,
            },
        }

        impl<M: Dupe, T: Dupe> Property<M, T> {
            pub fn loc(&self) -> &M {
                match self {
                    Self::Valid { loc, .. } => loc,
                    Self::InvalidShorthand { loc, .. } => loc,
                }
            }

            pub fn loc_mut(&mut self) -> &mut M {
                match self {
                    Self::Valid { loc, .. } => loc,
                    Self::InvalidShorthand { loc, .. } => loc,
                }
            }
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct ObjectPattern<M: Dupe, T: Dupe> {
        pub properties: Arc<[object_pattern::Property<M, T>]>,
        pub rest: Option<RestPattern<M, T>>,
        pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
    }

    pub mod array_pattern {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Element<M: Dupe, T: Dupe> {
            pub index: M,
            pub pattern: MatchPattern<M, T>,
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct ArrayPattern<M: Dupe, T: Dupe> {
        pub elements: Arc<[array_pattern::Element<M, T>]>,
        pub rest: Option<RestPattern<M, T>>,
        pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum InstancePatternConstructor<M: Dupe, T: Dupe> {
        IdentifierConstructor(Identifier<M, T>),
        MemberConstructor(MemberPattern<M, T>),
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct InstancePattern<M: Dupe, T: Dupe> {
        pub constructor: InstancePatternConstructor<M, T>,
        pub properties: (M, ObjectPattern<M, T>),
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct OrPattern<M: Dupe, T: Dupe> {
        pub patterns: Arc<[MatchPattern<M, T>]>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod as_pattern {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Target<M: Dupe, T: Dupe> {
            Identifier(Identifier<M, T>),
            Binding {
                loc: M,
                pattern: BindingPattern<M, T>,
            },
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct AsPattern<M: Dupe, T: Dupe> {
        pub pattern: MatchPattern<M, T>,
        pub target: as_pattern::Target<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct WildcardPattern<M: Dupe> {
        pub comments: Option<Syntax<M, ()>>,
        pub invalid_syntax_default_keyword: bool,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum MatchPattern<M: Dupe, T: Dupe> {
        WildcardPattern {
            loc: M,
            inner: Box<WildcardPattern<M>>,
        },
        NumberPattern {
            loc: M,
            inner: Box<NumberLiteral<M>>,
        },
        BigIntPattern {
            loc: M,
            inner: Box<BigIntLiteral<M>>,
        },
        StringPattern {
            loc: M,
            inner: Box<StringLiteral<M>>,
        },
        BooleanPattern {
            loc: M,
            inner: Box<BooleanLiteral<M>>,
        },
        NullPattern {
            loc: M,
            inner: Box<Option<Syntax<M, ()>>>,
        },
        UnaryPattern {
            loc: M,
            inner: Arc<UnaryPattern<M>>,
        },
        BindingPattern {
            loc: M,
            inner: Arc<BindingPattern<M, T>>,
        },
        IdentifierPattern {
            loc: M,
            inner: Box<Identifier<M, T>>,
        },
        MemberPattern {
            loc: M,
            inner: Arc<MemberPattern<M, T>>,
        },
        ObjectPattern {
            loc: M,
            inner: Arc<ObjectPattern<M, T>>,
        },
        ArrayPattern {
            loc: M,
            inner: Arc<ArrayPattern<M, T>>,
        },
        OrPattern {
            loc: M,
            inner: Arc<OrPattern<M, T>>,
        },
        AsPattern {
            loc: M,
            inner: Arc<AsPattern<M, T>>,
        },
        InstancePattern {
            loc: M,
            inner: Arc<InstancePattern<M, T>>,
        },
    }

    impl<M: Dupe, T: Dupe> MatchPattern<M, T> {
        pub fn loc(&self) -> &M {
            match self {
                Self::WildcardPattern { loc, .. } => loc,
                Self::NumberPattern { loc, .. } => loc,
                Self::BigIntPattern { loc, .. } => loc,
                Self::StringPattern { loc, .. } => loc,
                Self::BooleanPattern { loc, .. } => loc,
                Self::NullPattern { loc, .. } => loc,
                Self::UnaryPattern { loc, .. } => loc,
                Self::BindingPattern { loc, .. } => loc,
                Self::IdentifierPattern { loc, .. } => loc,
                Self::MemberPattern { loc, .. } => loc,
                Self::ObjectPattern { loc, .. } => loc,
                Self::ArrayPattern { loc, .. } => loc,
                Self::InstancePattern { loc, .. } => loc,
                Self::OrPattern { loc, .. } => loc,
                Self::AsPattern { loc, .. } => loc,
            }
        }

        pub fn loc_mut(&mut self) -> &mut M {
            match self {
                Self::WildcardPattern { loc, .. } => loc,
                Self::NumberPattern { loc, .. } => loc,
                Self::BigIntPattern { loc, .. } => loc,
                Self::StringPattern { loc, .. } => loc,
                Self::BooleanPattern { loc, .. } => loc,
                Self::NullPattern { loc, .. } => loc,
                Self::UnaryPattern { loc, .. } => loc,
                Self::BindingPattern { loc, .. } => loc,
                Self::IdentifierPattern { loc, .. } => loc,
                Self::MemberPattern { loc, .. } => loc,
                Self::ObjectPattern { loc, .. } => loc,
                Self::ArrayPattern { loc, .. } => loc,
                Self::InstancePattern { loc, .. } => loc,
                Self::OrPattern { loc, .. } => loc,
                Self::AsPattern { loc, .. } => loc,
            }
        }
    }
}

pub mod pattern {
    use std::sync::Arc;

    use dupe::Dupe;

    use super::BigIntLiteral;
    use super::Comment;
    use super::ComputedKey;
    use super::NumberLiteral;
    use super::StringLiteral;
    use super::Syntax;
    use super::types::AnnotationOrHint;

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct RestElement<M: Dupe, T: Dupe> {
        pub loc: M,
        pub argument: Pattern<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod object {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Key<M: Dupe, T: Dupe> {
            StringLiteral((M, StringLiteral<M>)),
            NumberLiteral((M, NumberLiteral<M>)),
            BigIntLiteral((M, BigIntLiteral<M>)),
            Identifier(super::super::Identifier<M, T>),
            Computed(ComputedKey<M, T>),
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct NormalProperty<M: Dupe, T: Dupe> {
            pub loc: M,
            pub key: Key<M, T>,
            pub pattern: Pattern<M, T>,
            pub default: Option<super::super::expression::Expression<M, T>>,
            pub shorthand: bool,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Property<M: Dupe, T: Dupe> {
            NormalProperty(NormalProperty<M, T>),
            RestElement(RestElement<M, T>),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Object<M: Dupe, T: Dupe> {
        pub properties: Arc<[object::Property<M, T>]>,
        pub annot: AnnotationOrHint<M, T>,
        pub optional: bool,
        pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
    }

    pub mod array {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct NormalElement<M: Dupe, T: Dupe> {
            pub loc: M,
            pub argument: Pattern<M, T>,
            pub default: Option<super::super::expression::Expression<M, T>>,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Element<M: Dupe, T: Dupe> {
            NormalElement(NormalElement<M, T>),
            RestElement(RestElement<M, T>),
            Hole(M),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Array<M: Dupe, T: Dupe> {
        pub elements: Arc<[array::Element<M, T>]>,
        pub annot: AnnotationOrHint<M, T>,
        pub optional: bool,
        pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Identifier<M: Dupe, T: Dupe> {
        pub name: super::Identifier<M, T>,
        pub annot: AnnotationOrHint<M, T>,
        pub optional: bool,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum Pattern<M: Dupe, T: Dupe> {
        Object {
            loc: T,
            inner: Arc<Object<M, T>>,
        },
        Array {
            loc: T,
            inner: Arc<Array<M, T>>,
        },
        Identifier {
            loc: T,
            inner: Arc<Identifier<M, T>>,
        },
        Expression {
            loc: T,
            inner: Arc<super::expression::Expression<M, T>>,
        },
    }

    impl<M: Dupe, T: Dupe> Pattern<M, T> {
        pub fn loc(&self) -> &T {
            match self {
                Self::Object { loc, .. } => loc,
                Self::Array { loc, .. } => loc,
                Self::Identifier { loc, .. } => loc,
                Self::Expression { loc, .. } => loc,
            }
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    Dupe,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub enum CommentKind {
    Block,
    Line,
}

#[derive(
    Debug,
    Clone,
    Dupe,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct Comment<M: Dupe> {
    pub loc: M,
    pub kind: CommentKind,
    pub text: Arc<str>,
    pub on_newline: bool,
}

pub mod class {
    use std::sync::Arc;

    use dupe::Dupe;

    use super::Comment;
    use super::Identifier;
    use super::PrivateName;
    use super::Syntax;
    use super::Variance;
    use super::expression::Expression;
    use super::function::Function;
    use super::types::AnnotationOrHint;
    use super::types::TypeArgs;
    use super::types::TypeParams;

    pub mod ts_accessibility {
        use dupe::Dupe;

        use super::Syntax;

        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Kind {
            Public,
            Protected,
            Private,
        }

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct TSAccessibility<M: Dupe> {
            pub loc: M,
            pub kind: Kind,
            pub comments: Option<Syntax<M, ()>>,
        }
    }

    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum MethodKind {
        Constructor,
        Method,
        Get,
        Set,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Method<M: Dupe, T: Dupe> {
        pub loc: T,
        pub kind: MethodKind,
        pub key: super::expression::object::Key<M, T>,
        pub value: (M, Function<M, T>),
        pub static_: bool,
        pub override_: bool,
        pub ts_accessibility: Option<ts_accessibility::TSAccessibility<M>>,
        pub decorators: Arc<[Decorator<M, T>]>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod property {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub enum Value<M: Dupe, T: Dupe> {
            Declared,
            Uninitialized,
            Initialized(Expression<M, T>),
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Property<M: Dupe, T: Dupe> {
        pub loc: T,
        pub key: super::expression::object::Key<M, T>,
        pub value: property::Value<M, T>,
        pub annot: AnnotationOrHint<M, T>,
        pub static_: bool,
        pub override_: bool,
        pub optional: bool,
        pub variance: Option<Variance<M>>,
        pub ts_accessibility: Option<ts_accessibility::TSAccessibility<M>>,
        pub decorators: Arc<[Decorator<M, T>]>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct PrivateField<M: Dupe, T: Dupe> {
        pub loc: T,
        pub key: PrivateName<M>,
        pub value: property::Value<M, T>,
        pub annot: AnnotationOrHint<M, T>,
        pub static_: bool,
        pub override_: bool,
        pub optional: bool,
        pub variance: Option<Variance<M>>,
        pub ts_accessibility: Option<ts_accessibility::TSAccessibility<M>>,
        pub decorators: Arc<[Decorator<M, T>]>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct StaticBlock<M: Dupe, T: Dupe> {
        pub loc: M,
        pub body: Arc<[super::statement::Statement<M, T>]>,
        pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct DeclareMethod<M: Dupe, T: Dupe> {
        pub loc: T,
        pub kind: MethodKind,
        pub key: super::expression::object::Key<M, T>,
        pub annot: super::types::Annotation<M, T>,
        pub static_: bool,
        pub override_: bool,
        pub optional: bool,
        pub ts_accessibility: Option<ts_accessibility::TSAccessibility<M>>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct AbstractMethod<M: Dupe, T: Dupe> {
        pub loc: T,
        pub key: super::expression::object::Key<M, T>,
        pub annot: (M, super::types::Function<M, T>),
        pub override_: bool,
        pub ts_accessibility: Option<ts_accessibility::TSAccessibility<M>>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct AbstractProperty<M: Dupe, T: Dupe> {
        pub loc: T,
        pub key: super::expression::object::Key<M, T>,
        pub annot: AnnotationOrHint<M, T>,
        pub override_: bool,
        pub ts_accessibility: Option<ts_accessibility::TSAccessibility<M>>,
        pub variance: Option<Variance<M>>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Extends<M: Dupe, T: Dupe> {
        pub loc: M,
        pub expr: Expression<M, T>,
        pub targs: Option<TypeArgs<M, T>>,
        pub comments: Option<Syntax<M, ()>>,
    }

    pub mod implements {
        use super::*;

        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize
        )]
        pub struct Interface<M: Dupe, T: Dupe> {
            pub loc: M,
            pub id: super::super::types::generic::Identifier<M, T>,
            pub targs: Option<TypeArgs<M, T>>,
        }
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Implements<M: Dupe, T: Dupe> {
        pub loc: M,
        pub interfaces: Arc<[implements::Interface<M, T>]>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum BodyElement<M: Dupe, T: Dupe> {
        Method(Method<M, T>),
        Property(Property<M, T>),
        PrivateField(PrivateField<M, T>),
        StaticBlock(StaticBlock<M, T>),
        DeclareMethod(DeclareMethod<M, T>),
        AbstractMethod(AbstractMethod<M, T>),
        AbstractProperty(AbstractProperty<M, T>),
        IndexSignature(super::types::object::Indexer<M, T>),
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Body<M: Dupe, T: Dupe> {
        pub loc: M,
        pub body: Arc<[BodyElement<M, T>]>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Decorator<M: Dupe, T: Dupe> {
        pub loc: M,
        pub expression: Expression<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Class<M: Dupe, T: Dupe> {
        pub id: Option<Identifier<M, T>>,
        pub body: Body<M, T>,
        pub tparams: Option<TypeParams<M, T>>,
        pub extends: Option<Extends<M, T>>,
        pub implements: Option<Implements<M, T>>,
        pub class_decorators: Arc<[Decorator<M, T>]>,
        pub abstract_: bool,
        pub comments: Option<Syntax<M, ()>>,
    }
}

pub mod function {
    use std::sync::Arc;

    use dupe::Dupe;

    use super::Comment;
    use super::Identifier;
    use super::Syntax;
    use super::expression::Expression;
    use super::pattern::Pattern;
    use super::types::Annotation;
    use super::types::Predicate;
    use super::types::TypeGuardAnnotation;
    use super::types::TypeParams;

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct ThisParam<M: Dupe, T: Dupe> {
        pub loc: M,
        pub annot: Annotation<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum Param<M: Dupe, T: Dupe> {
        RegularParam {
            loc: M,
            argument: Pattern<M, T>,
            default: Option<Expression<M, T>>,
        },
        ParamProperty {
            loc: M,
            property: super::class::Property<M, T>,
        },
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct RestParam<M: Dupe, T: Dupe> {
        pub loc: M,
        pub argument: Pattern<M, T>,
        pub comments: Option<Syntax<M, ()>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Params<M: Dupe, T: Dupe> {
        pub loc: M,
        pub this_: Option<ThisParam<M, T>>,
        pub params: Arc<[Param<M, T>]>,
        pub rest: Option<RestParam<M, T>>,
        pub comments: Option<Syntax<M, Arc<[Comment<M>]>>>,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum ReturnAnnot<M: Dupe, T: Dupe> {
        Missing(T),
        Available(Annotation<M, T>),
        TypeGuard(TypeGuardAnnotation<M, T>),
    }

    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum Effect {
        Hook,
        Arbitrary,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub struct Function<M: Dupe, T: Dupe> {
        pub id: Option<Identifier<M, T>>,
        pub params: Params<M, T>,
        pub body: Body<M, T>,
        pub async_: bool,
        pub generator: bool,
        pub effect_: Effect,
        pub predicate: Option<Predicate<M, T>>,
        pub return_: ReturnAnnot<M, T>,
        pub tparams: Option<TypeParams<M, T>>,
        pub comments: Option<Syntax<M, ()>>,
        // Location of the signature portion of a function, e.g.
        // function foo(): void {}
        // ^^^^^^^^^^^^^^^^^^^^
        pub sig_loc: M,
    }

    #[derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum Body<M: Dupe, T: Dupe> {
        BodyBlock((M, super::statement::Block<M, T>)),
        BodyExpression(Expression<M, T>),
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct Program<M: Dupe, T: Dupe> {
    pub loc: M,
    pub statements: Arc<[statement::Statement<M, T>]>,
    pub interpreter: Option<(M, String)>, // interpreter directive / shebang
    pub comments: Option<Syntax<M, ()>>,
    pub all_comments: Arc<[Comment<M>]>,
}
