use self::helpers::{surrounded_by_whitespace, whitespace};
use crate::{
    error::Res,
    models::{
        AmountOf, Eat, GMDLog, Gram, LogEntry, ProductDefinition, ProductName, Quantity, StartDay,
        UnitOfMeasure,
    },
};
use chrono::NaiveDate;
use eyre::{eyre, Result, WrapErr};
use nom::{
    branch::alt,
    bytes::complete::take_while1,
    multi::{many1, separated_list0},
    sequence::{separated_pair, tuple},
    Parser,
};
use nom_supreme::{tag::complete::tag, ParserExt};
use nonempty::NonEmpty;
use rust_decimal::Decimal;
use tap::prelude::*;

pub mod keyword;

pub trait FromGMD: Sized {
    fn from_gmd(input: &str) -> Result<Self> {
        Self::parse
            .context(crate::type_name!())
            .complete()
            .all_consuming()
            .parse(input)
            .map_err(|e| eyre!("{e:?}"))
            .map(|(_, v)| v)
            .with_context(|| format!("Parsing [{}]", crate::type_name!()))
    }
    fn parse(input: &str) -> Res<'_, Self>;
}

pub trait ToGMD {
    fn to_gmd(&self) -> String;
}

impl FromGMD for Gram {
    fn parse(input: &str) -> Res<'_, Self> {
        tag("g").map(|_| Self).parse(input)
    }
}

impl FromGMD for UnitOfMeasure {
    fn parse(input: &str) -> Res<'_, Self> {
        Gram::parse.map(Self::from).parse(input)
    }
}

impl FromGMD for Decimal {
    fn parse(input: &str) -> Res<'_, Self> {
        const LEGAL: &[char] = &['-', '.', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];

        take_while1(|c: char| LEGAL.contains(&c))
            .map_res(Decimal::from_str_exact)
            .parse(input)
    }
}

impl FromGMD for Quantity {
    fn parse(input: &str) -> Res<'_, Self> {
        tuple((Decimal::parse, UnitOfMeasure::parse))
            .map(|(amount, unit)| Self { amount, unit })
            .parse(input)
    }
}

pub mod helpers {
    use nom::{bytes::complete::take_while1, Parser};
    use nom_supreme::{error::ErrorTree, ParserExt};

    use crate::error::Res;

    pub fn whitespace(input: &str) -> Res<'_, ()> {
        take_while1(|c: char| c.is_whitespace())
            .context("whitespace")
            .map(|_| ())
            .parse(input)
    }

    pub fn surrounded_by_whitespace<'a, T>(
        mut parser: impl Parser<&'a str, T, ErrorTree<&'a str>>,
    ) -> impl FnMut(&'a str) -> Res<'a, T> {
        move |input: &'a str| {
            let (input, _) = whitespace.parse(input)?;
            let (input, t) = parser.parse(input)?;
            let (input, _) = whitespace.parse(input)?;
            Ok((input, t))
        }
    }
}

impl<T: FromGMD> FromGMD for AmountOf<T> {
    fn parse(input: &str) -> Res<'_, Self> {
        separated_pair(
            Quantity::parse,
            keyword::OF::tag.pipe(surrounded_by_whitespace),
            T::parse,
        )
        .map(|(quantity, inner)| AmountOf { quantity, inner })
        .parse(input)
    }
}

impl FromGMD for NaiveDate {
    fn parse(input: &str) -> Res<'_, Self> {
        take_while1(|c: char| {
            ['.', '-', '/'].pipe(|legal| legal.contains(&c) || c.is_ascii_digit())
        })
        .map_res(|input| {
            ["%Y-%m-%d", "%Y.%m.%d", "%Y/%m/%d", ].pipe(|formats| {
                formats
                    .into_iter()
                    .find_map(|format| NaiveDate::parse_from_str(input, format).ok())
                    .ok_or_else(|| eyre!("'{input}' matched no date format - supported formats are [{formats:?}]"))
            })
        }).parse(input)
    }
}

impl FromGMD for StartDay {
    fn parse(input: &str) -> Res<'_, Self> {
        NaiveDate::parse.map(Self).parse(input)
    }
}

impl FromGMD for ProductName {
    fn parse(input: &str) -> Res<'_, Self> {
        take_while1(|c: char| c != '\n')
            .map(|v: &str| v.to_string().pipe(Self))
            .parse(input)
    }
}

impl FromGMD for ProductDefinition {
    fn parse(input: &str) -> Res<'_, Self> {
        tuple((
            AmountOf::<ProductName>::parse.preceded_by(keyword::DEFINE::tag.terminated(whitespace)),
            AmountOf::<ProductName>::parse
                .preceded_by(tag("-").pipe(surrounded_by_whitespace))
                .pipe(many1)
                .opt(),
        ))
        .map(
            |(
                AmountOf {
                    quantity,
                    inner: name,
                },
                ingredients,
            )| {
                ingredients
                    .and_then(NonEmpty::from_vec)
                    .pipe(|ingredients| ProductDefinition {
                        name,
                        ingredients: ingredients.map(|ingredients| quantity.of(ingredients)),
                    })
            },
        )
        .parse(input)
    }
}

impl FromGMD for Eat {
    fn parse(input: &str) -> Res<'_, Self> {
        AmountOf::<ProductName>::parse
            .preceded_by(keyword::EAT::tag.terminated(whitespace))
            .map(Self)
            .parse(input)
    }
}

impl FromGMD for LogEntry {
    fn parse(input: &str) -> Res<'_, Self> {
        alt((
            Eat::parse.map(LogEntry::from),
            ProductDefinition::parse.map(LogEntry::from),
            StartDay::parse.map(LogEntry::from),
        ))
        .parse(input)
    }
}

impl FromGMD for GMDLog {
    fn parse(input: &str) -> Res<'_, Self> {
        separated_list0(many1(tag("\n")), LogEntry::parse)
            .map(Self)
            .parse(input)
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    use super::*;

    #[test]
    fn test_unit_of_measure() -> Result<()> {
        assert_eq!(UnitOfMeasure::from_gmd("g")?, UnitOfMeasure::Gram(Gram));
        assert!(UnitOfMeasure::from_gmd("e").is_err());
        assert!(UnitOfMeasure::from_gmd(" ").is_err());
        Ok(())
    }

    #[test]
    fn test_decimal() -> Result<()> {
        assert!(Decimal::from_gmd("g").is_err());
        assert_eq!(Decimal::from_gmd("1")?, dec!(1.0));
        assert_eq!(Decimal::from_gmd("21.37")?, dec!(21.37));
        assert_eq!(Decimal::from_gmd("-21.37")?, dec!(-21.37));
        assert!(Decimal::from_gmd("21-37").is_err());
        Ok(())
    }

    #[test]
    fn test_quantity() -> Result<()> {
        assert_eq!(
            Quantity::from_gmd("10g")?,
            Quantity {
                amount: dec!(10.0),
                unit: UnitOfMeasure::Gram(Gram)
            }
        );
        Ok(())
    }
}
