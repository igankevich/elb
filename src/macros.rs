macro_rules! define_specific_enum {
    {
        $doc: literal,
        $enum: ident,
        $int: ident,
        $error: ident,
        $tests: ident,
        $(($name: ident, $value: expr),)*
        $(Range($name2: ident ($low: expr, $high: expr)),)*
        $(Other($name3: ident))*
    } => {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        #[doc = $doc]
        pub enum $enum {
            $( $name, )*
            $( $name2($int), )*
            $( $name3($int), )*
        }

        impl $enum {
            pub(crate) const fn as_number(self) -> $int {
                match self {
                    $( Self::$name => $value, )*
                    $( Self::$name2(n) => n, )*
                    $( Self::$name3(n) => n, )*
                }
            }
        }

        impl TryFrom<$int> for $enum {
            type Error = crate::Error;
            fn try_from(n: $int) -> Result<Self, Self::Error> {
                match n {
                    $( $value => Ok(Self::$name), )*
                    $( $low..=$high => Ok(Self::$name2(n)), )*
                    $( n => Ok(Self::$name3(n)), )*
                    #[allow(unreachable_patterns)]
                    n => Err(crate::Error::$error(n)),
                }
            }
        }

        #[cfg(test)]
        mod $tests {
            use super::*;
            use ::arbtest::arbtest;

            impl<'a> ::arbitrary::Arbitrary<'a> for $enum {
                fn arbitrary(u: &mut ::arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
                    loop {
                        let number: $int = u.arbitrary()?;
                        if let Ok(value) = $enum::try_from(number) {
                            break Ok(value);
                        }
                    }
                }
            }

            #[test]
            fn test_symmetry() {
                arbtest(|u| {
                    let expected: $enum = u.arbitrary()?;
                    let actual: $enum = expected.as_number().try_into().unwrap();
                    assert_eq!(expected, actual);
                    Ok(())
                });
            }
        }
    };
}

pub(crate) use define_specific_enum;

macro_rules! define_infallible_enum {
    {
        $doc: literal,
        $enum: ident,
        $int: ident,
        $(($name: ident, $value: expr),)*
    } => {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        #[doc = $doc]
        pub enum $enum {
            $( $name, )*
            Other($int),
        }

        impl $enum {
            pub(crate) const fn as_number(self) -> $int {
                match self {
                    $( Self::$name => $value, )*
                    Self::Other(n) => n,
                }
            }
        }

        impl From<$int> for $enum {
            fn from(n: $int) -> Self {
                match n {
                    $( $value => Self::$name, )*
                    n => Self::Other(n),
                }
            }
        }

        #[cfg(test)]
        impl<'a> ::arbitrary::Arbitrary<'a> for $enum {
            fn arbitrary(u: &mut ::arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
                let number: $int = u.arbitrary()?;
                Ok($enum::from(number))
            }
        }
    };
}

pub(crate) use define_infallible_enum;
