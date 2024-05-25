use super::{
    Category,
    ExprU::{self, *},
    Keyword, Requirement,
    Requirement::*,
    Schema, SchemaTypeCheckError,
    SchemaTypeCheckError::*,
};
use std::{collections::HashSet, fmt, result::Result as StdResult};
use ExprT::*;

#[derive(Clone, Debug, PartialEq, Eq)]
enum ExprT {
    SchemaT(Schema),
    CategoryT((Category, Vec<Keyword>)),
    KeywordT(Keyword),
    RequirementT(Requirement),
    NatT(u8),
    StringT(String),
    ListT(Vec<ExprT>),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    String,
    Nat,
    Keyword,
    List(Box<Type>),
    Schema,
    Category,
    Requirement,
    Hole,
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::String => write!(f, "string"),
            Type::Nat => write!(f, "nat"),
            Type::Keyword => write!(f, "keyword"),
            Type::List(_) => write!(f, "list"),
            Type::Schema => write!(f, "schema"),
            Type::Category => write!(f, "category"),
            Type::Requirement => write!(f, "requirement"),
            Type::Hole => write!(f, "unknown"),
        }
    }
}

type Result<T> = StdResult<T, SchemaTypeCheckError>;

pub fn typecheck(expr: ExprU) -> Result<Schema> {
    match typecheck_(expr) {
        Ok(SchemaT(x)) => Ok(x),
        Ok(_) => Err(ExpectedTopLevelSchema),
        Err(e) => Err(e),
    }
}

fn typecheck_(expr: ExprU) -> Result<ExprT> {
    match expr {
        NatU(x) => Ok(NatT(x)),
        StringU(x) => Ok(StringT(x)),
        KeywordU { name, id } => Ok(KeywordT(Keyword { name, id })),
        ListU(xs) => {
            let xs = xs
                .iter()
                .map(|x| typecheck_(x.clone()))
                .collect::<Result<Vec<ExprT>>>()?;
            let mut types = HashSet::with_capacity(2);
            for x in xs.clone() {
                types.insert(type_of(&x));
            }
            let types = types.into_iter().collect::<Vec<Type>>();
            match &types[..] {
                // empty list can take on any type
                [] => Ok(ListT(xs)),
                // homogeneous
                [_] => Ok(ListT(xs)),
                // heterogeneous, but coercable. TODO make this more graceful
                [Type::String, Type::Keyword] | [Type::Keyword, Type::String] => {
                    let xs = xs
                        .iter()
                        .map(|x| match x {
                            StringT(s) => KeywordT(Keyword {
                                name: s.to_string(),
                                id: s.to_string(),
                            }),
                            x => x.clone(),
                        })
                        .collect::<Vec<ExprT>>();
                    Ok(ListT(xs))
                }
                // heterogenous
                _ => Err(HeterogeneousListIsNotCoercable(types.clone())),
            }
        }
        FnU { name, args } => match (name.as_str(), &args[..]) {
            ("exactly", [NatU(x)]) => Ok(RequirementT(Exactly(*x))),
            ("at_least", [NatU(x)]) => Ok(RequirementT(AtLeast(*x))),
            ("at_most", [NatU(x)]) => Ok(RequirementT(AtMost(*x))),
            ("category", [StringU(name), req @ FnU { .. }, keywords @ ListU(_)]) => {
                let req = typecheck_(req.clone())?;
                let keywords = typecheck_(keywords.clone())?;
                let t = type_of(&keywords);
                match (req, t.clone(), keywords) {
                    (RequirementT(requirement), Type::List(t), ListT(xs)) => {
                        if let Type::Keyword = *t {
                            let keywords: Vec<Keyword> = xs
                                .into_iter()
                                .map(|elem| match elem {
                                    KeywordT(kw) => kw,
                                    _ => panic!("unreachable"),
                                })
                                .collect();
                            Ok(CategoryT((
                                Category {
                                    name: name.clone(),
                                    requirement,
                                },
                                keywords,
                            )))
                        } else {
                            Err(TypeMismatch {
                                expected: Type::List(Box::new(Type::Keyword)),
                                got: Type::List(t),
                            })
                        }
                    }
                    _ => Err(TypeMismatch {
                        expected: Type::List(Box::new(Type::Keyword)),
                        got: t,
                    }),
                }
            }
            ("schema", [StringU(delim), StringU(empty), categories @ ListU(_)]) => {
                let categories = typecheck_(categories.clone())?;
                let t = type_of(&categories);
                match (t.clone(), categories) {
                    (Type::List(telem), ListT(xs)) => {
                        if let Type::Category = *telem {
                            let categories = xs
                                .into_iter()
                                .map(|x| match x {
                                    CategoryT(c) => c,
                                    _ => panic!("unreachable"),
                                })
                                .collect();
                            Ok(SchemaT(Schema {
                                delim: delim.clone(),
                                empty: empty.clone(),
                                categories,
                            }))
                        } else {
                            Err(TypeMismatch {
                                expected: Type::List(Box::new(Type::Category)),
                                got: Type::List(telem),
                            })
                        }
                    }
                    _ => Err(TypeMismatch {
                        expected: Type::List(Box::new(Type::Category)),
                        got: t,
                    }),
                }
            }
            (name, _) => Err(UnknownFunction(name.to_string())),
        },
    }
}

fn type_of(expr: &ExprT) -> Type {
    match expr {
        SchemaT(_) => Type::Schema,
        RequirementT(_) => Type::Requirement,
        CategoryT(_) => Type::Category,
        NatT(_) => Type::Nat,
        StringT(_) => Type::String,
        KeywordT(_) => Type::Keyword,
        ListT(args) => match &args[..] {
            [] => Type::List(Box::new(Type::Hole)),
            [h, _t @ ..] => Type::List(Box::new(type_of(h))),
        },
    }
}

#[test]
fn test_typecheck() {
    assert_eq!(
        typecheck_(ListU(vec![
            StringU("a".to_string()),
            KeywordU {
                name: "boo".to_string(),
                id: "b".to_string()
            }
        ])),
        Ok(ListT(vec![
            KeywordT(Keyword {
                name: "a".to_string(),
                id: "a".to_string()
            }),
            KeywordT(Keyword {
                name: "boo".to_string(),
                id: "b".to_string()
            })
        ]))
    );
}
