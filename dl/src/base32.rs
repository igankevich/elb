#![allow(missing_docs)]

pub const fn encoded_len(input_len: usize) -> usize {
    debug_assert!(input_len / 5 <= usize::MAX / 8);
    input_len / 5 * 8
        + match input_len % 5 {
            0 => 0,
            1 => 1,
            2 => 4,
            3 => 5,
            _ => 7,
        }
}

pub const fn decoded_len(input_len: usize) -> usize {
    input_len / 8 * 5
        + match input_len % 8 {
            0 => 0,
            1 => 1,
            2 | 3 => 2,
            4 => 3,
            _ => 4,
        }
}

pub fn encode_into(input: &[u8], output: &mut [u8]) {
    let input_len = input.len();
    let output_len = output.len();
    debug_assert!(encoded_len(input_len) <= output.len());
    let aligned_input_len = input_len / 5 * 5;
    let aligned_output_len = output_len / 8 * 8;
    let input_iter = (0..aligned_input_len).step_by(5);
    let output_iter = (0..aligned_output_len).step_by(8);
    for (i, j) in output_iter.zip(input_iter) {
        let a = input[j];
        let b = input[j + 1];
        let c = input[j + 2];
        let d = input[j + 3];
        let e = input[j + 4];
        output[i] = CHARS[(a & 0b11111) as usize]; // 5 bits
        output[i + 1] = CHARS[((a >> 5) | ((b & 0b11) << 3)) as usize]; // 3 + 2 bits
        output[i + 2] = CHARS[((b >> 2) & 0b11111) as usize]; // 5 bits
        output[i + 3] = CHARS[((b >> 7) | ((c & 0b1111) << 1)) as usize]; // 1 + 4 bits
        output[i + 4] = CHARS[((c >> 4) | ((d & 0b1) << 4)) as usize]; // 4 + 1 bits
        output[i + 5] = CHARS[((d >> 1) & 0b11111) as usize]; // 5 bits
        output[i + 6] = CHARS[((d >> 6) | ((e & 0b111) << 2)) as usize]; // 2 + 3 bits
        output[i + 7] = CHARS[(e >> 3) as usize]; // 5 bits
    }
    let remaining = input_len - aligned_input_len;
    if remaining == 0 {
        return;
    }
    let i = aligned_output_len;
    let j = aligned_input_len;
    let a = input[j];
    output[i] = CHARS[(a & 0b11111) as usize]; // 5 bits
    if remaining == 1 {
        return;
    }
    let b = input[j + 1];
    let c = input.get(j + 2).copied().unwrap_or(0);
    output[i + 1] = CHARS[((a >> 5) | ((b & 0b11) << 3)) as usize]; // 3 + 2 bits
    output[i + 2] = CHARS[((b >> 2) & 0b11111) as usize]; // 5 bits
    output[i + 3] = CHARS[((b >> 7) | ((c & 0b1111) << 1)) as usize]; // 1 + 4 bits
    if remaining == 2 {
        return;
    }
    let d = input.get(j + 3).copied().unwrap_or(0);
    output[i + 4] = CHARS[((c >> 4) | ((d & 0b1) << 4)) as usize]; // 4 + 1 bits
    if remaining == 3 {
        return;
    }
    let e = input.get(j + 4).copied().unwrap_or(0);
    output[i + 5] = CHARS[((d >> 1) & 0b11111) as usize]; // 5 bits
    output[i + 6] = CHARS[((d >> 6) | ((e & 0b111) << 2)) as usize]; // 2 + 3 bits
}

pub fn decode_into(input: &[u8], output: &mut [u8]) {
    let input_len = input.len();
    let output_len = output.len();
    debug_assert!(decoded_len(input_len) <= output.len());
    let aligned_input_len = input_len / 8 * 8;
    let aligned_output_len = output_len / 5 * 5;
    let input_iter = (0..aligned_input_len).step_by(8);
    let output_iter = (0..aligned_output_len).step_by(5);
    for (i, j) in output_iter.zip(input_iter) {
        let a = char_index(input[j]);
        let b = char_index(input[j + 1]);
        let c = char_index(input[j + 2]);
        let d = char_index(input[j + 3]);
        let e = char_index(input[j + 4]);
        let f = char_index(input[j + 5]);
        let g = char_index(input[j + 6]);
        let h = char_index(input[j + 7]);
        output[i] = a | ((b & 0b111) << 5); // 5 + 3 bits
        output[i + 1] = (b >> 3) | (c << 2) | ((d & 0b1) << 6); // 2 + 5 + 1 bits
        output[i + 2] = (d >> 1) | ((e & 0b1111) << 4); // 4 + 4 bits
        output[i + 3] = (e >> 4) | (f << 1) | ((g & 0b11) << 6); // 1 + 5 + 2 bits
        output[i + 4] = (g >> 2) | (h << 3); // 3 + 5 bits
    }
    let remaining = input_len - aligned_input_len;
    if remaining == 0 {
        return;
    }
    let i = aligned_output_len;
    let j = aligned_input_len;
    let a = char_index(input[j]);
    let b = input.get(j + 1).copied().map(char_index).unwrap_or(0);
    output[i] = a | ((b & 0b111) << 5); // 5 + 3 bits
    if remaining == 1 {
        return;
    }
    let c = input.get(j + 2).copied().map(char_index).unwrap_or(0);
    let d = input.get(j + 3).copied().map(char_index).unwrap_or(0);
    output[i + 1] = (b >> 3) | (c << 2) | ((d & 0b1) << 6); // 2 + 5 + 1 bits
    if remaining == 2 || remaining == 3 {
        return;
    }
    let e = input.get(j + 4).copied().map(char_index).unwrap_or(0);
    output[i + 2] = (d >> 1) | ((e & 0b1111) << 4); // 4 + 4 bits
    if remaining == 4 {
        return;
    }
    let f = input.get(j + 5).copied().map(char_index).unwrap_or(0);
    let g = input.get(j + 6).copied().map(char_index).unwrap_or(0);
    output[i + 3] = (e >> 4) | (f << 1) | ((g & 0b11) << 6); // 1 + 5 + 2 bits
}

const fn char_index(ch: u8) -> u8 {
    let i = (ch >> 6) & 1;
    let j = (ch & 0b0011_1111) - 33;
    INDICES[i as usize][j as usize]
}

// Crockford's base32.
const CHARS: [u8; 32] = *b"0123456789abcdefghjkmnpqrstvwxyz";

const INDICES_1: [u8; 26] = [
    NA, NA, NA, NA, NA, NA, NA, NA, NA, NA, NA, NA, NA, NA, NA, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, NA,
];

const INDICES_2: [u8; 26] = [
    10, 11, 12, 13, 14, 15, 16, 17, NA, 18, 19, NA, 20, 21, NA, 22, 23, 24, 25, 26, NA, 27, 28, 29,
    30, 31,
];

const INDICES: [[u8; 26]; 2] = [INDICES_1, INDICES_2];

const NA: u8 = b'_';

#[cfg(test)]
mod tests {
    use super::*;
    use arbtest::arbtest;

    #[test]
    fn test_indices() {
        for (i, ch) in CHARS.iter().enumerate() {
            eprintln!("{i:03} {}", *ch as char);
            assert_eq!(i, char_index(*ch) as usize);
        }
    }

    #[test]
    fn test_encode() {
        let input = *b"hello";
        let mut output = [b'_'; encoded_len(5)];
        encode_into(&input, &mut output[..]);
        eprintln!("{}", std::str::from_utf8(&output[..]).unwrap());
        let mut decoded = [b'_'; 5];
        decode_into(&output, &mut decoded);
        eprintln!("{}", std::str::from_utf8(&decoded[..]).unwrap());
    }

    #[test]
    fn test_arbitrary() {
        arbtest(|u| {
            let input: Vec<u8> = u.arbitrary()?;
            let mut encoded = vec![b'_'; encoded_len(input.len())];
            encode_into(&input, &mut encoded);
            assert!(
                !encoded.contains(&b'_'),
                "input = {:?}, encoded = {:?}",
                input,
                std::str::from_utf8(&encoded)
            );
            let mut decoded = vec![b'_'; decoded_len(encoded.len())];
            decode_into(&encoded, &mut decoded);
            Ok(())
        });
    }
}
