#![allow(missing_docs)]

use alloc::vec::Vec;
use arbitrary::Arbitrary;
use arbitrary::Unstructured;
use arbtest::arbtest;
use core::fmt::Debug;

use crate::BlockRead;
use crate::BlockWrite;
use crate::ByteOrder;
use crate::Class;
use crate::EntityIo;

pub fn test_entity_io<T>()
where
    T: EntityIo + for<'a> ArbitraryWithClass<'a> + Debug + PartialEq + Eq,
{
    arbtest(|u| {
        let byte_order: ByteOrder = u.arbitrary()?;
        let class: Class = u.arbitrary()?;
        let expected: T = T::arbitrary(u, class)?;
        let mut buf = Vec::new();
        expected
            .write(&mut buf, class, byte_order)
            .inspect_err(|e| panic!("Failed to write {:#?}: {e}", expected))
            .unwrap();
        let actual = T::read(&mut &buf[..], class, byte_order)
            .inspect_err(|e| panic!("Failed to read {:#?}: {e}", expected))
            .unwrap();
        assert_eq!(expected, actual);
        Ok(())
    });
}

pub fn test_block_io<T>()
where
    T: BlockRead + BlockWrite + for<'a> ArbitraryWithClass<'a> + Debug + PartialEq + Eq,
{
    arbtest(|u| {
        let byte_order: ByteOrder = u.arbitrary()?;
        let class: Class = u.arbitrary()?;
        let expected: T = T::arbitrary(u, class)?;
        let mut buf = Vec::new();
        expected
            .write(&mut buf, class, byte_order)
            .inspect_err(|e| panic!("Failed to write {:#?}: {e}", expected))
            .unwrap();
        let len = buf.len() as u64;
        let actual = T::read(&mut &buf[..], class, byte_order, len)
            .inspect_err(|e| panic!("Failed to read {:#?}: {e}", expected))
            .unwrap();
        assert_eq!(expected, actual);
        Ok(())
    });
}

pub trait ArbitraryWithClass<'a> {
    fn arbitrary(u: &mut Unstructured<'a>, class: Class) -> arbitrary::Result<Self>
    where
        Self: Sized;
}

impl<'a, T: Arbitrary<'a>> ArbitraryWithClass<'a> for T {
    fn arbitrary(u: &mut Unstructured<'a>, _class: Class) -> arbitrary::Result<Self> {
        u.arbitrary()
    }
}
