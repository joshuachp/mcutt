use super::packets::DecodeError;

pub(crate) fn read_u8(bytes: &[u8]) -> Result<(u8, &[u8]), DecodeError> {
    let ([byte], rest) = read_chunk::<1>(bytes)?;

    Ok((*byte, rest))
}

pub(crate) fn read_u16(bytes: &[u8]) -> Result<(u16, &[u8]), DecodeError> {
    let (length, rest) = read_chunk::<2>(bytes)?;
    let length = u16::from_be_bytes(*length);

    Ok((length, rest))
}

pub(crate) const fn read_exact(bytes: &[u8], length: usize) -> Result<(&[u8], &[u8]), DecodeError> {
    if length > bytes.len() {
        return Err(DecodeError::not_enough(bytes, length));
    }

    Ok(bytes.split_at(length))
}

pub(crate) fn read_chunk<const N: usize>(bytes: &[u8]) -> Result<(&[u8; N], &[u8]), DecodeError> {
    let (read, rest) = read_exact(bytes, N)?;

    read.try_into()
        .map(|read| (read, rest))
        .map_err(|_| DecodeError::not_enough(bytes, N))
}
