macro_rules! define_specific_enum {
    {
        $enum:ident,
        $int:ident,
        $(($name:ident, $value:expr),)*
        $(Range($name2:ident ($low:expr, $high:expr)),)*
    } => {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        #[repr($int)]
        pub enum $enum {
            $( $name = $value, )*
            $( $name2($int), )*
        }

        impl $enum {
            pub(crate) const fn as_number(self) -> $int {
                match self {
                    $( Self::$name => $value, )*
                    $( Self::$name2(n) => n, )*
                }
            }

            pub fn validate(self) -> Result<(), std::io::Error> {
                match self {
                    $(
                        Self::$name2(n) => if !($low..=$high).contains(&n) {
                            return Err(
                                std::io::Error::other(concat!("Invalid ", stringify!($enum)))
                            );
                        }
                    )*
                    _ => {}
                }
                Ok(())
            }
        }

        impl TryFrom<$int> for $enum {
            type Error = std::io::Error;
            fn try_from(n: $int) -> Result<Self, Self::Error> {
                match n {
                    $( $value => Ok(Self::$name), )*
                    $( $low..=$high => Ok(Self::$name2(n)), )*
                    _ => Err(std::io::ErrorKind::InvalidData.into()),
                }
            }
        }

        #[cfg(test)]
        impl<'a> arbitrary::Arbitrary<'a> for $enum {
            fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
                loop {
                    let number: $int = u.arbitrary()?;
                    if let Ok(value) = $enum::try_from(number) {
                        break Ok(value);
                    }
                }
            }
        }
    };
}

pub(crate) use define_specific_enum;
