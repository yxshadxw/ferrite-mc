//! Skipping network NBT (a nameless root tag) without building a tree, to locate the
//! fields that follow it.

use ferrite_protocol::DecodeError;
use ferrite_protocol::codec::take;

const MAX_DEPTH: usize = 512;

/// Advance `buf` past one network NBT value (`TAG_End` alone means "no value").
pub fn skip(buf: &mut &[u8]) -> Result<(), DecodeError> {
    let tag = byte(buf)?;
    if tag == 0 {
        return Ok(());
    }
    payload(buf, tag, 0)
}

fn byte(buf: &mut &[u8]) -> Result<u8, DecodeError> {
    Ok(take(buf, 1)?[0])
}

fn len(buf: &mut &[u8]) -> Result<usize, DecodeError> {
    let n = i32::from_be_bytes(take(buf, 4)?.try_into().unwrap());
    usize::try_from(n).map_err(|_| DecodeError::Invalid("negative NBT length"))
}

fn payload(buf: &mut &[u8], tag: u8, depth: usize) -> Result<(), DecodeError> {
    if depth > MAX_DEPTH {
        return Err(DecodeError::Invalid("NBT too deep"));
    }
    match tag {
        1 => drop(take(buf, 1)?),
        2 => drop(take(buf, 2)?),
        3 | 5 => drop(take(buf, 4)?),
        4 | 6 => drop(take(buf, 8)?),
        7 => {
            let n = len(buf)?;
            take(buf, n)?;
        }
        8 => {
            let n = u16::from_be_bytes(take(buf, 2)?.try_into().unwrap()) as usize;
            take(buf, n)?;
        }
        9 => {
            let element = byte(buf)?;
            let n = len(buf)?;
            if element == 0 && n > 0 {
                return Err(DecodeError::Invalid("NBT list of TAG_End"));
            }
            for _ in 0..n {
                payload(buf, element, depth + 1)?;
            }
        }
        10 => loop {
            let element = byte(buf)?;
            if element == 0 {
                break;
            }
            let n = u16::from_be_bytes(take(buf, 2)?.try_into().unwrap()) as usize;
            take(buf, n)?;
            payload(buf, element, depth + 1)?;
        },
        11 => {
            let n = len(buf)?;
            take(buf, n.checked_mul(4).ok_or(DecodeError::Eof)?)?;
        }
        12 => {
            let n = len(buf)?;
            take(buf, n.checked_mul(8).ok_or(DecodeError::Eof)?)?;
        }
        _ => return Err(DecodeError::Invalid("unknown NBT tag")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_nested_compound_and_stops_at_following_field() {
        // {a: [1L, 2L], b: {c: "hi"}, l: [I; 7]} followed by 0xAB.
        let mut data = vec![10];
        data.extend_from_slice(&[12, 0, 1, b'a', 0, 0, 0, 2]);
        data.extend_from_slice(&[0; 16]);
        data.extend_from_slice(&[10, 0, 1, b'b', 8, 0, 1, b'c', 0, 2, b'h', b'i', 0]);
        data.extend_from_slice(&[9, 0, 1, b'l', 3, 0, 0, 0, 1, 0, 0, 0, 7]);
        data.push(0);
        data.push(0xAB);
        let mut buf = data.as_slice();
        skip(&mut buf).unwrap();
        assert_eq!(buf, [0xAB]);
    }

    #[test]
    fn end_tag_is_empty_value_and_truncation_errors() {
        let mut buf: &[u8] = &[0, 1];
        skip(&mut buf).unwrap();
        assert_eq!(buf, [1]);
        let mut buf: &[u8] = &[8, 0, 5, b'x'];
        assert!(skip(&mut buf).is_err());
    }
}
