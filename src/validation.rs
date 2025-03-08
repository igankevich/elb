use crate::Error;

pub fn validate_u32(word: u64, name: &'static str) -> Result<(), Error> {
    if word > u32::MAX as u64 {
        return Err(Error::TooBig(name));
    }
    Ok(())
}

pub fn align_is_valid(align: u64) -> bool {
    align <= 1 || align.checked_next_power_of_two() == Some(align)
}

