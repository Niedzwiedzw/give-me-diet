use nom::Parser;
use nom_supreme::ParserExt;

use super::Res;

#[derive(Clone, Copy, Debug)]
pub struct Keyword {
    value: &'static str,
}

impl From<Keyword> for String {
    fn from(val: Keyword) -> Self {
        val.value.into()
    }
}

impl std::fmt::Display for Keyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl Keyword {
    pub fn tag<'a>(&self, input: &'a str) -> Res<'a, Self> {
        nom_supreme::tag::complete::tag(self.value)
            .map(move |_| *self)
            .context(self.value)
            .parse(input)
    }
}

macro_rules! keyword {
    ($name:ident, $value:literal) => {
        #[allow(non_camel_case_types)]
        #[derive(Debug, Clone, Copy)]
        pub struct $name;
        impl $name {
            pub fn tag(input: &str) -> Res<'_, Self> {
                nom_supreme::tag::complete::tag($value)
                    .map(|_| Self)
                    .context($value)
                    .parse(input)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                $value.fmt(f)
            }
        }

        impl From<$name> for String {
            fn from(_: $name) -> String {
                String::from($value)
            }
        }
    };
}

keyword!(MINUS, "-");
keyword!(PERCENT, "%");
keyword!(OF, "of");
keyword!(DEFINE, "of");
keyword!(EAT, "eat");
