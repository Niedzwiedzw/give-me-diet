use self::helpers::{surrounded_by_whitespace, whitespace};
use crate::{
    error::Res,
    models::{
        AmountOf, Eat, GMDLog, Gram, Kcal, LogEntry, ProductDefinition, ProductName, Quantity,
        StartDay, UnitOfMeasure,
    },
    type_name,
};
use chrono::NaiveDate;
use eyre::{eyre, Result, WrapErr};
use nom::{
    branch::alt,
    bytes::complete::take_while1,
    multi::{many1, separated_list1},
    sequence::{separated_pair, tuple},
    Parser,
};
use nom_supreme::{tag::complete::tag, ParserExt};
use nonempty::NonEmpty;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tap::prelude::*;
use tracing::Level;

pub mod keyword;

pub trait FromGMD: Sized {
    fn from_gmd(input: &str) -> Result<Self> {
        Self::parse
            .context(crate::type_name!())
            .complete()
            .all_consuming()
            .parse(input.trim())
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
    #[tracing::instrument(skip(input), ret(level = Level::TRACE))]
    fn parse(input: &str) -> Res<'_, Self> {
        tag("g").map(|_| Self).context(type_name!()).parse(input)
    }
}

impl FromGMD for Kcal {
    #[tracing::instrument(skip(input), ret(level = Level::TRACE))]
    fn parse(input: &str) -> Res<'_, Self> {
        tag("kcal").map(|_| Self).context(type_name!()).parse(input)
    }
}

impl FromGMD for UnitOfMeasure {
    #[tracing::instrument(skip(input), ret(level = Level::TRACE))]
    fn parse(input: &str) -> Res<'_, Self> {
        alt((Gram::parse.map(Self::from), Kcal::parse.map(Self::from)))
            .context(type_name!())
            .parse(input)
    }
}

impl FromGMD for Decimal {
    #[tracing::instrument(skip(input), ret(level = Level::TRACE))]
    fn parse(input: &str) -> Res<'_, Self> {
        const LEGAL: &[char] = &['-', '.', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];

        take_while1(|c: char| LEGAL.contains(&c))
            .map_res(Decimal::from_str_exact)
            .context(type_name!())
            .parse(input)
    }
}

enum SpecialUnitOfMeasure {
    Milligram,
    Microgram,
}

impl SpecialUnitOfMeasure {
    const MILI: Decimal = dec!(0.001);
    const MICRO: Decimal = dec!(0.000001);

    pub fn convert(self, amount: Decimal) -> Quantity {
        match self {
            SpecialUnitOfMeasure::Milligram => Quantity {
                amount: amount * Self::MILI,
                unit: Gram.into(),
            },
            SpecialUnitOfMeasure::Microgram => Quantity {
                amount: amount * Self::MICRO,
                unit: Gram.into(),
            },
        }
    }
}

impl FromGMD for SpecialUnitOfMeasure {
    fn parse(input: &str) -> Res<'_, Self> {
        alt((
            tag("mg").map(|_| Self::Milligram),
            tag("µg").map(|_| Self::Microgram),
        ))
        .context(type_name!())
        .parse(input)
    }
}

impl FromGMD for Quantity {
    #[tracing::instrument(skip(input), ret(level = Level::TRACE))]
    fn parse(input: &str) -> Res<'_, Self> {
        alt((
            tuple((Decimal::parse, SpecialUnitOfMeasure::parse))
                .map(|(amount, unit)| unit.convert(amount))
                .context(type_name!()),
            tuple((Decimal::parse, UnitOfMeasure::parse))
                .map(|(amount, unit)| Self { amount, unit })
                .context(type_name!()),
        ))
        .parse(input)
    }
}

pub mod helpers {
    use crate::error::Res;
    use nom::{bytes::complete::take_while1, Parser};
    use nom_supreme::{error::ErrorTree, ParserExt};

    pub fn whitespace(input: &str) -> Res<'_, ()> {
        take_while1(|c: char| c.is_whitespace())
            .context("whitespace")
            .map(|_| ())
            .context("whitespace")
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

impl<T: FromGMD + std::fmt::Debug> FromGMD for AmountOf<T> {
    #[tracing::instrument(skip(input), ret(level = Level::TRACE))]
    fn parse(input: &str) -> Res<'_, Self> {
        separated_pair(
            Quantity::parse,
            keyword::OF::tag.pipe(surrounded_by_whitespace),
            T::parse,
        )
        .map(|(quantity, inner)| AmountOf { quantity, inner })
        .context(type_name!())
        .parse(input)
    }
}

impl FromGMD for NaiveDate {
    #[tracing::instrument(skip(input), ret(level = Level::TRACE))]
    fn parse(input: &str) -> Res<'_, Self> {
        take_while1(|c: char| {
            ['.', '-', '/'].pipe(|legal| legal.contains(&c) || c.is_ascii_digit())
        })
        .map_res(|input| {
            ["%Y-%m-%d", "%Y.%m.%d", "%Y/%m/%d"].pipe(|formats| {
                formats
                    .into_iter()
                    .find_map(|format| NaiveDate::parse_from_str(input, format).ok())
                    .ok_or_else(|| eyre!("'{input}' matched no date format"))
                    .with_context(|| format!("supported formats are [{formats:?}]"))
            })
        })
        .context(type_name!())
        .parse(input)
    }
}

impl FromGMD for StartDay {
    #[tracing::instrument(skip(input), ret(level = Level::TRACE))]
    fn parse(input: &str) -> Res<'_, Self> {
        NaiveDate::parse
            .map(Self)
            .context(type_name!())
            .parse(input)
    }
}

impl FromGMD for ProductName {
    #[tracing::instrument(skip(input), ret(level = Level::TRACE))]
    fn parse(input: &str) -> Res<'_, Self> {
        take_while1(|c: char| c != '\n')
            .map(|v: &str| v.to_string().pipe(Self))
            .context(type_name!())
            .parse(input)
    }
}

impl FromGMD for ProductDefinition {
    #[tracing::instrument(skip(input), ret(level = Level::TRACE))]
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
        .context(type_name!())
        .parse(input)
    }
}

impl FromGMD for Eat {
    #[tracing::instrument(skip(input), ret(level = Level::TRACE))]
    fn parse(input: &str) -> Res<'_, Self> {
        AmountOf::<ProductName>::parse
            .preceded_by(keyword::EAT::tag.terminated(whitespace))
            .map(Self)
            .context(type_name!())
            .parse(input)
    }
}

impl FromGMD for LogEntry {
    #[tracing::instrument(skip(input), ret(level = Level::TRACE))]
    fn parse(input: &str) -> Res<'_, Self> {
        alt((
            Eat::parse.map(LogEntry::from),
            ProductDefinition::parse.map(LogEntry::from),
            StartDay::parse.map(LogEntry::from),
        ))
        .context(type_name!())
        .parse(input)
    }
}

impl FromGMD for GMDLog {
    #[tracing::instrument(skip(input), ret(level = Level::TRACE))]
    fn parse(input: &str) -> Res<'_, Self> {
        separated_list1(whitespace, LogEntry::parse)
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
