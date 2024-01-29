pub mod models;
pub mod parser;
pub mod error {
    use nom::IResult;
    use nom_supreme::error::ErrorTree;

    pub type Res<'a, T> = IResult<&'a str, T, ErrorTree<&'a str>>;
}

use itertools::Itertools;
use models::ProductDefinition;
use nonempty::NonEmpty;
use tap::prelude::*;

#[macro_export]
macro_rules! type_name {
    () => {
        std::any::type_name::<Self>()
            .split("::")
            .last()
            .unwrap_or(std::any::type_name::<Self>())
    };
}

#[derive(Debug, Clone, Copy)]
pub struct Key<T>(T);

impl<'input> PartialOrd for Key<&'input ProductDefinition> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<'input> Ord for Key<&'input ProductDefinition> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.name.cmp(&other.0.name)
    }
}
impl<'input> PartialEq for Key<&'input ProductDefinition> {
    fn eq(&self, other: &Self) -> bool {
        self.0.name.eq(&other.0.name)
    }
}

impl<'input> Eq for Key<&'input ProductDefinition> {}

#[extension_traits::extension(pub trait NonEmptyExt)]
impl<T> NonEmpty<T> {
    fn as_ref(&self) -> NonEmpty<&T> {
        self.iter()
            .collect_vec()
            .pipe(NonEmpty::from_vec)
            .expect("this is legal because we're collecting from other NonEmpty")
    }
}

pub mod calculator {
    use crate::{
        models::{
            AmountOf, Eat, GMDLog, LogEntry, ProductDefinition, ProductName, Quantity, StartDay,
        },
        NonEmptyExt,
    };
    use eyre::{eyre, ContextCompat, Result, WrapErr};
    use itertools::Itertools;
    use nonempty::NonEmpty;
    use rust_decimal::Decimal;
    use std::{
        collections::BTreeMap,
        iter::once,
        ops::{Div, Mul, MulAssign},
    };
    use tap::prelude::*;
    use tracing::warn;

    #[derive(Default, Debug)]
    pub struct GMDDay<'input> {
        pub state: BTreeMap<ProductName, Vec<Quantity>>,
        pub defined_products: BTreeMap<&'input ProductName, &'input ProductDefinition>,
    }

    fn boxed<'a, T>(iter: impl Iterator<Item = T> + 'a) -> Box<dyn Iterator<Item = T> + 'a> {
        iter.pipe(Box::new)
    }

    #[derive(Default, Debug)]
    pub struct GMDSummary<'input>(pub BTreeMap<chrono::NaiveDate, GMDDay<'input>>);

    #[derive(Debug, Clone, Copy)]
    pub struct Ratio(Decimal);

    impl Mul<Ratio> for Quantity {
        type Output = Quantity;

        fn mul(self, Ratio(ratio): Ratio) -> Self::Output {
            self.pipe(|Self { amount, unit }| {
                ratio
                    .mul(amount)
                    .pipe(|amount| Self::Output { amount, unit })
            })
        }
    }

    impl MulAssign<Ratio> for Quantity {
        fn mul_assign(&mut self, ratio: Ratio) {
            *self = *self * ratio;
        }
    }

    impl<T> Mul<Ratio> for AmountOf<T> {
        type Output = Self;

        fn mul(self, ratio: Ratio) -> Self::Output {
            self.tap_mut(|Self { quantity, .. }| {
                *quantity *= ratio;
            })
        }
    }

    impl Quantity {
        pub fn ratio(self, other: Quantity) -> Option<Ratio> {
            match (self.unit, other.unit) {
                (left, right) if left == right => left.pipe(Some),
                _other => None,
            }
            .map(|_| self.amount.div(&other.amount).pipe(Ratio))
        }
    }

    struct GMDSummaryBuilder<'input> {
        current: GMDSummary<'input>,
        current_day: chrono::NaiveDate,
    }

    impl Quantity {
        pub fn of_ingredient(
            self,
            product: &ProductDefinition,
        ) -> Result<NonEmpty<AmountOf<&ProductName>>> {
            product
                .ingredients
                .as_ref()
                .ok_or_else(|| eyre!("product [{:?}] has no ingredients", product.name))
                .and_then(|ingredients| {
                    ingredients.inner.as_ref().try_map(|ingredient| {
                        ingredient
                            .quantity
                            .ratio(ingredients.quantity)
                            .ok_or_else(|| {
                                eyre!(
                                    "cannot calculate ratio of [{:?}] within {:?}",
                                    ingredient.quantity,
                                    ingredients.quantity
                                )
                            })
                            .with_context(|| {
                                format!(
                                    "calculating ratio of [{:?}] in [{:?}]",
                                    ingredient.inner, product.name
                                )
                            })
                            .map(|ratio| {
                                self.mul(ratio).pipe(|quantity| AmountOf {
                                    quantity,
                                    inner: &ingredient.inner,
                                })
                            })
                    })
                })
        }
    }

    impl<'input> GMDSummaryBuilder<'input> {
        pub fn definition<'name, 'state: 'name>(
            &'state self,
            product_name: &'name ProductName,
        ) -> Option<&'state ProductDefinition> {
            self.pipe(
                |Self {
                     current,
                     current_day,
                 }| {
                    current
                        .0
                        .range(..=current_day)
                        .rev()
                        .find_map(|(_, day)| day.defined_products.get(product_name).copied())
                },
            )
        }
        pub fn flatten_product_once(
            &self,
            product: &ProductDefinition,
            amount: Quantity,
        ) -> Result<NonEmpty<AmountOf<&ProductDefinition>>> {
            amount.of_ingredient(product).and_then(|ingredients| {
                ingredients.try_map(|ingredient| {
                    ingredient.try_map_inner(|name| {
                        self.definition(name)
                            .with_context(|| format!("no such product: [{name:?}]"))
                    })
                })
            })
        }

        pub fn flatten_product<'state, 'product: 'state>(
            &'state self,
            product: &'product ProductDefinition,
            quantity: Quantity,
        ) -> impl Iterator<Item = AmountOf<&'state ProductName>> + 'state {
            match self.flatten_product_once(product, quantity) {
                Ok(more) => more
                    .into_iter()
                    .flat_map(
                        |AmountOf {
                             quantity,
                             inner: product,
                         }| self.flatten_product(product, quantity),
                    )
                    .pipe(boxed),
                Err(message) => {
                    warn!(?message, "flattening product");
                    quantity.of(&product.name).pipe(once).pipe(boxed)
                }
            }
        }
    }

    impl<'input> GMDSummaryBuilder<'input> {
        pub fn new() -> Self {
            Self {
                current: Default::default(),
                current_day: chrono::Local::now().date_naive(),
            }
        }
    }

    impl<'input> GMDSummary<'input> {
        pub fn from_log(log: &'input GMDLog) -> Result<Self> {
            log.0
                .iter()
                .try_fold(GMDSummaryBuilder::new(), |acc, next| match next {
                    LogEntry::StartDay(StartDay(day)) => acc
                        .tap_mut(|acc| {
                            acc.current.0.entry(*day).or_default();
                        })
                        .pipe(Ok),
                    LogEntry::Define(product) => acc
                        .tap_mut(|acc| {
                            acc.current
                                .0
                                .entry(acc.current_day)
                                .or_default()
                                .defined_products
                                .insert(&product.name, product);
                        })
                        .pipe(Ok),
                    LogEntry::Eat(Eat(AmountOf {
                        quantity,
                        inner: product_name,
                    })) => acc
                        .definition(product_name)
                        .with_context(|| format!("product [{product_name:?}] is not defined"))
                        .map(|definition| {
                            acc.flatten_product(definition, *quantity)
                                .map(|inner| inner.map_inner(Clone::clone))
                                .collect_vec()
                        })
                        .map(|eaten| {
                            acc.tap_mut(|acc| {
                                eaten.into_iter().for_each(
                                    |AmountOf {
                                         quantity,
                                         inner: product_name,
                                     }| {
                                        acc.current
                                            .0
                                            .entry(acc.current_day)
                                            .or_default()
                                            .state
                                            .entry(product_name)
                                            .or_default()
                                            .push(quantity)
                                    },
                                )
                            })
                        }),
                })
                .map(|GMDSummaryBuilder { current, .. }| current)
        }
    }
}

#[cfg(test)]
mod tests;
