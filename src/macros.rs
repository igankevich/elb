macro_rules! define_infallible_enum {
    {
        $doc: literal,
        $enum: ident,
        $int: ident,
        $(($name: ident, $value: expr$(, $doc1: literal)*),)*
    } => {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        #[doc = $doc]
        pub enum $enum {
            $(
                $(#[doc = $doc1])*
                $name,
            )*
            /// Other.
            Other($int),
        }

        impl $enum {
            #[inline]
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
            fn arbitrary(u: &mut ::arbitrary::Unstructured<'a>) -> ::arbitrary::Result<Self> {
                let number: $int = u.arbitrary()?;
                Ok($enum::from(number))
            }
        }
    };
}

pub(crate) use define_infallible_enum;

macro_rules! define_enum_v2 {
    {
        $doc: literal,
        $enum: ident,
        $int: ident,
        $(($name: ident, $value: expr$(, $doc1: literal)*),)*
    } => {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        #[doc = $doc]
        pub enum $enum {
            $(
                $(#[doc = $doc1])*
                $name,
            )*
            /// Other.
            Other($int),
        }

        impl $enum {
            #[inline]
            pub(crate) const fn as_number(self) -> $int {
                match self {
                    $( Self::$name => $value, )*
                    Self::Other(n) => n,
                }
            }

            pub(crate) fn from(n: $int) -> Self {
                match n {
                    $( $value => Self::$name, )*
                    n => Self::Other(n),
                }
            }
        }
    };
}

pub(crate) use define_enum_v2;
