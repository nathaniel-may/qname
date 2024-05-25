use super::{ExprU, ExprU::*, SchemaParseError};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_till},
    character::complete::{alpha1, char, newline, space0, space1, u8},
    combinator::{all_consuming, complete, eof, recognize},
    error::{ErrorKind, ParseError},
    multi::{many0, many0_count, many1},
    sequence::{delimited, pair, preceded, separated_pair, terminated},
    Err, IResult, InputLength, Parser,
};
use std::result::Result as StdResult;

pub type Result<T> = StdResult<T, SchemaParseError>;

type NomParseResult<'a, O> = StdResult<(&'a str, O), nom::Err<NomParseError<&'a str>>>;

#[derive(Debug, PartialEq)]
pub enum NomParseError<I> {
    Custom(SchemaParseError),
    Nom(I, ErrorKind),
}

impl<I> ParseError<I> for NomParseError<I> {
    fn from_error_kind(input: I, kind: ErrorKind) -> Self {
        NomParseError::Nom(input, kind)
    }

    fn append(_: I, _: ErrorKind, other: Self) -> Self {
        other
    }
}

impl<I> From<SchemaParseError> for NomParseError<I> {
    fn from(e: SchemaParseError) -> Self {
        NomParseError::Custom(e)
    }
}

impl<I> From<(I, ErrorKind)> for NomParseError<I> {
    fn from(e: (I, ErrorKind)) -> Self {
        NomParseError::Nom(e.0, e.1)
    }
}

impl<I> From<nom::Err<(I, ErrorKind)>> for NomParseError<I> {
    fn from(e: nom::Err<(I, ErrorKind)>) -> Self {
        match e {
            nom::Err::Failure((input, kind)) => NomParseError::Nom(input, kind),
            nom::Err::Error((input, kind)) => NomParseError::Nom(input, kind),
            // TODO handle this better
            nom::Err::Incomplete(_) => {
                panic!("attempted to convert from nom::Err::Incomplete.")
            }
        }
    }
}

pub fn parse(input: &str) -> Result<ExprU> {
    match complete(expr).parse(input) {
        Ok((_, schema @ FnU { .. })) => Ok(schema),
        Ok((_, _)) => Err(SchemaParseError::MustStartWithSchemaConstructor),
        Err(e) => match e {
            nom::Err::Error(e) | nom::Err::Failure(e) => match e {
                NomParseError::Custom(e) => Err(e),
                NomParseError::Nom(input, _kind) => {
                    println!("{_kind:?}");
                    Err(SchemaParseError::UnexpectedInput(input.to_string()))
                }
            },
            // call to complete skips this branch
            nom::Err::Incomplete(_) => {
                panic!("reached unreachable nom::Err::Incomplete while parsing.")
            }
        },
    }
}

fn expr(input: &str) -> NomParseResult<ExprU> {
    alt((
        parens(expr),
        list,
        nat.map(NatU),
        // keywords are above commands because the syntax leads with a string
        keyword,
        string.map(StringU),
        func,
    ))(input)
}

fn func(input: &str) -> NomParseResult<ExprU> {
    let (input, name) = lexeme_vert_allowed(identifier).parse(input)?;
    let (input, args) = all_consuming(many0(lexeme_vert_allowed(expr)))(input)?;
    Ok((
        input,
        FnU {
            name: name.to_string(),
            args,
        },
    ))
}

fn list(input: &str) -> NomParseResult<ExprU> {
    let (rest, inside) = between(&'[', &']').parse(input)?;
    let (_, args) = all_consuming(alt((
        sep_by1(
            delimited(line_space0, tag(","), line_space0),
            delimited(line_space0, expr, line_space0),
        ),
        line_space0.map(|_| vec![]),
    )))(inside)?;
    Ok((rest, ListU(args)))
}

fn identifier(input: &str) -> NomParseResult<&str> {
    recognize(pair(alpha1, many0_count(alt((alpha1, tag("-"))))))(input)
}

fn keyword(input: &str) -> NomParseResult<ExprU> {
    let (rest, (name, id)) = separated_pair(string, tag("/"), string)(input)?;
    Ok((rest, KeywordU { name, id }))
}

fn string(input: &str) -> NomParseResult<String> {
    between(&'"', &'"').map(|x| x.to_string()).parse(input)
}

fn nat(input: &str) -> NomParseResult<u8> {
    u8(input)
}

fn indent(input: &str) -> NomParseResult<&str> {
    alt((tag("  "), tag("\t")))(input).map(|(rest, _)| (rest, ""))
}

fn line_space0(input: &str) -> NomParseResult<&str> {
    many0(alt((char(' '), char('\t'), char('\n'))))(input).map(|(rest, _)| (rest, ""))
}

fn parens<'a, F, O, E: ParseError<&'a str>>(inner: F) -> impl Parser<&'a str, O, E>
where
    F: Parser<&'a str, O, E>,
{
    between(&'(', &')').and_then(inner)
}

/// takes two characters and returns a parser for the str between them.
fn between<'a, E: ParseError<&'a str>>(
    l: &'static char,
    r: &'static char,
) -> impl Parser<&'a str, &'a str, E> {
    |input: &'a str| terminated(preceded(char(*l), take_till(|c| c == *r)), char(*r))(input)
}

// inspired by the implementation of many0
pub fn sep_by1<I, O1, O2, E: ParseError<I>, F, G>(
    mut sep: F,
    mut value: G,
) -> impl FnMut(I) -> IResult<I, Vec<O2>, E>
where
    I: InputLength + Clone,
    F: Parser<I, O1, E>,
    G: Parser<I, O2, E>,
{
    move |i: I| {
        let (i, x) = value.parse(i)?;
        let mut acc = Vec::with_capacity(4);
        acc.push(x);
        let mut i = i;
        loop {
            let len = i.input_len();
            match sep.parse(i.clone()) {
                Err(Err::Error(_)) => return Ok((i, acc)),
                Err(e) => return Err(e),
                Ok((i2, _)) => match value.parse(i2.clone()) {
                    Err(Err::Error(_)) => return Ok((i, acc)),
                    Err(e) => return Err(e),
                    Ok((i3, x)) => {
                        // infinite loop check: the parser must always consume
                        if i3.input_len() == len {
                            return Err(Err::Error(E::from_error_kind(i, ErrorKind::Many0)));
                        }
                        i = i3;
                        acc.push(x)
                    }
                },
            }
        }
    }
}

/// requries trailing whitespace or end of input
fn lexeme<'a, F, O, E: ParseError<&'a str>>(inner: F) -> impl Parser<&'a str, O, E>
where
    F: Parser<&'a str, O, E>,
{
    terminated(inner, alt((space1, eof)))
}

/// requries trailing whitespace, newline, or end of input
fn lexeme_vert_allowed<'a, F, O>(inner: F) -> impl Parser<&'a str, O, NomParseError<&'a str>>
where
    F: Parser<&'a str, O, NomParseError<&'a str>>,
{
    terminated(
        inner,
        alt((
            pair(pair(space0, many1(newline)), indent).map(|_| ""),
            alt((space1, eof)),
        )),
    )
}

/// requries trailing whitespace or end of input
fn symbol<'a, E: ParseError<&'a str>>(s: &'a str) -> impl Parser<&'a str, &'a str, E> {
    terminated(tag(s), alt((space1, eof)))
}

#[test]
fn top_level() {
    //   let input = r#"schema "-" "_"
    // [ category "Media" (exactly 1) ["art", "photo"/"ph", "video"/"v"]
    // ]"#;
    let input = r#"schema "-" "_"
  [ category []
  ]"#;

    let expr = FnU {
        name: "schema".to_string(),
        args: vec![
            StringU("-".to_string()),
            StringU("_".to_string()),
            ListU(vec![FnU {
                name: "category".to_string(),
                args: vec![
                    StringU("Media".to_string()),
                    FnU {
                        name: "exactly".to_string(),
                        args: vec![NatU(1)],
                    },
                    ListU(vec![
                        StringU("art".to_string()),
                        KeywordU {
                            name: "photo".to_string(),
                            id: "ph".to_string(),
                        },
                        KeywordU {
                            name: "video".to_string(),
                            id: "v".to_string(),
                        },
                    ]),
                ],
            }]),
        ],
    };

    // assert_eq!(Ok(expr), parse(input));
    assert!(parse(input).is_ok())
}

#[test]
fn parse_func() {
    let foo0 = FnU {
        name: "foo".to_string(),
        args: vec![NatU(0)],
    };

    let fool01 = FnU {
        name: "foo".to_string(),
        args: vec![ListU(vec![NatU(0), NatU(1)])],
    };

    let foo99l01 = FnU {
        name: "foo".to_string(),
        args: vec![NatU(99), ListU(vec![NatU(0), NatU(1)])],
    };

    assert_eq!(func("foo 0"), Ok(("", foo0.clone())));
    assert_eq!(func("foo\n  0"), Ok(("", foo0.clone())));
    assert_eq!(func("foo\n\t0"), Ok(("", foo0.clone())));
    assert_eq!(func("foo \n  0"), Ok(("", foo0.clone())));
    assert_eq!(func("foo \n  [ 0\n  , 1\n  ]"), Ok(("", fool01.clone())));
    assert_eq!(
        func("foo 99\n  [ 0\n  , 1\n  ]"),
        Ok(("", foo99l01.clone()))
    );
    assert!(func(r#"category "Media" (exactly 1) ["art", "photo"/"ph", "video"/"v"]"#).is_ok())
}

#[test]
fn parse_list() {
    assert_eq!(list("[]"), Ok(("", ListU(vec![]))));
    assert_eq!(list("[[]]"), Ok(("", ListU(vec![ListU(vec![])]))));
    assert_eq!(list("[0,1]"), Ok(("", ListU(vec![NatU(0), NatU(1)]))));
    assert_eq!(list("[0, 1]"), Ok(("", ListU(vec![NatU(0), NatU(1)]))));
    assert_eq!(list("[ 0 ]"), Ok(("", ListU(vec![NatU(0)]))));
    assert_eq!(list("[ ]"), Ok(("", ListU(vec![]))));
    assert_eq!(list("[\n\t]"), Ok(("", ListU(vec![]))));
    assert_eq!(list("[ 0\n\t]"), Ok(("", ListU(vec![NatU(0)]))));
    assert_eq!(list("[ 0\n, 1\n]"), Ok(("", ListU(vec![NatU(0), NatU(1)]))));
}

#[test]
fn parse_keyword() {
    assert_eq!(
        keyword(r#""abc"/"a""#),
        Ok((
            "",
            KeywordU {
                name: "abc".to_string(),
                id: "a".to_string()
            }
        ))
    );
}

#[test]
fn parse_line_space0() {
    assert_eq!(line_space0(""), Ok(("", "")));
    assert_eq!(line_space0(" x"), Ok(("x", "")));
    assert_eq!(line_space0("\n   \t x"), Ok(("x", "")));
}

#[test]
fn parse_sep_by1() {
    let alpha1 = alpha1::<&str, (&str, ErrorKind)>;
    assert!(sep_by1(tag(","), alpha1)("").is_err());
    assert_eq!(sep_by1(tag(","), alpha1)("a"), Ok(("", vec!["a"])));
    assert_eq!(
        sep_by1(tag(","), alpha1)("a,b,c123"),
        Ok(("123", vec!["a", "b", "c"]))
    );
}
