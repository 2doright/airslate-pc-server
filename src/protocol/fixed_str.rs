use crate::protocol::error::ProtocolError;

pub fn encode_fixed_utf8<const N: usize>(value: &str) -> [u8; N] {
    let mut out = [0_u8; N];
    let mut written = 0;

    for ch in value.chars() {
        let mut buf = [0_u8; 4];
        let encoded = ch.encode_utf8(&mut buf).as_bytes();
        if written + encoded.len() > N {
            break;
        }
        out[written..written + encoded.len()].copy_from_slice(encoded);
        written += encoded.len();
    }

    out
}

pub fn decode_fixed_utf8<const N: usize>(
    field: &'static str,
    bytes: &[u8; N],
) -> Result<String, ProtocolError> {
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(N);

    if bytes[end..].iter().any(|byte| *byte != 0) {
        return Err(ProtocolError::InvalidPadding { field });
    }

    let value =
        std::str::from_utf8(&bytes[..end]).map_err(|_| ProtocolError::InvalidUtf8 { field })?;
    Ok(value.to_string())
}
