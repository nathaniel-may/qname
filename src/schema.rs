use crate::app::{to_empty_state, State, UiCategory};
use crate::error::{Error::ConfigParse, Result};
use nom::branch::alt;
use nom::bytes::complete::tag;
use nom::bytes::streaming::take_until;
use nom::combinator::{eof, fail};
use nom::error::ParseError;
use nom::multi::many0;
use nom::sequence::terminated;
#[cfg(test)]
use quickcheck::Arbitrary;
use serde::Deserialize;
use std::fmt;
use std::result::Result as StdResult;
#[cfg(test)]
use Requirement::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilenameParseError {}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Deserialize)]
pub struct Schema {
    pub delim: String,
    pub categories: Vec<Category>,
}

impl Schema {
    fn parse(&self, input: &str) -> StdResult<State, FilenameParseError> {
        fn to_alt<'a, E>(
            tags: &'a [String],
        ) -> impl Fn(&'a str) -> nom::IResult<&'a str, &'a str, E>
        where
            E: ParseError<&'a str>,
        {
            move |i: &'a str| match tags {
                [] => fail(i),
                [h, t @ ..] => alt((tag(h.as_str()), to_alt(t)))(i),
            }
        }

        let (input, salt) = take_until(self.delim.as_str())(input).unwrap();
        let salt = salt.to_string();
        let mut input = input;

        let mut categories = Vec::with_capacity(self.categories.len());

        for cat in self.categories {
            let tag_parser = to_alt(&cat.values);
            let (i, tags) =
                many0(terminated(tag_parser, alt((tag(self.delim.as_str()), eof))))(input).unwrap();

            // update input
            input = i;

            let values = cat
                .values
                .into_iter()
                .map(|name| (name, tags.contains(&&name.as_str())))
                .collect();

            categories.push(UiCategory {
                name: cat.name.clone(),
                values,
            });
        }

        let state = State { salt, categories };
        Ok(state)
    }
}

#[cfg(test)]
impl Arbitrary for Schema {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let mut delim = char::arbitrary(g).to_string();
        if bool::arbitrary(g) {
            delim.push(char::arbitrary(g))
        }

        Schema {
            delim,
            categories: Arbitrary::arbitrary(g),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(
            self.categories
                .shrink()
                .map(|categories| Schema {
                    // two char delims will cause different problems than single char delims. don't shrink.
                    delim: self.delim.clone(),
                    categories,
                })
                .collect::<Vec<_>>()
                .into_iter(),
        )
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Deserialize)]
pub struct Category {
    pub name: String,
    pub rtype: Requirement,
    pub rvalue: usize,
    pub values: Vec<String>,
}

#[cfg(test)]
impl Arbitrary for Category {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Category {
            name: Arbitrary::arbitrary(g),
            rtype: Arbitrary::arbitrary(g),
            rvalue: *g.choose(&[0, 1, 2, 3]).unwrap(),
            values: Arbitrary::arbitrary(g),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(
            self.values
                .shrink()
                .map(|values| Category {
                    name: self.name.shrink().next().unwrap_or(self.name.clone()),
                    rtype: self.rtype,
                    rvalue: if self.rvalue == 0 { 0 } else { self.rvalue - 1 },
                    values,
                })
                .collect::<Vec<_>>()
                .into_iter(),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Deserialize)]
pub enum Requirement {
    Exactly,
    AtLeast,
    AtMost,
}

#[cfg(test)]
impl Arbitrary for Requirement {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        *g.choose(&[Exactly, AtLeast, AtMost]).unwrap()
    }

    // no way to shrink this value
}

impl fmt::Display for Requirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exactly => write!(f, "exactly"),
            Self::AtLeast => write!(f, "at least"),
            Self::AtMost => write!(f, "at most"),
        }
    }
}

pub fn parse_schema(contents: &str) -> Result<Schema> {
    serde_dhall::from_str(contents)
        .parse()
        .map_err(|e| ConfigParse(Box::new(e)))
}

#[test]
fn init_config_file_parses() {
    use std::fs;
    use std::path::Path;

    use crate::schema::Category;
    use crate::schema::Requirement::*;

    let expected = Schema {
        delim: "-".to_string(),
        categories: vec![
            Category {
                name: "Medium".to_string(),
                rtype: Exactly,
                rvalue: 1,
                values: vec![
                    "art".to_string(),
                    "photo".to_string(),
                    "ai".to_string(),
                    "other".to_string(),
                ],
            },
            Category {
                name: "Subject".to_string(),
                rtype: AtLeast,
                rvalue: 0,
                values: vec![
                    "plants".to_string(),
                    "animals".to_string(),
                    "people".to_string(),
                ],
            },
        ],
    };

    match parse_schema(&fs::read_to_string(Path::new("./src/init.dhall")).unwrap()) {
        Err(e) => panic!("{e}"),
        Ok(schema) => assert_eq!(expected, schema),
    }
}

#[cfg(test)]
mod prop_tests {
    use crate::app::to_empty_state;

    use super::Schema;
    use quickcheck::{Gen, QuickCheck};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    // schemas should be able to parse the filenames they generate
    // TODO this does not include the salt and it should
    #[test]
    fn parse_generated_schemas() {
        fn closed_loop(schema: Schema, selection: u32, seed: u64) -> bool {
            // quickcheck doesn't have a great way to generate bool values larger than the gen size
            // so I'm using this u32 like each bit is an arbitrary bool.
            let mut bool_selection = Vec::with_capacity(32);
            for i in 0..32 {
                let test = 1 << i;
                bool_selection.push(test & selection == test)
            }

            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let mut state = to_empty_state(&schema, &mut rng);
            let mut selection = bool_selection.to_vec();
            for cat in &mut state.categories[..] {
                let tags = cat.values.clone().into_iter().map(|(s, _)| s);
                let size = tags.len();
                cat.values = tags.zip(selection.drain(0..size)).collect();
            }

            match crate::filename::selection_to_filename(&schema, &state) {
                Err(_) => false,
                Ok(filename) => match schema.parse(&filename) {
                    Err(_) => false,
                    Ok(parsed_state) => parsed_state == state,
                },
            }
        }

        QuickCheck::new()
            .gen(Gen::new(5))
            .quickcheck(closed_loop as fn(Schema, u32, u64) -> bool);
    }
}
