use eyre::{eyre, Result};
use nonempty::NonEmpty;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tap::prelude::*;

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, Clone, Copy, Default)]
pub struct Gram;

impl std::fmt::Display for Gram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "g")
    }
}

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, Clone, Copy, Default)]
pub struct Kcal;

impl std::fmt::Display for Kcal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "kcal")
    }
}

#[derive(
    Debug,
    Eq,
    PartialEq,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    derive_more::From,
    derive_more::Display,
)]
pub enum UnitOfMeasure {
    Gram(Gram),
    Kcal(Kcal),
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Copy)]
pub struct Quantity {
    pub amount: Decimal,
    pub unit: UnitOfMeasure,
}

impl std::fmt::Display for Quantity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.pipe_ref(|Self { amount, unit }| write!(f, "{}{}", amount.normalize(), unit))
    }
}

impl Quantity {
    pub fn try_add(&mut self, other: Self) -> Result<()> {
        match (self.unit, other.unit) {
            (one, two) if one == two => {
                self.amount += other.amount;
                Ok(())
            }
            (one, two) => Err(eyre!("incompatible units of measure: [{one}, {two}]")),
        }
    }
    pub fn of<T>(self, inner: T) -> AmountOf<T> {
        AmountOf {
            quantity: self,
            inner,
        }
    }
}

#[derive(
    Debug,
    Serialize,
    Deserialize,
    PartialOrd,
    Ord,
    PartialEq,
    Eq,
    Clone,
    transpare::Transpare,
    derive_more::Display,
    Hash,
)]
pub struct ProductNameKind<Name>(pub Name);

pub type ProductName = ProductNameKind<String>;

impl ProductName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy, transpare::Transpare, Serialize, Deserialize)]
pub struct AmountOf<Inner> {
    pub quantity: Quantity,
    pub inner: Inner,
}

impl<Inner> AmountOf<Inner> {
    pub fn as_ref_inner(&self) -> AmountOf<&Inner> {
        self.pipe_ref(|Self { quantity, inner }| AmountOf {
            quantity: *quantity,
            inner,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProductDefinition {
    pub name: ProductName,
    pub ingredients: Option<AmountOf<NonEmpty<AmountOf<ProductName>>>>,
}

impl ProductDefinition {
    pub fn primitive(name: impl Into<String>) -> Self {
        Self {
            name: ProductName::new(name),
            ingredients: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Eat(pub AmountOf<ProductName>);

#[derive(Debug, Serialize, Deserialize)]
pub struct StartDay(pub chrono::NaiveDate);

#[derive(Debug, Serialize, Deserialize, derive_more::From)]
pub enum LogEntry {
    StartDay(StartDay),
    Define(ProductDefinition),
    Eat(Eat),
}

/// define 30g of Pasibus Avocadus:
///  - 10g of protein
///  
/// 2024-01-20
/// eat Pasibus Avocadus
/// eat 50g of Jogurt grecki lidl
#[derive(Debug, Serialize, Deserialize)]
pub struct GMDLog(pub Vec<LogEntry>);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calculator::GMDSummary;
    use chrono::NaiveDate;
    use eyre::{ContextCompat, Result};
    use itertools::Itertools;
    use pretty_assertions::assert_eq;
    use rust_decimal::prelude::FromPrimitive;
    use rust_decimal_macros::dec;
    use std::iter::{empty, once};

    #[test]
    fn test_sample_input() -> Result<()> {
        fn g(v: f32) -> Quantity {
            Quantity {
                amount: Decimal::from_f32(v).unwrap(),
                unit: Gram.into(),
            }
        }

        NaiveDate::from_ymd_opt(2024, 1, 27).unwrap().pipe(|today| {
            let carbohydrates = "Carbohydrates".pipe(ProductDefinition::primitive);
            let fat = "Fat".pipe(ProductDefinition::primitive);
            let protein = "Protein".pipe(ProductDefinition::primitive);
            let fiber = "Fiber".pipe(ProductDefinition::primitive);
            let water = "Water".pipe(ProductDefinition::primitive);

            empty()
                .chain(StartDay(today).pipe(once).map(LogEntry::from))
                .chain(protein.clone().pipe(once).map(LogEntry::from))
                .chain(fat.clone().pipe(once).map(LogEntry::from))
                .chain(fiber.clone().pipe(once).map(LogEntry::from))
                .chain(carbohydrates.clone().pipe(once).map(LogEntry::from))
                .chain(water.clone().pipe(once).map(LogEntry::from))
                .chain(
                    ProductDefinition {
                        name: "Frytki".pipe(ProductName::new),
                        ingredients: empty()
                            .chain(
                                carbohydrates
                                    .name
                                    .clone()
                                    .pipe(|product| g(41.44).of(product))
                                    .pipe(once),
                            )
                            .chain(
                                fat.name
                                    .clone()
                                    .pipe(|product| g(14.73).of(product))
                                    .pipe(once),
                            )
                            .chain(
                                protein
                                    .name
                                    .clone()
                                    .pipe(|product| g(3.43).of(product))
                                    .pipe(once),
                            )
                            .chain(
                                fiber
                                    .name
                                    .clone()
                                    .pipe(|product| g(3.8).of(product))
                                    .pipe(once),
                            )
                            .chain(
                                water
                                    .name
                                    .clone()
                                    .pipe(|product| g(38.55).of(product))
                                    .pipe(once),
                            )
                            .collect_vec()
                            .pipe(NonEmpty::from_vec)
                            .map(|per_100g| g(100.).of(per_100g)),
                    }
                    .pipe(once)
                    .map(LogEntry::from),
                )
                .chain(
                    Eat(g(100.).of("Frytki".pipe(ProductName::new)))
                        .pipe(once)
                        .map(LogEntry::from),
                )
                .collect::<Vec<_>>()
                .pipe(|log| {
                    log.pipe(GMDLog)
                        .tap(|log| println!("LOG\n{log:#?}\n"))
                        .pipe_ref(GMDSummary::from_log)
                        .tap_ok(|summary| println!("SUMMARY\n{summary:#?}\n"))
                        .and_then(|summary| {
                            assert_eq!(
                                Quantity {
                                    amount: dec!(3.43),
                                    unit: Gram.into()
                                },
                                *summary
                                    .0
                                    .get(&today)
                                    .context("no such day")
                                    .and_then(|day| day
                                        .state
                                        .get(&ProductName::new("Protein"))
                                        .context("no such product"))?,
                            );
                            Ok(())
                        })
                })
        })
    }
}
